//! HMD / コントローラ pose から VRM humanoid 骨格を駆動する VR IK。
//!
//! `bevy_ash_xr` の `ik/` から VRM 知識層 (2 ボーン解析 IK・腰推定・スパイン分配・
//! rest-pose キャリブレーション・骨への適用) を移管したもの。歩行サイクルなどの
//! 入力生成はアプリ側の責務で、[`VrIkTargets`] を通して毎フレーム受け取る。
//!
//! # 使い方
//!
//! VRM entity に [`VrIk`] を挿すと有効化される ([`VrIkTargets`] は required component
//! として自動挿入)。アプリは毎フレーム [`VrIkTargets`] へ HMD / コントローラ pose を書く。
//! [`VrIkChainCache`] は骨 spawn 完了後に自動挿入される。
//!
//! # 座標空間の契約 (POC 制約)
//!
//! - [`VrIkTargets`] の pose は「VRM の骨階層が living する空間」の座標で渡す。
//!   hips の祖先 (VRM root など) が identity である前提で hips へ直接書き込む
//! - 脚 IK の foot target は床 y=0 を前提とする
//! - 同一 entity で VRM を差し替えた場合 (detach → 再ロード)、旧寸法の
//!   [`VrIkChainCache`] が残るため手動で remove すること
//! - `LookAt` / `BodyTracking` が同一 entity で有効な場合、spine chain 全体
//!   (spine/chest/neck/head) は gaze 系が IK の上に上書きする。併用の可否はアプリの責任

pub mod calibration;
pub mod solver;
mod systems;

use bevy::math::{Quat, Vec3};
use bevy::prelude::*;

/// VR IK のシステム (キャッシュ初期化 + 毎フレーム適用) が属する `SystemSet`。
///
/// [`VrmCorePlugin`](crate::vrm::VrmCorePlugin) が `AnimationSystems` の後・
/// [`VrmSystemSets::Constraints`](crate::system_set::VrmSystemSets) の前に chain で
/// 組み込む (IK はアニメーション同様「humanoid ポーズを書く側」のため、node constraint /
/// `LookAt` / `SpringBone` が同フレームで IK 結果を反映できる位置に置く)。
/// [`VrIkTargets`] を書くシステムは、同フレームで反映させたい場合この set より前
/// (`Update` など) に配置すること。
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VrIkSystems;

pub struct VrIkPlugin;

impl Plugin for VrIkPlugin {
    fn build(
        &self,
        app: &mut App,
    ) {
        app.register_type::<VrIk>()
            .register_type::<VrIkTargets>()
            .register_type::<VrIkPose>()
            .register_type::<VrIkFootStep>()
            .register_type::<VrIkChainCache>()
            .register_type::<VrIkLegChainCache>()
            .add_systems(
                PostUpdate,
                (systems::init_vr_ik_chain_cache, systems::apply_vr_ik)
                    .chain()
                    .in_set(VrIkSystems)
                    .run_if(any_with_component::<VrIk>),
            );
    }
}

/// VR IK のチューニング設定。VRM entity に挿すと IK が有効化される。
///
/// [`VrIkTargets`] は required component として自動挿入される。
/// [`VrIkChainCache`] は骨 spawn 完了後に自動挿入される。
#[derive(Component, Debug, Clone, Reflect)]
#[reflect(Component)]
#[require(VrIkTargets)]
pub struct VrIk {
    /// spine → chest → neck → head への回転分配の重み。
    pub spine_weights: [f32; 4],
}

impl Default for VrIk {
    fn default() -> Self {
        Self {
            spine_weights: [0.15, 0.2, 0.25, 0.4],
        }
    }
}

/// 外部ポーズ入力 (HMD / コントローラ / 歩行オフセット)。アプリが毎フレーム書く。
///
/// 座標空間は VRM の骨階層が living する空間 (モジュール doc の契約参照)。
#[derive(Component, Debug, Clone, Default, Reflect)]
#[reflect(Component)]
pub struct VrIkTargets {
    /// 頭部ターゲット (HMD pose)。`None` = IK 全体を不発 (未接続相当)。
    pub head: Option<VrIkPose>,
    /// 左手ターゲット (コントローラ pose)。`None` = 左腕 IK をスキップ。
    pub left_hand: Option<VrIkPose>,
    /// 右手ターゲット (コントローラ pose)。`None` = 右腕 IK をスキップ。
    pub right_hand: Option<VrIkPose>,
    /// 歩行オフセット (アプリ側の歩行サイクルが計算)。デフォルト = 全ゼロ
    /// (足首 joint は床 y=0 直上・rest XZ オフセット位置に接地)。
    pub foot_step: VrIkFootStep,
}

/// IK ターゲットの位置と姿勢。
#[derive(Debug, Clone, Copy, Default, Reflect)]
pub struct VrIkPose {
    pub translation: Vec3,
    pub rotation: Quat,
}

/// 歩行による足オフセット。foot target へ加算される。
#[derive(Debug, Clone, Copy, Default, Reflect)]
pub struct VrIkFootStep {
    /// 左足の XZ オフセット (移動方向 × ストライド)。Y 成分は無視される。
    pub left_offset_xz: Vec3,
    /// 左足の足上げ高さ (foot target の Y 絶対値になる)。
    pub left_height: f32,
    /// 右足の XZ オフセット。Y 成分は無視される。
    pub right_offset_xz: Vec3,
    /// 右足の足上げ高さ。
    pub right_height: f32,
}

/// Rest-pose から計算した IK チェーン寸法キャッシュ。
///
/// [`VrIk`] を持つ entity へ骨 spawn 完了後に自動挿入される。
/// タプルは (left, right)。
#[derive(Component, Debug, Clone, Reflect)]
#[reflect(Component)]
pub struct VrIkChainCache {
    /// upper arm 骨の長さ (left, right)。
    pub upper_arm_len: (f32, f32),
    /// lower arm 骨の長さ (left, right)。
    pub lower_arm_len: (f32, f32),
    /// rest pose: hips-head オフセットの XZ 成分 (x, z)。Y は `hip_height_ratio` 経由で算出。
    pub hip_xz_offset: (f32, f32),
    /// rest pose: `shoulder_pos` - `head_pos` (left, right)。shoulder 骨が無い場合は `upper_arm` 代替。
    pub shoulder_offset: (Vec3, Vec3),
    /// `model_hip_y` / `model_head_y`。体長比 hip 高さ計算に使用。
    pub hip_height_ratio: f32,
    /// Arm bone axis → Y 補正 (left, right)。rest lower arm local translation から算出。
    pub arm_axis_correction: (Quat, Quat),
    /// Arm bone axis → -Z 補正 (left, right)。hand rotation alignment 用。
    pub arm_hand_correction: (Quat, Quat),
    /// 脚チェーン (脚骨 6 本が揃っている場合のみ `Some`)。
    pub legs: Option<VrIkLegChainCache>,
}

/// Rest-pose 脚チェーン長とオフセット。タプルは (left, right)。
#[derive(Debug, Clone, Reflect)]
pub struct VrIkLegChainCache {
    /// upper leg 骨の長さ (left, right)。
    pub upper_leg_len: (f32, f32),
    /// lower leg 骨の長さ (left, right)。
    pub lower_leg_len: (f32, f32),
    /// rest pose: `upper_leg_pos` - `hips_pos` (left, right)。
    pub upper_leg_offset: (Vec3, Vec3),
    /// rest pose: `foot_pos` - `hips_pos` (left, right)。
    pub foot_offset: (Vec3, Vec3),
    /// Leg bone axis → Y 補正 (left, right)。rest lower leg local translation から算出。
    pub leg_axis_correction: (Quat, Quat),
}
