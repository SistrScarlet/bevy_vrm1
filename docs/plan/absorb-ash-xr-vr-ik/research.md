# batch-2: VR IK の VRM 知識層移管 — 調査結果

- 日付: 2026-07-12
- 移管元: ash_xr `crates/bevy_ash_openxr/src/ik/` (mod.rs 94 行 / calibration.rs 279 行 / solver.rs 444 行 / systems.rs 527 行)
- 移管先: bevy_vrm1 (本 worktree、branch `feat/absorb-ash-xr-vr-ik`)

## 移管元の構成 (ash_xr `ik/`)

### mod.rs — コンポーネント + Plugin

- `VrIk` (Component): チューニングパラメータ。`spine_weights: [f32; 4]` (default `[0.15, 0.2, 0.25, 0.4]`)、`pole_bias_down: f32` (0.6)、`extension_blend_start: f32` (0.7)。**注意: `pole_bias_down` / `extension_blend_start` は現在どこからも読まれていない** (grep 済み — solver にも systems にも参照なし。POC の名残)
- `IkChainCache` (Component): rest-pose から計算した骨格寸法キャッシュ。`upper_arm_len/lower_arm_len: (f32, f32)` (L, R)、`hip_offset: Vec3`、`spine_chain_len: f32` (IK 計算では未使用、init 時の info! ログでのみ読まれる — フィールドを落とすならログも連動)、`shoulder_offset: (Vec3, Vec3)`、`hip_height_ratio: f32`、`legs: Option<LegChainCache>`
- `LegChainCache`: `upper_leg_len/lower_leg_len: (f32, f32)`、`upper_leg_offset/foot_offset: (Vec3, Vec3)` (hips 基準)
- `FootStepState` (Component): 歩行サイクル runtime state (phase/stride/motion_dir_stage)。**work-map 判断: アプリ側残置**
- `IkPlugin`: PostUpdate に 3 システム登録。`.in_set(VrmSystemSets::GazeControl)` + `run_if(any_with_component::<VrIk>)` が付くのは `apply_vr_ik` のみ。`init_ik_chain_cache` / `update_foot_step` はどの set にも属さない素の PostUpdate システム (`.after` エッジのみ) → **init 系の set 配置は vrm1 側で新たに決める設計項目**

### calibration.rs — 純粋関数

- `build_ik_chain_cache(19 個の Vec3/Option<Vec3>) -> IkChainCache`: rest-pose world 座標から寸法抽出。純幾何、panic なし
  - `hip_height_ratio = hips.y / head.y` (head.y ≤ 0.01 なら 0.6 フォールバック)
  - spine chain: hips → spine? → chest? → neck? → head、None はスキップして隣接加算
  - shoulder_offset: shoulder 無しは upper_arm で代替、head 基準
  - 脚 6 引数が全て Some のときのみ `legs = Some(..)`
- テスト 2 件 (basic / optional bones missing) — そのまま移植可能

### solver.rs — 純粋関数 (bevy_math のみ)

移管対象 (VRM 知識 or 汎用 IK):
- `two_bone_ik(shoulder, target, upper_len, lower_len, pole) -> (Quat, Quat)`: 余弦定理 2 ボーン解析 IK。規約 `rotation * Vec3::Y = bone direction`。到達不能距離は clamp、退化入力で NaN を出さない
- `bone_rotation(dir, pole) -> Quat` (private): pole から side 軸を作る gimbal 回避
- `estimate_hip(hmd_t, hmd_r, hip_height_ratio, hip_xz_offset) -> (Vec3, Quat)`: HMD から腰位置・yaw 推定。`hip.y = hmd.y * ratio` で体長差吸収
- `distribute_spine(hip_rot, head_rot, weights: &[f32; 4]) -> [(yaw, pitch); 4]`: 腰→頭の回転差分を yaw/pitch 分解して 4 骨に加重分配。**pitch 反転は VRM +Z 前方知識** (doc コメント参照)

アプリ側残置 (歩行サイクル、work-map 判断に従う):
- `step_foot_offsets(phase, stride, step_height)` / `advance_step_cycle(...)` / 定数 `STEP_*` 6 個
- テスト: two_bone_ik 4 件 / estimate_hip 3 件 / distribute_spine 3 件は移植、step 系 8 件は残置

### systems.rs — ECS システム

- `bone_name` モジュール: VRM 1.0 humanoid bone 名 (camelCase) の SSOT 定数 19 個。「vrm1 は骨名定数を export しない」ことへの防御 → **vrm1 側に移すなら公開定数化する価値あり** (typo → silent 不発の防止が動機)
- `init_ik_chain_cache`: `With<VrIk>, Without<IkChainCache>` の entity に対し `HumanoidBoneEntities::find` + `RestGlobalTransform` で rest 位置を引き、`build_ik_chain_cache` して `IkChainCache` + `FootStepState` を insert。必須骨 8 個 (head/hips/両腕 3×2)、無ければ次フレーム自動リトライ。骨名が map に無い場合は `warn_once` (malformed VRM / typo 検知)
  - **PERF_SNIFF 1Hz 計測ログ (`Local<Option<Instant>>` + `Local<u32>`) は ash_xr の診断用 — 移管しない**
- `update_foot_step`: `PlayerVelocity` + `XrSceneOffset` (ash_xr 固有 Resource) から FootStepState 更新 → **アプリ側残置**
- `apply_vr_ik`: 毎フレーム 4 段階
  1. **Hip**: `estimate_hip` → hips の `Transform.translation/rotation` を直接書く。`model_flip = Ry(PI)` (VRM +Z 前方 ↔ Bevy -Z 前方) を hips rotation に合成
  2. **Spine 分配**: `distribute_spine` の delta を `rest_tf.rotation * from_euler(YXZ, yaw, pitch, 0)` で spine/chest/neck/head に適用 (optional 骨は delta 捨て)
  3. **Arm IK** (`apply_arm_ik`): コントローラごとに独立。shoulder world = hmd + hmd_rot * (model_flip * shoulder_offset)。bone axis 補正 = lower_arm の rest local translation から導出 (`from_rotation_arc(bone_axis, Y)`)。upper arm の world→local は「rest parent + model_flip」近似。hand は controller rotation + `from_rotation_arc(bone_axis, NEG_Z)` 補正。pole = `Vec3::NEG_Y` 固定
  4. **Leg IK** (`apply_leg_ik`): 脚キャッシュ + 脚骨 6 本が揃うときのみ。foot target = hip 位置 + 回転済み foot_offset + 歩行オフセット (FootStepState 由来、無ければゼロ)。foot target の Y = step_height (**床 = y0 前提**)。pole = hip 前方。bone axis 補正は lower_leg rest translation から (フォールバック `NEG_Y`)。foot 骨は書かない (lower_leg 子として追従、POC 品質)

### 入力境界 (ash_xr 固有 → 切断面)

- `HmdPoseResource` / `LeftControllerPoseResource` / `RightControllerPoseResource`: 毎フレーム publish される Resource。中身は `{ translation: Vec3, rotation: Quat }` (OpenXR reference space 原点基準の raw pose)。Resource 不在 = 未接続 → apply は early return (HMD)、腕は片側スキップ
- **歩行オフセット (第 3 の切断面)**: 移管対象の `apply_vr_ik` 自身が残置対象に直接依存している (`step_states: Query<&FootStepState>` + `step_foot_offsets` + `STEP_HEIGHT`、systems.rs:211,342-344)。FootStepState を宣言どおりアプリ側に残すなら、足オフセット (左右の XZ オフセット + 足上げ高さ) を外部入力として受ける形に切断する設計判断が必要 (汎用 component 化 / 入力構造体のフィールド化 / vrm1 版では省く、のいずれか)
- `PlayerVelocity` / `XrSceneOffset`: `update_foot_step` (残置) の入力 → 切断不要
- **座標空間の暗黙前提**: `apply_vr_ik` は hips の local `Transform` に world 座標を直接書く = **VRM root〜hips 親までの ancestor が identity である前提** (ash_xr ではローカルプレイヤー VRM が stage 原点に居る)。切断面ドキュメントに明記が必要

### ash_xr 側の利用箇所 (adopt branch で置換するもの)

- `lib.rs:57` re-export、`bevy_ash_event_client/src/main.rs:96` `IkPlugin` 追加 + `:640` `VrIk::default()` insert、`examples/schedule_dump.rs`
- work-map 記載の「`apply_vr_ik` の 16 個別 `*BoneEntity` query → `HumanoidBoneEntities` 移行」は **ash_xr 側で対応済み** (`c5ff4501` の code-review 対応。現行コードは全て `HumanoidBoneEntities::find`)

## 移管先 (vrm1) の受け皿

- `HumanoidBoneEntities` (`src/vrm/humanoid_bone.rs:50`): `find(&str) -> Option<Entity>`。VRM root entity に一括 insert 済み
- `RestTransform` / `RestGlobalTransform` (`src/vrm.rs:101,108`): 骨初期化時に insert。IK の rest 位置・bone axis 導出の入力
- SystemSet chain (`src/vrm.rs:163-178`): `Constraints → PropagateAfterConstraints → GazeControl → BoneOverlaySystems → Expressions → PropagateAfterExpressions → SpringBone → DetermineRedraw` を `configure_sets(...).chain().after(AnimationSystems).before(TransformSystems::Propagate)` で担保。**空 set への `.after()` はエッジを生まないため、新 set は chain への組込が必須** (BoneOverlaySystems 前例)
- `bone_overlay.rs`: 「新 SystemSet を chain に差し込み、専用 Plugin を `VrmCorePlugin` に追加、純粋関数 + ユニットテスト」という直近の吸収パターンの手本
- ash_xr の `apply_vr_ik` は現在 `.in_set(VrmSystemSets::GazeControl)` (LookAt と同 set、set 内順序は未定義。`BodyTracking` も head-chain 回転を書く同 set の潜在的二重書き手)
- vrm1 の feature: `serde` / `log` / `develop`。IK は bevy_math + ECS のみで重い依存なし

## 制約・既知の落とし穴

1. **additive API 原則** (CLAUDE.md): 既存 API・デフォルト挙動を変えない。マージ順 vrm1 → ash_xr、vrm1 マージ後に ash_xr main (旧 ik/ 保持) がビルドできること — ik/ は自己完結なので新規追加のみなら自動的に満たされる
2. **chain 組込**: 新 set を `configure_sets` chain に足すと既存 smoke test (`vrm_core_plugin_schedule_builds_without_cycles`) が順序 cycle を検出できる
3. **model_flip (Ry(PI)) は「bevy_vrm1 が glTF を座標変換せずロードする」ことへの補正** = vrm1 の内部知識。移管でこの知識が vrm1 に閉じるのが本 batch の主目的の 1 つ
4. **VrIk の未使用フィールド** `pole_bias_down` / `extension_blend_start` を API にそのまま持ち込むか要設計判断
5. **床 y=0 前提** (leg IK の foot target Y) と **ancestor identity 前提** (hips への world 座標直書き) は POC 品質の制約として引き継ぐ (ドキュメント化)
6. bevy_vrm1 のビルドは `make check` 等 (共有ビルドロック) 経由

## 関連ファイル

- ash_xr: `crates/bevy_ash_openxr/src/ik/{mod,calibration,solver,systems}.rs`、`src/boundary/{head_pose,controller_pose}.rs`
- vrm1: `src/vrm.rs` (plugin 構成 + chain)、`src/system_set.rs`、`src/vrm/humanoid_bone.rs`、`src/vrm/bone_overlay.rs` (吸収パターン手本)
