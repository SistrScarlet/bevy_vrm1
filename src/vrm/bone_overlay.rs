//! 骨のローカル回転に加算合成する汎用オーバーレイ。
//!
//! [`BoneRotationOverlay`] を骨 entity に挿すと、GazeControl の後 (`SpringBone` の前) に
//! `Transform::rotation` へ加算合成される。被弾のけぞりや procedural な揺らぎなど、
//! アニメーション・IK が書いたポーズの上へ一時的な回転を乗せる用途を想定している。
//!
//! 毎フレーム乗算合成するため、対象骨の rotation がアニメーション・IK 等で毎フレーム
//! 上書きされることを前提とする。上書きされない骨に weight > 0.0 のまま放置すると
//! 回転が累積する点に注意。

use crate::system_set::VrmSystemSets;
use bevy::app::{App, Plugin, PostUpdate};
use bevy::prelude::*;

/// [`BoneRotationOverlay`] を適用するシステムが属する `SystemSet`。
/// オーバーレイ値を書くシステムは、同フレームで反映させたい場合
/// この set の `.before()` に配置すること。
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BoneOverlaySystems;

/// 骨 entity のローカル回転へ加算合成される回転。
///
/// `GazeControl` の後・SpringBone の前に `Transform::rotation *= slerp(IDENTITY, rotation, weight)`
/// として適用される。
#[derive(Component, Reflect, Debug, Clone, Copy)]
#[reflect(Component)]
pub struct BoneRotationOverlay {
    /// weight = 1.0 のときに加算される回転。
    pub rotation: Quat,
    /// 適用率。0.0 (無効) ..= 1.0 (rotation 全量)。範囲外は clamp される。
    pub weight: f32,
}

impl Default for BoneRotationOverlay {
    fn default() -> Self {
        Self {
            rotation: Quat::IDENTITY,
            weight: 0.0,
        }
    }
}

pub struct BoneOverlayPlugin;

impl Plugin for BoneOverlayPlugin {
    fn build(
        &self,
        app: &mut App,
    ) {
        app.register_type::<BoneRotationOverlay>().add_systems(
            PostUpdate,
            apply_bone_rotation_overlays
                .in_set(BoneOverlaySystems)
                .after(VrmSystemSets::GazeControl)
                .before(VrmSystemSets::SpringBone),
        );
    }
}

fn apply_bone_rotation_overlays(mut bones: Query<(&mut Transform, &BoneRotationOverlay)>) {
    for (mut tf, overlay) in &mut bones {
        if overlay.weight.clamp(0.0, 1.0) <= 0.0 {
            continue;
        }
        tf.rotation = compose_overlay(tf.rotation, overlay);
    }
}

fn compose_overlay(
    rotation: Quat,
    overlay: &BoneRotationOverlay,
) -> Quat {
    let weight = overlay.weight.clamp(0.0, 1.0);
    if weight <= 0.0 {
        return rotation;
    }
    rotation * Quat::IDENTITY.slerp(overlay.rotation, weight)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_weight_leaves_rotation_unchanged() {
        let overlay = BoneRotationOverlay {
            rotation: Quat::from_rotation_x(1.0),
            weight: 0.0,
        };
        let rotation = compose_overlay(Quat::IDENTITY, &overlay);
        assert!(rotation.angle_between(Quat::IDENTITY) < 1e-6);
    }

    #[test]
    fn full_weight_applies_entire_rotation() {
        let additive = Quat::from_rotation_x(0.5);
        let overlay = BoneRotationOverlay {
            rotation: additive,
            weight: 1.0,
        };
        let rotation = compose_overlay(Quat::IDENTITY, &overlay);
        assert!(rotation.angle_between(additive) < 1e-6);
    }

    #[test]
    fn half_weight_applies_half_rotation() {
        let overlay = BoneRotationOverlay {
            rotation: Quat::from_rotation_x(0.5),
            weight: 0.5,
        };
        let rotation = compose_overlay(Quat::IDENTITY, &overlay);
        assert!(rotation.angle_between(Quat::from_rotation_x(0.25)) < 1e-5);
    }

    #[test]
    fn weight_is_clamped() {
        let additive = Quat::from_rotation_x(0.5);
        let overlay = BoneRotationOverlay {
            rotation: additive,
            weight: 2.0,
        };
        let rotation = compose_overlay(Quat::IDENTITY, &overlay);
        assert!(rotation.angle_between(additive) < 1e-6);
    }

    #[test]
    fn composes_on_top_of_existing_rotation() {
        let base = Quat::from_rotation_y(1.0);
        let additive = Quat::from_rotation_x(0.5);
        let overlay = BoneRotationOverlay {
            rotation: additive,
            weight: 1.0,
        };
        let rotation = compose_overlay(base, &overlay);
        assert!(rotation.angle_between(base * additive) < 1e-6);
    }
}
