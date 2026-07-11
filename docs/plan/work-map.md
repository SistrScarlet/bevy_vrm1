# bevy_vrm1 作業マップ

- 作成: 2026-07-11
- 性質: fork (SistrScarlet/bevy_vrm1) の branch 状態 + 残作業の全景 (進行 SSOT)。bevy_ash_xr の `first-stage-work-map.md` + `first-stage-branch-plan.md` の縮小統合版
- 更新: branch の起票 / 完了 (merge) / セッション引き継ぎ時に更新する。セッションを跨ぐ作業は `/clear` 前に必ずここへ反映

## 運用

- 開発トランク = main ([[project_fork_policy]])。作業は feature branch 単位 (worktree 推奨)
- **マージ規律**: merge 前 `/code-review` 必須 + user 承認後に `merge-branch` skill (CLAUDE.md § 開発体制参照)
- bevy_ash_xr 連動変更の手順も CLAUDE.md § 開発体制参照

---

## Active Branch

### feat/absorb-ash-xr-vrm-generics (batch-1: ash_xr の VRM 汎用処理吸収)

- **状態: `/code-review` 実施済み + finding 全 10 件解消済み = user マージ承認待ち** (2026-07-11)。main 未マージ (一度 ff マージしたが `/code-review` 未実施のため取り消し済)
- commit: `d16e0ee` (実装) + `9badf80` (review fix) + `2889988` (ビルドロック導入)
- `/code-review` (high、8 finder + verify) の主要 finding と修正 (詳細は `9badf80` のコミットメッセージ):
  - **BoneOverlay 伝播順序 (CONFIRMED)**: VRM set chain が bevy 標準 propagation と無順序で、overlay が描画に現れない可能性 + SpringBone が overlay を見られない → chain に `BoneOverlaySystems` を組込 + `.before(TransformSystems::Propagate)` + overlay アクティブ時のみの条件付き propagation を `PropagateAfterExpressions` に追加 (fork の性能最適化は非アクティブ時維持)
  - **NaN weight (CONFIRMED)**: clamp を素通りして Transform を汚染 → 非有限値ガード
  - **detach 残留 (CONFIRMED)**: `HumanoidBoneEntities` が除去リスト漏れ → 追加
  - capsule_fit 幾何 3 件 / hips unwrap panic / 無警告の部分初期化 / MToon reflect 登録の二重リスト / `is_auto` 負論理 → 全て修正
- **ビルドロック** (`2889988`): ash_xr と同一の `/tmp/bevy_ash_xr-build.lock` を flock する Makefile を導入 (WSL RAM 対策、ビルドは `make check/clippy/test/build` 経由)。flock-make 自己デッドロック防止 hook も ash_xr と同一のものを `.claude/settings.json` に追加
- 内容 (全て追加のみ、既存 API・デフォルト挙動不変):
  - `VrmCorePlugin`: MToon (wgpu MaterialPlugin) 抜きコア構成。`VrmPlugin` = `VrmCorePlugin` + `MtoonMaterialPlugin` の合成に。MToon-less 経路の reflect 登録 (`MeshMaterial3d<StandardMaterial>` 含む) も core が担う
  - `VrmExpressionPlugin` に `InheritWeightSystems.after(Expressions)` の順序エッジ
  - `HumanoidBoneEntities`: bone 名 → entity 一括マップ (VRM ルートに自動挿入)。`new_type` (String) に `Borrow<str>` 実装
  - `humanoid_bone::capsule_fit`: 骨位置 → カプセル近似 (物理エンジン非依存)
  - `bone_overlay`: `BoneRotationOverlay` / `BoneOverlaySystems` (GazeControl 後・SpringBone 前の加算回転)
  - `ExpressionCategory::is_auto` / `ExpressionEntityMap::manual_expression_names`
- 検証済: `cargo check` / `clippy` (warning 0) / `cargo test --lib` 66 件 green
- **対応 ash_xr branch**: `feat/adopt-vrm1-absorbed-generics` (`f5cb0a36`、+161/-641)。vrm_bridge の VrmPlugin::build() 複製解消 / morph 順序補完削除 / flinch の overlay 化 / 表情フィルタ・カプセル幾何・bone query の vrm1 API 置換
- **残手順** (この順で):
  1. ~~vrm1: branch checkout → `/code-review` → finding 解消~~ 済 (2026-07-11)
  2. user 承認 → vrm1 main へマージ
  3. ash_xr main (旧コード) が新 vrm1 main でビルドできることを確認 (additive 設計なので通るはず。2026-07-11 に一度実証済。ただし review fix で `BoneOverlaySystems` の順序付けが chain 組込に変わったため再確認すること)
  4. ash_xr: branch checkout → `/code-review` → finding 解消 → user 承認 → ash_xr main へマージ
- **検証注意**: ash_xr 側 branch の build/test は vrm1 側も本 branch に切り替えた状態で行うこと (`path = "../bevy_vrm1"`)。worktree の場合は兄弟配置 (CLAUDE.md § 開発体制)
- **ash_xr 側への引き継ぎ** (review で発見、ash_xr branch で対応):
  - `vrm_bridge.rs` (ash_xr main) は VrmCorePlugin 相当を手組みしており `BoneOverlayPlugin` を含まない + doc の vrm1 行番号参照が stale。adopt branch での `VrmCorePlugin` 置換で解消されるはずだが要確認 (置換せず個別 add のままだと `BoneRotationOverlay` がサイレント no-op)
  - `manual_expression_names()` は毎呼び出しで alloc + sort する。immediate-mode UI から毎フレーム呼ばない (結果をキャッシュする)

---

## 残作業ブロック

| ブロック | 状態 | 内容 |
|---|---|---|
| batch-1: ash_xr 汎用処理吸収 | review 済 / マージ承認待ち | 上記 Active Branch 参照 |
| batch-2: VR IK の VRM 知識層移管 | 未着手 (設計から) | ash_xr `ik/` の移管。切断面の設計判断が必要: 外部ポーズ入力用 SystemSet の新設、feature gate の要否、`FootStepState` (歩行サイクル) はアプリ側残置。対象 = 軸 retarget (`ik/systems.rs:549-555` rest translation → bone axis 補正、VRM +Z ↔ Bevy -Z の model_flip)、rest pose キャリブレーション (`ik/calibration.rs`)、2 ボーン解析 IK + 腰推定 + スパイン分配 (`ik/solver.rs`、純 bevy_math)。`apply_vr_ik` の 16 個別 `*BoneEntity` query → `HumanoidBoneEntities` 移行も batch-2 で (ホットパスのため batch-1 では見送り) |
| spring bone 単独 reset / pause API | 未着手 | 現状リセットは `PlayVrma::reset_spring_bones` 経由のみ。`SpringJointState` / `reset_velocity()` は pub(crate)。ash_xr の LOD detach/reattach と組み合わせると将来必要になる見込み (2026-07-11 調査) |
| `LookAtType::Expression` 実装 | 未着手 | `src/vrm/look_at.rs:123` が `todo!()`。Bone モードのみ動作 |

## 完了済 Branch

(なし — batch-1 が初回)
