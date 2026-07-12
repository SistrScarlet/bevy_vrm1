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

(現在アクティブな branch なし)

---

## 残作業ブロック

| ブロック | 状態 | 内容 |
|---|---|---|
| batch-2 の ash_xr 側取り込み | 未着手 | vrm1 `feat/absorb-ash-xr-vr-ik` マージ済み (`3b6b6f1`)。ash_xr 側は adopt branch 未着手。引き継ぎ事項: (1) `ik/` を vrm1 API (`VrIk` / `VrIkTargets` / `VrIkChainCache`) へ置換 (`FootStepState` + step 系 solver 関数はアプリ側残置、`VrIkTargets` へ毎フレーム転写する adapter system を新設)。(2) **実行順を ash_xr 版の `GazeControl` 内から chain 先頭 (`AnimationSystems` 直後・`Constraints` 前) に変更した** — twist 骨の同フレーム追従が改善する方向だが実機での見た目再検証が必要。(3) `VrIk` の未使用フィールド `pole_bias_down` / `extension_blend_start` は持ち込んでいない |
| spring bone 単独 reset / pause API | 未着手 | 現状リセットは `PlayVrma::reset_spring_bones` 経由のみ。`SpringJointState` / `reset_velocity()` は pub(crate)。ash_xr の LOD detach/reattach と組み合わせると将来必要になる見込み (2026-07-11 調査) |
| `LookAtType::Expression` 実装 | 未着手 | `src/vrm/look_at.rs:123` が `todo!()`。Bone モードのみ動作 |
| `vrm_error!` の format capture 不具合 | 未着手 (小粒) | `src/error.rs` の arm 1 (`$err:expr`) は単一文字列リテラルにもマッチし `let _e = "..{name}.."` → `error!("{_e}")` となるため inline capture が展開されず波括弧ごと出力される。`src/vrma/initialize.rs` の既存 5 メッセージが該当。修正案: リテラル+capture 用の arm を先頭に足すか、該当呼び出しを `vrm_warn!` 同様のパススルー arm に寄せる (2026-07-11 の review 作業中に発見。新規コードは `vrm_warn!` を使用済み) |

## 完了済 Branch

- `feat/absorb-ash-xr-vr-ik` — merge `3b6b6f1` (2026-07-12、8 commits)。batch-2: ash_xr `ik/` の VR IK VRM 知識層を bevy_vrm1 に移管 (solver / calibration / ECS systems)。公開 API: `VrIk` / `VrIkTargets` (外部ポーズ入力の切断面) / `VrIkChainCache` / `VrIkSystems` + solver 純粋関数 + `bone_names` 定数 (humanoid_bone)。`/code-review` finding 5 件解消 (`26b3727`、VR IK correctness + efficiency + simplification)。テスト 108 件 GREEN + clippy 0。ash_xr main (旧コード) の新 vrm1 main でのビルド確認済み。ash_xr 側の adopt branch は未着手 (残作業ブロック参照)
- `feat/absorb-ash-xr-vrm-generics` — merge `5e28802` (2026-07-11、3 commits)。batch-1: ash_xr の VRM 汎用処理吸収 (`VrmCorePlugin` 分離 / `HumanoidBoneEntities` / `capsule_fit` / `bone_overlay` / expression 拡張) + `/code-review` finding 10 件解消 (BoneOverlay 伝播順序・NaN weight・detach 残留ほか、詳細 `9badf80`) + ash_xr 共有ビルドロック導入 (`2889988`)。ash_xr main (旧コード) の新 vrm1 main でのビルド確認済み。ash_xr 側の対応 branch `feat/adopt-vrm1-absorbed-generics` も ash_xr main へマージ済み (`8ae313ef`、2026-07-12、code-review 8 findings 対応込み)。引き継ぎ 2 件 (vrm_bridge の `VrmCorePlugin` 置換 / `manual_expression_names` の毎フレーム呼び回避) も対応確認済み → **batch-1 は両リポジトリで完了**
