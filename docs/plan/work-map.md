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

(なし)

---

## ash_xr 連動: batch-1 の残手順 (vrm1 側は完了)

vrm1 の batch-1 は main マージ済み (`5e28802`)。残りは ash_xr 側:

- **対応 ash_xr branch**: `feat/adopt-vrm1-absorbed-generics` (`f5cb0a36`、+161/-641)。vrm_bridge の VrmPlugin::build() 複製解消 / morph 順序補完削除 / flinch の overlay 化 / 表情フィルタ・カプセル幾何・bone query の vrm1 API 置換
- **残手順**: ash_xr branch checkout → `/code-review` → finding 解消 → user 承認 → ash_xr main へマージ
- **注意**: vrm1 側の review fix で `BoneOverlaySystems` の順序付けが変わった (ad-hoc エッジ → `VrmCorePlugin` の chain 組込 + overlay アクティブ時のみ `PropagateAfterExpressions` で条件付き propagation)。adopt branch は vrm1 の新 main を前提に再検証すること
- **ash_xr 側への引き継ぎ** (vrm1 review で発見、ash_xr branch で対応):
  - `vrm_bridge.rs` (ash_xr main) は VrmCorePlugin 相当を手組みしており `BoneOverlayPlugin` を含まない + doc の vrm1 行番号参照が stale。adopt branch での `VrmCorePlugin` 置換で解消されるはずだが要確認 (置換せず個別 add のままだと `BoneRotationOverlay` がサイレント no-op)。また vrm1 の chain (`.before(TransformSystems::Propagate)` 込み) は `VrmCorePlugin::build` にあるため、bridge を残す場合は同等の configure_sets 補完が必要
  - `manual_expression_names()` は毎呼び出しで alloc + sort する。immediate-mode UI から毎フレーム呼ばない (結果をキャッシュする)

---

## 残作業ブロック

| ブロック | 状態 | 内容 |
|---|---|---|
| batch-1: ash_xr 汎用処理吸収 | vrm1 側完了 (`5e28802`) / ash_xr 側 review 待ち | 上記「ash_xr 連動」参照 |
| batch-2: VR IK の VRM 知識層移管 | 未着手 (設計から) | ash_xr `ik/` の移管。切断面の設計判断が必要: 外部ポーズ入力用 SystemSet の新設、feature gate の要否、`FootStepState` (歩行サイクル) はアプリ側残置。対象 = 軸 retarget (`ik/systems.rs:549-555` rest translation → bone axis 補正、VRM +Z ↔ Bevy -Z の model_flip)、rest pose キャリブレーション (`ik/calibration.rs`)、2 ボーン解析 IK + 腰推定 + スパイン分配 (`ik/solver.rs`、純 bevy_math)。`apply_vr_ik` の 16 個別 `*BoneEntity` query → `HumanoidBoneEntities` 移行も batch-2 で (ホットパスのため batch-1 では見送り) |
| spring bone 単独 reset / pause API | 未着手 | 現状リセットは `PlayVrma::reset_spring_bones` 経由のみ。`SpringJointState` / `reset_velocity()` は pub(crate)。ash_xr の LOD detach/reattach と組み合わせると将来必要になる見込み (2026-07-11 調査) |
| `LookAtType::Expression` 実装 | 未着手 | `src/vrm/look_at.rs:123` が `todo!()`。Bone モードのみ動作 |
| `vrm_error!` の format capture 不具合 | 未着手 (小粒) | `src/error.rs` の arm 1 (`$err:expr`) は単一文字列リテラルにもマッチし `let _e = "..{name}.."` → `error!("{_e}")` となるため inline capture が展開されず波括弧ごと出力される。`src/vrma/initialize.rs` の既存 5 メッセージが該当。修正案: リテラル+capture 用の arm を先頭に足すか、該当呼び出しを `vrm_warn!` 同様のパススルー arm に寄せる (2026-07-11 の review 作業中に発見。新規コードは `vrm_warn!` を使用済み) |

## 完了済 Branch

- `feat/absorb-ash-xr-vrm-generics` — merge `5e28802` (2026-07-11、3 commits)。batch-1: ash_xr の VRM 汎用処理吸収 (`VrmCorePlugin` 分離 / `HumanoidBoneEntities` / `capsule_fit` / `bone_overlay` / expression 拡張) + `/code-review` finding 10 件解消 (BoneOverlay 伝播順序・NaN weight・detach 残留ほか、詳細 `9badf80`) + ash_xr 共有ビルドロック導入 (`2889988`)。ash_xr main (旧コード) の新 vrm1 main でのビルド確認済み
