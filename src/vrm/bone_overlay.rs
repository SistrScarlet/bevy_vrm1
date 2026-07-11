//! 骨のローカル回転に加算合成する汎用オーバーレイ。
//!
//! [`BoneRotationOverlay`] を骨 entity に挿すと、GazeControl の後 (`SpringBone` の前) に
//! `Transform::rotation` へ加算合成される。被弾のけぞりや procedural な揺らぎなど、
//! アニメーション・IK が書いたポーズの上へ一時的な回転を乗せる用途を想定している。
//!
//! 順序保証は 2 段構え (いずれも `VrmCorePlugin` の `configure_sets` chain が担保):
//! - [`BoneOverlaySystems`] は VRM set chain の一員として `GazeControl` の後・
//!   `TransformSystems::Propagate` の前に走るため、オーバーレイは同フレームの
//!   描画ポーズへ必ず反映される。
//! - weight > 0.0 のオーバーレイが 1 つでも存在するフレームは
//!   `VrmSystemSets::PropagateAfterExpressions` で追加の transform propagation が走り、
//!   `SpringBone` も同フレームでオーバーレイ適用後の `GlobalTransform` を読む
//!   (存在しないフレームは propagation を省略する。fork の性能最適化を参照)。
//!
//! 毎フレーム乗算合成するため、対象骨の rotation がアニメーション・IK 等で毎フレーム
//! 上書きされることを前提とする。上書きされない骨に weight > 0.0 のまま放置すると
//! 回転が累積する点に注意。

use crate::system_set::VrmSystemSets;
use bevy::app::{App, Plugin, PostUpdate};
use bevy::prelude::*;
use bevy::transform::systems::{propagate_parent_transforms, sync_simple_transforms};

/// [`BoneRotationOverlay`] を適用するシステムが属する `SystemSet`。
/// `VrmCorePlugin` が `GazeControl` と Expressions の間に chain で組み込む。
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

        // オーバーレイが書いた Transform を SpringBone が同フレームで読めるようにする
        // 追加 propagation。オーバーレイ非アクティブ時は走らないため、機能を使わない
        // アプリでは fork の性能最適化 (2 回目の全 scene propagation 削除) が維持される。
        app.add_systems(
            PostUpdate,
            (sync_simple_transforms, propagate_parent_transforms)
                .chain()
                .run_if(any_active_bone_overlay)
                .in_set(VrmSystemSets::PropagateAfterExpressions),
        );
    }
}

fn any_active_bone_overlay(overlays: Query<&BoneRotationOverlay>) -> bool {
    overlays.iter().any(|overlay| overlay.weight > 0.0)
}

fn apply_bone_rotation_overlays(mut bones: Query<(&mut Transform, &BoneRotationOverlay)>) {
    for (mut tf, overlay) in &mut bones {
        let Some(rotation) = compose_overlay(tf.rotation, overlay) else {
            continue;
        };
        tf.rotation = rotation;
    }
}

/// 適用後の回転を返す。適用不要 (weight が 0 以下) または入力が不正
/// (weight・rotation が NaN/無限大) な場合は `None` を返し、呼び出し側は
/// `Transform` に触れない (change detection を汚さない)。
fn compose_overlay(
    rotation: Quat,
    overlay: &BoneRotationOverlay,
) -> Option<Quat> {
    let weight = overlay.weight.clamp(0.0, 1.0);
    // NaN は clamp を素通りするため is_finite で明示的に弾く。
    if !weight.is_finite() || weight <= 0.0 || !overlay.rotation.is_finite() {
        return None;
    }
    Some(rotation * Quat::IDENTITY.slerp(overlay.rotation, weight))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_weight_is_skipped() {
        let overlay = BoneRotationOverlay {
            rotation: Quat::from_rotation_x(1.0),
            weight: 0.0,
        };
        assert!(compose_overlay(Quat::IDENTITY, &overlay).is_none());
    }

    #[test]
    fn full_weight_applies_entire_rotation() {
        let additive = Quat::from_rotation_x(0.5);
        let overlay = BoneRotationOverlay {
            rotation: additive,
            weight: 1.0,
        };
        let rotation = compose_overlay(Quat::IDENTITY, &overlay).unwrap();
        assert!(rotation.angle_between(additive) < 1e-6);
    }

    #[test]
    fn half_weight_applies_half_rotation() {
        let overlay = BoneRotationOverlay {
            rotation: Quat::from_rotation_x(0.5),
            weight: 0.5,
        };
        let rotation = compose_overlay(Quat::IDENTITY, &overlay).unwrap();
        assert!(rotation.angle_between(Quat::from_rotation_x(0.25)) < 1e-5);
    }

    #[test]
    fn weight_is_clamped() {
        let additive = Quat::from_rotation_x(0.5);
        let overlay = BoneRotationOverlay {
            rotation: additive,
            weight: 2.0,
        };
        let rotation = compose_overlay(Quat::IDENTITY, &overlay).unwrap();
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
        let rotation = compose_overlay(base, &overlay).unwrap();
        assert!(rotation.angle_between(base * additive) < 1e-6);
    }

    #[test]
    fn nan_weight_is_skipped() {
        // 例: weight = elapsed / duration で duration が 0.0 のケース (0.0/0.0 = NaN)
        let overlay = BoneRotationOverlay {
            rotation: Quat::from_rotation_x(0.5),
            weight: f32::NAN,
        };
        assert!(compose_overlay(Quat::IDENTITY, &overlay).is_none());
    }

    #[test]
    fn negative_weight_is_skipped() {
        let overlay = BoneRotationOverlay {
            rotation: Quat::from_rotation_x(0.5),
            weight: -1.0,
        };
        assert!(compose_overlay(Quat::IDENTITY, &overlay).is_none());
    }

    #[test]
    fn non_finite_rotation_is_skipped() {
        let overlay = BoneRotationOverlay {
            rotation: Quat::from_xyzw(f32::NAN, 0.0, 0.0, 1.0),
            weight: 1.0,
        };
        assert!(compose_overlay(Quat::IDENTITY, &overlay).is_none());
    }
}
