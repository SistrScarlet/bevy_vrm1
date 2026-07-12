# batch-2: VR IK の VRM 知識層移管 — 論理設計

- 日付: 2026-07-12
- 前提: `research.md` (同ディレクトリ)。設計原則 = additive (既存 API・デフォルト挙動不変)

## スコープ宣言

vrm1 に移管するのは **VRM 知識を含む IK 本体**: 2 ボーン解析 IK・腰推定・スパイン分配 (solver)、rest-pose キャリブレーション (calibration)、骨への適用 (model_flip / bone axis 補正 / rest 基準の local 変換)。

アプリ側 (ash_xr) に残すのは **入力の生成**: HMD/コントローラ pose の取得、歩行サイクル (`FootStepState` / `advance_step_cycle` / `step_foot_offsets` / `STEP_*` 定数)、`PlayerVelocity`・`XrSceneOffset` 由来の計算。

## 振る舞い

### 有効化

ユーザーは VRM entity (VrmHandle を挿した entity) に 2 つの component を挿す:

1. `VrIk` — チューニング設定 (`Default` あり)。挿すと IK が有効化される
2. `VrIkTargets` — 毎フレームの外部ポーズ入力。アプリが毎フレーム書く

`VrIk` は `#[require(VrIkTargets)]` で `VrIkTargets` を required component にする (挿し忘れによる silent 不発を型で防ぐ。実質「component 1 つで有効化」)。`VrIkChainCache` は自動挿入される (後述)。プラグイン追加は不要 (`VrmCorePlugin` に組込み。IK 未使用アプリのコストは `run_if(any_with_component::<VrIk>)` のみ)。

### 入力 (VrIkTargets) — 切断面

```rust
#[derive(Component, Default)]
pub struct VrIkTargets {
    /// 頭部ターゲット (HMD pose)。None = IK 全体を不発 (未接続相当)
    pub head: Option<VrIkPose>,
    /// 左手ターゲット。None = 左腕 IK をスキップ
    pub left_hand: Option<VrIkPose>,
    /// 右手ターゲット。None = 右腕 IK をスキップ
    pub right_hand: Option<VrIkPose>,
    /// 歩行オフセット (アプリ側の歩行サイクルが計算)。デフォルト = 全ゼロ
    /// (足首 joint は床 y=0 直上・rest XZ オフセット位置に接地。rest の足首高さは
    /// 使われない — 移管元アルゴリズム踏襲)
    pub foot_step: VrIkFootStep,
}

pub struct VrIkPose { pub translation: Vec3, pub rotation: Quat }

#[derive(Default)]
pub struct VrIkFootStep {
    pub left_offset_xz: Vec3,   // 移動方向 × ストライド (XZ 平面。Y 成分は無視される)
    pub left_height: f32,       // 足上げ高さ (foot target の Y 絶対値になる)
    pub right_offset_xz: Vec3,
    pub right_height: f32,
}
```

- 座標空間の契約: **VRM の骨階層が living する空間** (= hips の祖先が identity である前提の world 座標。ash_xr では OpenXR reference space = stage 空間)。この ancestor-identity 前提は POC 品質の制約としてドキュメント化する
- `VrIkPose` は自前 struct とする (`Isometry3d` は Vec3A で利用側に摩擦、`Transform` は scale が無意味)

### 設定 (VrIk)

```rust
#[derive(Component)]
pub struct VrIk {
    /// spine → chest → neck → head への回転分配の重み
    pub spine_weights: [f32; 4],  // default [0.15, 0.2, 0.25, 0.4]
}
```

- ash_xr 版の未使用フィールド `pole_bias_down` / `extension_blend_start` は**持ち込まない** (新 API なので不要物を輸入しない。将来必要になったら additive に足す)

### キャリブレーション (VrIkChainCache 自動挿入)

- `VrIk` があり `VrIkChainCache` が無い entity に対し、毎フレーム `HumanoidBoneEntities` + `RestGlobalTransform` から寸法を計算して挿入 (骨 spawn 完了まで自動リトライ)
- 必須骨 8 個 (head / hips / 両腕の upperArm・lowerArm・hand): `HumanoidBoneEntities` に名前ごと無い場合は warn_once (malformed VRM の検知)、`RestGlobalTransform` 未 spawn は silent リトライ — ash_xr の挙動を踏襲
- 脚骨 6 個が揃わない VRM は腕・スパインのみ動作 (`legs: None`)
- フィールドは pub (観測可能)。`spine_chain_len` は IK 計算で未使用のため**持ち込まない** (連動していた init info ログも簡素化)
- `VrIk` を remove しても cache は残す (同一モデルなら rest pose 由来で不変なため無害。再 insert で再利用)
- **既知の制約**: 同一 entity で VRM を差し替えた場合 (detach → 再ロード)、旧寸法の cache が残る。POC 制約としてドキュメント化 (将来 detach 連動の cache 除去を検討)。また骨欠損の warn_once は callsite 単位のため 2 体目以降の malformed VRM は警告されない (ash_xr 踏襲)

### 毎フレーム適用 (apply)

ash_xr `apply_vr_ik` の 4 段階をそのまま移管 (アルゴリズム変更なし):

1. Hip: `estimate_hip` → hips の Transform に直書き (`model_flip = Ry(PI)` 合成 — VRM +Z 前方の知識は vrm1 内部に閉じる)
2. スパイン分配: `distribute_spine` → rest rotation 基準で spine/chest/neck/head へ適用 (optional 骨の delta は捨てる)
3. 腕 IK: 左右独立。`VrIkTargets.left_hand/right_hand` が `Some` の側のみ
4. 脚 IK: `legs` cache と脚骨 6 本が揃うときのみ。foot target Y = `foot_step.*_height` (床 y=0 前提、POC 制約としてドキュメント化)。foot 骨は書かない (lower_leg 追従)

異常系:
- `head: None` → その entity はスキップ (何も書かない)
- 骨 Transform 取得失敗 → その骨だけスキップ
- graceful fallback 優先、panic なし (solver は NaN を出さない、ash_xr のテストで保証済み)

## 実行順 (SystemSet) — 切断面

新しい公開 SystemSet `VrIkSystems` を `VrmCorePlugin` の chain に組み込む:

```
AnimationSystems → VrIkSystems → Constraints → PropagateAfterConstraints
    → GazeControl → BoneOverlaySystems → Expressions → … → SpringBone
```

- **ash_xr 版 (`GazeControl` 内) から変更**: IK は「humanoid ポーズを書く側」なので VRM spec 更新順 (animation → constraints → gaze → …) の animation 直後に置く。これにより:
  - Node constraint (twist 骨等) が IK ポーズを同フレームで反映 (ash_xr 版は 1 frame 遅延だった)
  - `PropagateAfterConstraints` が IK の書いた Transform を propagate するため、LookAt / SpringBone も同フレームの IK ポーズ基準で動く (ash_xr 版より改善)
  - LookAt / BodyTracking との同 set 内順序不定が解消 (IK が先、gaze 系が上書き)
- IK は現フレームの `GlobalTransform` を読まない (`RestGlobalTransform` と入力のみ) ので、chain 先頭配置に propagation 依存はない
- 空 set への `.after()` はエッジを生まないため、順序は chain 組込みでのみ担保 (BoneOverlaySystems 前例)
- cache 初期化システムも `VrIkSystems` 内で apply の前に chain
- LookAt (Bone モード) / `BodyTracking` が同一 entity で有効な場合、head 回転は gaze 系が IK の上に上書きする。併用の可否はアプリの責任 (ドキュメント化)
- **VRMA との共存**: `head: Some` の間、IK 管理骨 (hips / spine 系最大 4 骨 / 腕 3×2 骨 / 脚 2×2 骨) は VRMA アニメーション出力を毎フレーム上書きする (VR embodiment では IK が勝つのが意図)。IK が書かない骨 (指・foot・つま先・目・shoulder) と expression は VRMA 側が生きる部分共存。`head: None` にすると全面的に animation へフォールバック

## Feature gate

**設けない**。純 bevy_math + ECS で依存追加なし、未使用時コストは `run_if` の存在チェックのみ。gate は API 表面を複雑にするだけ。

## 公開 API まとめ

`src/vrm/vr_ik.rs` (+ サブモジュール、物理配置は TDD で決定):

- Component: `VrIk`, `VrIkTargets` (+ `VrIkPose`, `VrIkFootStep`), `VrIkChainCache` (+ `VrIkLegChainCache`)
- SystemSet: `VrIkSystems`
- 純粋関数 (pub、アプリの拡張・検証用): `two_bone_ik`, `estimate_hip`, `distribute_spine`, `build_ik_chain_cache` (ash_xr 版は位置引数 19 個 — pub API 化にあたり入力 struct 化を TDD フェーズで検討。`Option<Vec3>` の並び間違いがコンパイルを通るため)
- 骨名定数 `pub mod bone_names` (`"hips"`, `"leftUpperArm"` 等 19 個 + IK 外でも有用な全 humanoid 骨名): typo → silent 不発の防止 SSOT。`humanoid_bone` モジュール側に置く (IK 専用ではないため)
- 上記を `vrm::prelude` に追加 (bone_overlay と同様)

## 利用パターン (ash_xr adopt branch の想定)

```rust
// spawn 時
commands.entity(vrm).insert((VrIk::default(), VrIkTargets::default()));

// 毎フレーム (Update): boundary Resource → VrIkTargets へ転写
fn feed_ik_targets(
    hmd: Option<Res<HmdPoseResource>>, left: ..., right: ...,
    mut targets: Query<(&mut VrIkTargets, &FootStepState)>,  // FootStepState は ash_xr 残置
) {
    for (mut t, step) in &mut targets {
        t.head = hmd.as_ref().map(|h| VrIkPose { translation: h.0.translation, rotation: h.0.rotation });
        t.left_hand = ...; t.right_hand = ...;
        let ((l_along, l_h), (r_along, r_h)) = step_foot_offsets(step.phase, step.stride, STEP_HEIGHT);
        t.foot_step = VrIkFootStep { left_offset_xz: step.motion_dir_stage * l_along, left_height: l_h, ... };
    }
}
```

ash_xr の `ik/` からは solver の step 系関数と `FootStepState` + `update_foot_step` だけが残り、`IkPlugin` は削除される (adopt branch)。

## TDD からの発見

- **`build_vr_ik_chain_cache` の入力 struct 化を採用** (`VrIkRestPositions`、名前付きフィールド 19 個)。ash_xr 版の位置引数 19 個は `Option<Vec3>` の並び間違いがコンパイルを通るため pub API には不適だった
- **ECS テストの fixture は平坦 entity 群で十分**: IK システムは propagation を使わず `RestTransform`/`RestGlobalTransform` しか読まないため、骨の親子関係 (`ChildOf`) を組む必要がなかった。world 座標の検証は rest 階層 (identity 回転) からの手動合成で行い、TransformPlugin も不要
- **`VrIkPlugin` は素の `App::new()` で動く**: asset / scene 依存がないため ECS テストが軽量 (`MinimalPlugins` すら不要)
- **change detection の非汚染検証**: `head: None` テストは `Changed<Transform>` を数える probe システムを `VrIkSystems` の後に置く方式で観測可能だった
