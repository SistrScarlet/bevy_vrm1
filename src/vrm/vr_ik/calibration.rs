//! VRM 骨格からの IK チェーン長抽出 (calibration)。
//!
//! [`build_vr_ik_chain_cache`] は VRM ロード直後の rest-pose 位置から
//! [`VrIkChainCache`] を構築する純粋関数。panic なし、幾何計算のみ。

use bevy::math::{Quat, Vec3};

use crate::vrm::vr_ik::{VrIkChainCache, VrIkLegChainCache};

/// [`build_vr_ik_chain_cache`] の入力: humanoid 骨の rest-pose world 座標。
///
/// `Option` の骨は VRM 1.0 仕様上 optional な骨 (欠けていても IK は縮退動作する)。
/// 位置引数の並び間違い事故を防ぐため、struct の名前付きフィールドで受ける。
#[derive(Debug, Clone, Copy)]
pub struct VrIkRestPositions {
    pub head: Vec3,
    pub neck: Option<Vec3>,
    pub chest: Option<Vec3>,
    pub spine: Option<Vec3>,
    pub hips: Vec3,
    pub left_shoulder: Option<Vec3>,
    pub left_upper_arm: Vec3,
    pub left_lower_arm: Vec3,
    pub left_hand: Vec3,
    pub right_shoulder: Option<Vec3>,
    pub right_upper_arm: Vec3,
    pub right_lower_arm: Vec3,
    pub right_hand: Vec3,
    pub left_upper_leg: Option<Vec3>,
    pub left_lower_leg: Option<Vec3>,
    pub left_foot: Option<Vec3>,
    pub right_upper_leg: Option<Vec3>,
    pub right_lower_leg: Option<Vec3>,
    pub right_foot: Option<Vec3>,
}

/// VRM 骨格の rest-pose world 座標から [`VrIkChainCache`] を構築する。
///
/// - `hip_xz_offset = (hips.x - head.x, hips.z - head.z)`
/// - `hip_height_ratio = hips.y / head.y` (ゼロ除算ガード: head.y ≤ 0.01 なら 0.6)
/// - `upper_arm_len.0 = |left_upper_arm - left_lower_arm|` (右は `.1`)
/// - `lower_arm_len.0 = |left_hand - left_lower_arm|`
/// - `shoulder_offset.0 = left_shoulder.unwrap_or(left_upper_arm) - head`
/// - 脚 6 骨: 全て `Some` の場合のみ `legs = Some(VrIkLegChainCache { ... })`、それ以外は `None`
pub fn build_vr_ik_chain_cache(rest: &VrIkRestPositions) -> VrIkChainCache {
    let hip_offset = rest.hips - rest.head;

    let hip_height_ratio = if rest.head.y > 0.01 {
        rest.hips.y / rest.head.y
    } else {
        0.6
    };

    let upper_arm_len = (
        (rest.left_lower_arm - rest.left_upper_arm).length(),
        (rest.right_lower_arm - rest.right_upper_arm).length(),
    );
    let lower_arm_len = (
        (rest.left_hand - rest.left_lower_arm).length(),
        (rest.right_hand - rest.right_lower_arm).length(),
    );

    let shoulder_offset = (
        rest.left_shoulder.unwrap_or(rest.left_upper_arm) - rest.head,
        rest.right_shoulder.unwrap_or(rest.right_upper_arm) - rest.head,
    );

    // 脚チェーン: 6 骨が全て Some の場合のみ構築
    let legs = match (
        rest.left_upper_leg,
        rest.left_lower_leg,
        rest.left_foot,
        rest.right_upper_leg,
        rest.right_lower_leg,
        rest.right_foot,
    ) {
        (
            Some(l_upper),
            Some(l_lower),
            Some(l_foot),
            Some(r_upper),
            Some(r_lower),
            Some(r_foot),
        ) => Some(VrIkLegChainCache {
            upper_leg_len: ((l_lower - l_upper).length(), (r_lower - r_upper).length()),
            lower_leg_len: ((l_foot - l_lower).length(), (r_foot - r_lower).length()),
            upper_leg_offset: (l_upper - rest.hips, r_upper - rest.hips),
            foot_offset: (l_foot - rest.hips, r_foot - rest.hips),
            leg_axis_correction: (Quat::IDENTITY, Quat::IDENTITY),
        }),
        _ => None,
    };

    VrIkChainCache {
        upper_arm_len,
        lower_arm_len,
        hip_xz_offset: (hip_offset.x, hip_offset.z),
        shoulder_offset,
        hip_height_ratio,
        arm_axis_correction: (Quat::IDENTITY, Quat::IDENTITY),
        arm_hand_correction: (Quat::IDENTITY, Quat::IDENTITY),
        legs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_cache_basic() {
        // 脚骨付きで build_vr_ik_chain_cache を呼ぶ
        let cache = build_vr_ik_chain_cache(&VrIkRestPositions {
            head: Vec3::new(0.0, 1.7, 0.0),
            neck: Some(Vec3::new(0.0, 1.6, 0.0)),
            chest: Some(Vec3::new(0.0, 1.4, 0.0)),
            spine: Some(Vec3::new(0.0, 1.2, 0.0)),
            hips: Vec3::new(0.0, 1.0, 0.0),
            left_shoulder: Some(Vec3::new(-0.15, 1.55, 0.0)),
            left_upper_arm: Vec3::new(-0.2, 1.5, 0.0),
            left_lower_arm: Vec3::new(-0.5, 1.5, 0.0),
            left_hand: Vec3::new(-0.8, 1.5, 0.0),
            right_shoulder: Some(Vec3::new(0.15, 1.55, 0.0)),
            right_upper_arm: Vec3::new(0.2, 1.5, 0.0),
            right_lower_arm: Vec3::new(0.5, 1.5, 0.0),
            right_hand: Vec3::new(0.8, 1.5, 0.0),
            left_upper_leg: Some(Vec3::new(-0.1, 0.95, 0.0)),
            left_lower_leg: Some(Vec3::new(-0.1, 0.5, 0.0)),
            left_foot: Some(Vec3::new(-0.1, 0.0, 0.0)),
            right_upper_leg: Some(Vec3::new(0.1, 0.95, 0.0)),
            right_lower_leg: Some(Vec3::new(0.1, 0.5, 0.0)),
            right_foot: Some(Vec3::new(0.1, 0.0, 0.0)),
        });
        assert!(
            cache.hip_xz_offset.0.abs() < 0.01 && cache.hip_xz_offset.1.abs() < 0.01,
            "hip_xz_offset mismatch: {:?}",
            cache.hip_xz_offset
        );
        assert!(
            (cache.upper_arm_len.0 - 0.3).abs() < 0.01,
            "upper_arm_len.0: {}",
            cache.upper_arm_len.0
        );
        assert!(
            (cache.lower_arm_len.0 - 0.3).abs() < 0.01,
            "lower_arm_len.0: {}",
            cache.lower_arm_len.0
        );
        assert!(
            (cache.upper_arm_len.1 - 0.3).abs() < 0.01,
            "upper_arm_len.1: {}",
            cache.upper_arm_len.1
        );
        assert!(
            (cache.lower_arm_len.1 - 0.3).abs() < 0.01,
            "lower_arm_len.1: {}",
            cache.lower_arm_len.1
        );
        // hip_height_ratio = 1.0 / 1.7 ≈ 0.5882
        let expected_ratio = 1.0_f32 / 1.7;
        assert!(
            (cache.hip_height_ratio - expected_ratio).abs() < 0.01,
            "hip_height_ratio: {}, expected≈{expected_ratio:.4}",
            cache.hip_height_ratio
        );
        // 脚チェーン
        let legs = cache.legs.as_ref().expect("legs should be Some");
        // upper_leg_len: |lower_leg - upper_leg| = |(−0.1,0.5,0)−(−0.1,0.95,0)| = 0.45
        assert!(
            (legs.upper_leg_len.0 - 0.45).abs() < 0.01,
            "upper_leg_len.0: {}",
            legs.upper_leg_len.0
        );
        assert!(
            (legs.upper_leg_len.1 - 0.45).abs() < 0.01,
            "upper_leg_len.1: {}",
            legs.upper_leg_len.1
        );
        // lower_leg_len: |foot - lower_leg| = |(−0.1,0,0)−(−0.1,0.5,0)| = 0.5
        assert!(
            (legs.lower_leg_len.0 - 0.5).abs() < 0.01,
            "lower_leg_len.0: {}",
            legs.lower_leg_len.0
        );
        assert!(
            (legs.lower_leg_len.1 - 0.5).abs() < 0.01,
            "lower_leg_len.1: {}",
            legs.lower_leg_len.1
        );
        // upper_leg_offset: upper_leg - hips = (−0.1,0.95,0)−(0,1.0,0) = (−0.1,−0.05,0)
        assert!(
            (legs.upper_leg_offset.0 - Vec3::new(-0.1, -0.05, 0.0)).length() < 0.01,
            "upper_leg_offset.0: {:?}",
            legs.upper_leg_offset.0
        );
        assert!(
            (legs.upper_leg_offset.1 - Vec3::new(0.1, -0.05, 0.0)).length() < 0.01,
            "upper_leg_offset.1: {:?}",
            legs.upper_leg_offset.1
        );
        // foot_offset: foot - hips = (−0.1,0,0)−(0,1.0,0) = (−0.1,−1.0,0)
        assert!(
            (legs.foot_offset.0 - Vec3::new(-0.1, -1.0, 0.0)).length() < 0.01,
            "foot_offset.0: {:?}",
            legs.foot_offset.0
        );
        assert!(
            (legs.foot_offset.1 - Vec3::new(0.1, -1.0, 0.0)).length() < 0.01,
            "foot_offset.1: {:?}",
            legs.foot_offset.1
        );
    }

    #[test]
    fn build_cache_optional_bones_missing() {
        // optional 骨なし (neck/chest/spine/shoulder/脚 6 骨 = None)
        let cache = build_vr_ik_chain_cache(&VrIkRestPositions {
            head: Vec3::new(0.0, 1.7, 0.0),
            neck: None,
            chest: None,
            spine: None,
            hips: Vec3::new(0.0, 1.0, 0.0),
            left_shoulder: None,
            left_upper_arm: Vec3::new(-0.2, 1.5, 0.0),
            left_lower_arm: Vec3::new(-0.5, 1.5, 0.0),
            left_hand: Vec3::new(-0.8, 1.5, 0.0),
            right_shoulder: None,
            right_upper_arm: Vec3::new(0.2, 1.5, 0.0),
            right_lower_arm: Vec3::new(0.5, 1.5, 0.0),
            right_hand: Vec3::new(0.8, 1.5, 0.0),
            left_upper_leg: None,
            left_lower_leg: None,
            left_foot: None,
            right_upper_leg: None,
            right_lower_leg: None,
            right_foot: None,
        });
        assert!(
            cache.hip_xz_offset.0.abs() < 0.01 && cache.hip_xz_offset.1.abs() < 0.01,
            "hip_xz_offset mismatch: {:?}",
            cache.hip_xz_offset
        );
        // shoulder_offset.0: left_upper_arm - head = (-0.2,1.5,0) - (0,1.7,0) = (-0.2,-0.2,0)
        assert!(
            (cache.shoulder_offset.0 - Vec3::new(-0.2, -0.2, 0.0)).length() < 0.01,
            "shoulder_offset.0: {:?}",
            cache.shoulder_offset.0
        );
        // hip_height_ratio は head/hips から常に計算可能 (脚骨なしでも)
        let expected_ratio = 1.0_f32 / 1.7;
        assert!(
            (cache.hip_height_ratio - expected_ratio).abs() < 0.01,
            "hip_height_ratio: {}, expected≈{expected_ratio:.4}",
            cache.hip_height_ratio
        );
        // 脚骨 None → legs = None
        assert!(
            cache.legs.is_none(),
            "legs should be None when bones missing"
        );
    }

    #[test]
    fn build_cache_degenerate_head_height() {
        // head.y ≈ 0 の退化入力: hip_height_ratio はフォールバック値 0.6
        let cache = build_vr_ik_chain_cache(&VrIkRestPositions {
            head: Vec3::new(0.0, 0.0, 0.0),
            neck: None,
            chest: None,
            spine: None,
            hips: Vec3::new(0.0, -0.5, 0.0),
            left_shoulder: None,
            left_upper_arm: Vec3::new(-0.2, -0.1, 0.0),
            left_lower_arm: Vec3::new(-0.5, -0.1, 0.0),
            left_hand: Vec3::new(-0.8, -0.1, 0.0),
            right_shoulder: None,
            right_upper_arm: Vec3::new(0.2, -0.1, 0.0),
            right_lower_arm: Vec3::new(0.5, -0.1, 0.0),
            right_hand: Vec3::new(0.8, -0.1, 0.0),
            left_upper_leg: None,
            left_lower_leg: None,
            left_foot: None,
            right_upper_leg: None,
            right_lower_leg: None,
            right_foot: None,
        });
        assert!(
            (cache.hip_height_ratio - 0.6).abs() < 1e-6,
            "hip_height_ratio should fall back to 0.6, got {}",
            cache.hip_height_ratio
        );
    }
}
