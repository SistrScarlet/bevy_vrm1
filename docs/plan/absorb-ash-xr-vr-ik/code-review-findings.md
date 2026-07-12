# code-review findings (feat/absorb-ash-xr-vr-ik)

- 実施: 2026-07-12
- effort: high (8 angles × 6 candidates → 1-vote verify)
- 結果: 5 findings (correctness 2 + efficiency 2 + simplification 1)

## Findings (severity 順)

### 1. [correctness] apply_arm_ik の parent rotation に hip_rot 欠落

- file: `src/vrm/vr_ik/systems.rs:391`
- 現状: `corrected_parent = parent_rest_world * model_flip()` で rest 近似のみ
- 問題: runtime の `hip_rotation` (HMD yaw 由来) を含まないため、頭を回すと腕位置がずれる。`apply_leg_ik` は正しく `hip_rotation * model_flip()` を使っている
- テストが yaw=0 のみ + flat entity (hierarchy なし) のため未検出
- verdict: CONFIRMED

**修正**:
- `apply_arm_ik` に `hip_rotation: Quat` 引数を追加
- `let corrected_parent = hip_rotation * model_flip() * parent_rest_world;`
- yaw≠0 のテストを追加

### 2. [correctness] vr_ik.rs doc: BodyTracking の上書き範囲が不正確

- file: `src/vrm/vr_ik.rs:20`
- 現状: 「head 回転は gaze 系が IK の上に上書きする」
- 問題: BodyTracking は spine/chest/neck/head 全体を上書きする (body_tracking.rs L338-446)
- verdict: CONFIRMED

**修正**: doc を「spine chain 全体 (spine/chest/neck/head)」に修正

### 3. [efficiency] axis_correction / hand_correction が毎フレーム再計算

- file: `src/vrm/vr_ik/systems.rs:325,372,401`
- 問題: `from_rotation_arc(bone_axis, Y)` は RestTransform 依存の不変値だが毎フレーム計算
- 90Hz × 50VRM で 27,000 回/秒の不要な from_rotation_arc
- verdict: CONFIRMED

**修正**:
- `VrIkChainCache` に `arm_axis_correction: (Quat, Quat)`, `arm_hand_correction: (Quat, Quat)` を追加
- `VrIkLegChainCache` に `axis_correction: (Quat, Quat)` を追加
- `init_vr_ik_chain_cache` で RestTransform から算出して格納

### 4. [efficiency] model_flip() が毎回 sin/cos 呼出

- file: `src/vrm/vr_ik/systems.rs:16-18`
- 問題: `Quat::from_rotation_y(PI)` は libm sinf/cosf を呼ぶ (LLVM は const-fold しない)
- VRM あたり ~8 回/frame
- verdict: CONFIRMED

**修正**: `const MODEL_FLIP: Quat = Quat::from_xyzw(0.0, 1.0, 0.0, 0.0);` に置換

### 5. [simplification] hip_offset.y は dead state

- file: `src/vrm/vr_ik.rs:133`
- 問題: calibration で Vec3 として計算・格納されるが runtime は .x と .z のみ消費 (systems.rs:143)。Y は hip_height_ratio 経由で別途計算
- verdict: CONFIRMED

**修正**: `hip_offset: Vec3` → `hip_xz_offset: (f32, f32)` に縮小

## 実行順

1. #4 (const 化) — 独立、最小 diff
2. #2 (doc 修正) — 独立、1 行
3. #5 (hip_offset 縮小) — struct 変更だが局所的
4. #3 (axis_correction cache) — struct 追加 + init 変更
5. #1 (arm IK parent rotation) — 最重要、テスト追加を伴う

全て同一コミットで可。
