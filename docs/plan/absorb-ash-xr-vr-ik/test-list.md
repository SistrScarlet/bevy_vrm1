# batch-2: VR IK の VRM 知識層移管 — テストリスト

- 前提: `design.md`。移管元 (ash_xr) の既存テストは挙動仕様として移植する
- テスト基盤: 純粋関数 = 素のユニットテスト。ECS 振る舞い = `crate::tests::test_app()` ベースの最小 App
- **共有 fixture**: ECS テストは「最小 humanoid 骨格 (T-pose) を spawn し `HumanoidBoneEntities` + `RestTransform`/`RestGlobalTransform` を整合状態で手組する helper」を共有する (11 本の apply テストが各自組むのは非現実的)。fixture の骨名キーは `bone_names` 定数から組む (定数の typo 検出を兼ねる)。world 座標の検証は TransformPlugin を足して propagation するか、rest 階層で手動合成する (実装時に決定)

## solver: two_bone_ik (移管元テスト移植)

- [x] 完全伸展: 到達距離ちょうどのターゲットで upper/lower とも chain 方向を向く
- [x] 90° 屈曲: 正三角形構成で肘位置が pole 側に出る (upper/lower の方向 + 肘距離検証)
- [x] 到達不能 (遠すぎ): clamp されて両骨がターゲット方向へ伸展
- [x] 退化入力 (ターゲットが肩とほぼ同位置): NaN を出さず正規化された Quat を返す

## solver: estimate_hip (移管元テスト移植)

- [x] 直立: `hip.y = hmd.y * ratio`、回転 ≈ identity
- [x] 90° yaw: hip 回転が yaw のみ追従 (pitch/roll は捨てる)
- [x] 体長差: hmd.y の変化に hip.y が比例 (ratio 吸収)

## solver: distribute_spine (移管元テスト移植)

- [x] 差分なし: 全骨の (yaw, pitch) ≈ 0
- [x] 90° yaw 差分: weights 比で分配され合計 = π/2
- [x] pitch 符号反転: OpenXR 前傾 (負 pitch) が骨 local では正 pitch になる (VRM +Z 前方知識)

## calibration: build_ik_chain_cache (移管元テスト移植 + 入力 struct 化)

- [x] 全骨あり: hip_offset / 腕長 (L,R) / hip_height_ratio / 脚チェーン (長さ・オフセット) が rest 位置から正しく計算される
- [x] optional 骨なし (neck/chest/spine/shoulder/脚 6 骨 = None): shoulder_offset は upper_arm 代替、legs = None、他は計算可能
- [x] head.y ≈ 0 の退化入力: hip_height_ratio がフォールバック値 0.6

## ECS: キャリブレーション自動挿入

- [x] `VrIk` + `HumanoidBoneEntities` + `RestGlobalTransform` が揃った entity に `VrIkChainCache` が挿入される
- [x] 骨の `RestGlobalTransform` が未 spawn の間は挿入されず、揃った後のフレームで挿入される (リトライ)
- [x] 必須骨 (例: leftHand) が `HumanoidBoneEntities` に無い VRM では cache が挿入されない (+ warn、テストは未挿入のみ検証)
- [x] 脚骨なし VRM: cache は挿入され `legs = None`
- [x] `VrIk` を挿すと `VrIkTargets` が required component として自動挿入される
- [x] `VrIk` remove 後も cache は残り、再 insert で再キャリブレーションなしに IK が再開する
- [x] `VrIk::default()` の spine_weights がドキュメント記載のデフォルト値

## ECS: apply (毎フレーム適用)

- [x] `head: Some` で hips の Transform が書かれる (translation = estimate_hip 出力、rotation に model_flip Ry(π) が合成されている)
- [x] spine 分配: spine/chest/neck/head の rotation が rest 基準 + 分配 delta になる
- [x] spine 分配: optional 骨 (chest/neck) が無い VRM では該当 delta が捨てられる (残存骨へ再分配されない、合計回転が減る)
- [x] `head: None`: どの骨の Transform も書かれない (change detection も汚さない)
- [x] `left_hand: None` / `right_hand: None`: 該当側の腕 3 骨は書かれず、反対腕・hips は書かれる (左右各 1 ケース — cache の (L, R) タプル取り違え検出)
- [x] 腕 IK: 手の届く位置にターゲットを置くと、upper/lower arm の world 合成回転が two_bone_ik 解 + bone axis 補正になる (肘距離で検証)
- [x] hand 骨: controller rotation + finger axis 補正が適用される
- [x] 脚 IK: legs cache ありで foot target (床 y=0 + foot_step オフセット) へ upper/lower leg が解かれる。foot 骨の Transform は書かれない (lower_leg 追従)
- [x] foot_step デフォルト (全ゼロ): foot target = rest XZ オフセット位置・y=0
- [x] foot_step 指定: XZ オフセットと height が foot target に反映される (offset_xz の Y 成分は無視)
- [x] legs cache なし (脚なし VRM): 脚は書かれず腕・スパインは動く
- [x] 骨 entity が despawn されている場合: その骨だけスキップし他の骨は書かれる (panic しない)
- [x] `VrIk` を remove すると以後どの骨も書かれない (最終ポーズ残留)

## ECS: 実行順・プラグイン構成

- [x] `VrIkSystems` 込みの chain が cycle なく schedule 構築・実行できる (既存 smoke test の拡張 or 同型の新 test)
- [x] `VrIkSystems` は `AnimationSystems` の後・`VrmSystemSets::Constraints` の前に走る (chain エッジの検証: AnimationSystems 相当で書いた値を IK が上書きし、Constraints 相当が IK 結果を読める)
- [x] 複数 VRM: 2 entity にそれぞれ別の `VrIkTargets` を与えると独立に解かれる

## テスト対象外 (doc 化のみ — POC 制約として rustdoc に明記)

- ancestor-identity 前提 (VrIkTargets の座標空間契約)
- 床 y=0 前提 (leg IK の foot target)
- VRM 差し替え時の旧 cache 残留
- LookAt / BodyTracking 併用時の head 上書き規約

## テスト不可能 (実機/目視検証待ち — ash_xr adopt branch で確認)

- [ ] [実機テスト待ち] 実 VRM モデルでの見た目品質 (肘の曲がり方向、脚の接地感、spine の自然さ)
- [ ] [実機テスト待ち] OpenXR 実入力 (HMD/コントローラ) との結線 (ash_xr adopt branch)
- [ ] [実機テスト待ち] VRMA 再生との共存挙動 (IK 管理骨の上書き / 指などの部分共存)
- [ ] [実機テスト待ち] chain 先頭配置による twist 骨 (node constraint) の同フレーム追従改善
