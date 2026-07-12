//! VR IK の純粋幾何 solver 群 (bevy_math のみ、ECS 非依存)。
//!
//! bevy_ash_xr の `ik/solver.rs` から移管 (歩行サイクル系はアプリ側残置)。
//! アプリが独自の IK 適用を組む際の部品としても使えるよう pub で公開する。

use bevy::math::{EulerRot, Mat3, Quat, Vec3};

/// Analytical two-bone IK solver.
///
/// Returns (upper_bone_rotation, lower_bone_rotation) in world space.
/// Convention: rotation * Vec3::Y = bone direction (from joint to next joint).
/// The pole_vector determines the elbow plane orientation.
///
/// When target is unreachable, distance is clamped to [|upper-lower|+ε, upper+lower-ε].
pub fn two_bone_ik(
    shoulder_pos: Vec3,
    wrist_target: Vec3,
    upper_len: f32,
    lower_len: f32,
    pole_vector: Vec3,
) -> (Quat, Quat) {
    let chain = wrist_target - shoulder_pos;
    let raw_d = chain.length();

    let min_d = (upper_len - lower_len).abs() + 1e-4;
    let max_d = upper_len + lower_len - 1e-4;
    let d = raw_d.clamp(min_d, max_d);

    let chain_dir = if raw_d > 1e-6 { chain / raw_d } else { Vec3::Y };

    // Cosine rule: angle at shoulder
    let cos_shoulder =
        (upper_len * upper_len + d * d - lower_len * lower_len) / (2.0 * upper_len * d);
    let cos_shoulder = cos_shoulder.clamp(-1.0, 1.0);
    let shoulder_angle = cos_shoulder.acos();

    // Build elbow position using pole_vector to define bend plane
    let pole_norm = pole_vector.normalize_or_zero();

    // Project pole onto plane perpendicular to chain_dir
    let pole_proj = pole_norm - chain_dir * chain_dir.dot(pole_norm);
    let bend_axis = if pole_proj.length() > 1e-6 {
        pole_proj.normalize()
    } else {
        // Fallback: find any perpendicular
        let fallback = if chain_dir.x.abs() < 0.9 {
            Vec3::X
        } else {
            Vec3::Z
        };
        let perp = chain_dir.cross(fallback);
        if perp.length() > 1e-6 {
            perp.normalize()
        } else {
            Vec3::Z
        }
    };

    // Elbow position: rotate chain_dir by shoulder_angle around bend_axis (perpendicular to chain)
    let rot_axis = chain_dir.cross(bend_axis);
    let rot_axis = if rot_axis.length() > 1e-6 {
        rot_axis.normalize()
    } else {
        bend_axis
    };

    let upper_dir = Quat::from_axis_angle(rot_axis, shoulder_angle) * chain_dir;
    let elbow_pos = shoulder_pos + upper_dir * upper_len;
    let lower_dir = (wrist_target - elbow_pos).normalize_or_zero();
    let lower_dir = if lower_dir.length() < 0.5 {
        upper_dir
    } else {
        lower_dir
    };

    let upper_rot = bone_rotation(upper_dir, pole_vector);
    let lower_rot = bone_rotation(lower_dir, pole_vector);

    (upper_rot, lower_rot)
}

/// Build a bone rotation such that `rotation * Vec3::Y == dir`.
/// Uses pole to determine the "side" axis, avoiding gimbal singularities.
fn bone_rotation(
    dir: Vec3,
    pole: Vec3,
) -> Quat {
    let dir = dir.normalize_or_zero();
    if dir.length() < 0.5 {
        return Quat::IDENTITY;
    }

    let side = dir.cross(pole).normalize_or_zero();
    if side.length() < 0.01 {
        // dir and pole are parallel — fallback
        return Quat::from_rotation_arc(Vec3::Y, dir);
    }

    let adjusted_pole = side.cross(dir);
    let mat = Mat3::from_cols(side, dir, adjusted_pole);
    Quat::from_mat3(&mat)
}

/// Estimate hip world position and orientation from HMD pose.
///
/// - `hip_height_ratio`: model_hip_y / model_head_y。`hip.y = hmd.y * ratio` で体長差を吸収する。
/// - `hip_xz_offset`: rest pose の (x, 0, z) オフセット。Y は ratio で計算するため 0 を渡す。
///
/// Returns (hip_position, hip_rotation) where hip_rotation = Quat::from_rotation_y(hmd_yaw).
pub fn estimate_hip(
    hmd_translation: Vec3,
    hmd_rotation: Quat,
    hip_height_ratio: f32,
    hip_xz_offset: Vec3,
) -> (Vec3, Quat) {
    let (yaw, _pitch, _roll) = hmd_rotation.to_euler(EulerRot::YXZ);
    let hip_rotation = Quat::from_rotation_y(yaw);
    let xz = hip_rotation * hip_xz_offset;
    let hip_position = Vec3::new(
        hmd_translation.x + xz.x,
        hmd_translation.y * hip_height_ratio,
        hmd_translation.z + xz.z,
    );
    (hip_position, hip_rotation)
}

/// Distribute rotation delta between hip and head across spine chain bones.
///
/// Decomposes the rotation difference (hip_rotation → head_rotation) into yaw/pitch,
/// then distributes by weights. Returns per-bone (yaw_delta, pitch_delta).
/// weights.len() = number of spine chain bones (typically 4: spine, chest, neck, head).
///
/// Pitch is negated because VRM models face +Z (loaded without coordinate conversion)
/// while OpenXR/Bevy use -Z forward. The 180° facing difference inverts the pitch
/// direction when Euler angles are applied as bone-local rotations.
pub fn distribute_spine(
    hip_rotation: Quat,
    head_rotation: Quat,
    weights: &[f32; 4],
) -> [(f32, f32); 4] {
    let delta = hip_rotation.inverse() * head_rotation;
    let (delta_yaw, delta_pitch, _) = delta.to_euler(EulerRot::YXZ);

    [
        (delta_yaw * weights[0], -delta_pitch * weights[0]),
        (delta_yaw * weights[1], -delta_pitch * weights[1]),
        (delta_yaw * weights[2], -delta_pitch * weights[2]),
        (delta_yaw * weights[3], -delta_pitch * weights[3]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    // === two_bone_ik ===

    #[test]
    fn two_bone_ik_straight_extension() {
        // shoulder=(0,0,0), target=(2,0,0), lens=1+1, pole=(0,0,1)
        // Full extension: elbow at (1,0,0), both bones point +X
        let (upper, lower) = two_bone_ik(Vec3::ZERO, Vec3::new(2.0, 0.0, 0.0), 1.0, 1.0, Vec3::Z);
        let upper_dir = upper * Vec3::Y;
        let lower_dir = lower * Vec3::Y;
        assert!(
            (upper_dir - Vec3::X).length() < 0.05,
            "upper should point +X, got {upper_dir}"
        );
        assert!(
            (lower_dir - Vec3::X).length() < 0.05,
            "lower should point +X, got {lower_dir}"
        );
    }

    #[test]
    fn two_bone_ik_90_degree_bend() {
        // shoulder=(0,0,0), target=(1,0,0), lens=1+1, pole=(0,0,1)
        // Equilateral triangle: elbow at (0.5, 0, √0.75≈0.866)
        let (upper, lower) = two_bone_ik(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), 1.0, 1.0, Vec3::Z);
        let upper_dir = upper * Vec3::Y;
        let lower_dir = lower * Vec3::Y;
        let expected_upper_dir = Vec3::new(0.5, 0.0, 0.866).normalize();
        let expected_lower_dir = Vec3::new(0.5, 0.0, -0.866).normalize();
        assert!(
            (upper_dir - expected_upper_dir).length() < 0.05,
            "upper dir mismatch: got {upper_dir}, expected {expected_upper_dir}"
        );
        assert!(
            (lower_dir - expected_lower_dir).length() < 0.05,
            "lower dir mismatch: got {lower_dir}, expected {expected_lower_dir}"
        );
        // Verify distances: elbow should be at unit distance from both shoulder and target
        let elbow = Vec3::ZERO + upper_dir * 1.0;
        assert!(
            (elbow.length() - 1.0).abs() < 0.02,
            "shoulder-elbow distance"
        );
        assert!(
            ((elbow - Vec3::new(1.0, 0.0, 0.0)).length() - 1.0).abs() < 0.02,
            "elbow-wrist distance"
        );
    }

    #[test]
    fn two_bone_ik_reach_limit() {
        // Target way too far — should clamp and still produce valid rotations
        let (upper, lower) = two_bone_ik(Vec3::ZERO, Vec3::new(5.0, 0.0, 0.0), 1.0, 1.0, Vec3::Z);
        let upper_dir = upper * Vec3::Y;
        let lower_dir = lower * Vec3::Y;
        // Both should point roughly toward +X (full extension toward target)
        assert!(upper_dir.x > 0.9, "upper should point toward target");
        assert!(lower_dir.x > 0.9, "lower should point toward target");
    }

    #[test]
    fn two_bone_ik_degenerate_no_nan() {
        // Target very close to shoulder
        let (upper, lower) = two_bone_ik(Vec3::ZERO, Vec3::new(0.01, 0.0, 0.0), 1.0, 1.0, Vec3::Z);
        assert!(!upper.is_nan(), "upper rotation should not be NaN");
        assert!(!lower.is_nan(), "lower rotation should not be NaN");
        assert!(upper.is_normalized(), "upper should be normalized");
        assert!(lower.is_normalized(), "lower should be normalized");
    }

    // === estimate_hip ===

    #[test]
    fn estimate_hip_upright() {
        // ratio=1.0/1.7, hmd.y=1.7 → hip.y = 1.7 * (1.0/1.7) = 1.0
        let ratio = 1.0_f32 / 1.7;
        let (pos, rot) = estimate_hip(Vec3::new(0.0, 1.7, 0.0), Quat::IDENTITY, ratio, Vec3::ZERO);
        assert!(
            (pos - Vec3::new(0.0, 1.0, 0.0)).length() < 0.01,
            "hip pos: {pos}"
        );
        assert!(
            rot.dot(Quat::IDENTITY).abs() > 0.99,
            "hip rot should be ~identity"
        );
    }

    #[test]
    fn estimate_hip_rotated_90() {
        // Facing +X (90° yaw), ratio=1.0/1.7, hmd.y=1.7 → hip.y = 1.0
        let ratio = 1.0_f32 / 1.7;
        let hmd_rot = Quat::from_rotation_y(FRAC_PI_2);
        let (pos, rot) = estimate_hip(Vec3::new(0.0, 1.7, 0.0), hmd_rot, ratio, Vec3::ZERO);
        assert!(
            (pos.y - 1.0).abs() < 0.01,
            "hip Y should be 1.0, got {}",
            pos.y
        );
        let expected_rot = Quat::from_rotation_y(FRAC_PI_2);
        assert!(rot.dot(expected_rot).abs() > 0.99, "hip should face +X");
    }

    #[test]
    fn estimate_hip_ratio_different_heights() {
        // ratio = 1.0/1.7, XZ オフセットなし。hmd.y が変わると hip.y が比例する
        let ratio = 1.0_f32 / 1.7;
        let (pos_150, _) =
            estimate_hip(Vec3::new(0.0, 1.5, 0.0), Quat::IDENTITY, ratio, Vec3::ZERO);
        let (pos_190, _) =
            estimate_hip(Vec3::new(0.0, 1.9, 0.0), Quat::IDENTITY, ratio, Vec3::ZERO);
        // 1.5 * (1/1.7) ≈ 0.882
        assert!(
            (pos_150.y - 1.5 * ratio).abs() < 0.01,
            "hmd=1.5: hip.y={}, expected≈{:.3}",
            pos_150.y,
            1.5 * ratio
        );
        // 1.9 * (1/1.7) ≈ 1.118
        assert!(
            (pos_190.y - 1.9 * ratio).abs() < 0.01,
            "hmd=1.9: hip.y={}, expected≈{:.3}",
            pos_190.y,
            1.9 * ratio
        );
    }

    // === distribute_spine ===

    #[test]
    fn distribute_spine_no_delta() {
        let result = distribute_spine(Quat::IDENTITY, Quat::IDENTITY, &[0.15, 0.2, 0.25, 0.4]);
        for (yaw, pitch) in &result {
            assert!(yaw.abs() < 0.01, "yaw should be ~0");
            assert!(pitch.abs() < 0.01, "pitch should be ~0");
        }
    }

    #[test]
    fn distribute_spine_90_yaw() {
        let head = Quat::from_rotation_y(FRAC_PI_2);
        let weights = [0.15, 0.2, 0.25, 0.4];
        let result = distribute_spine(Quat::IDENTITY, head, &weights);
        let total_yaw: f32 = result.iter().map(|(y, _)| y).sum();
        assert!(
            (total_yaw - FRAC_PI_2).abs() < 0.01,
            "total yaw should be π/2, got {total_yaw}"
        );
        for (i, (yaw, pitch)) in result.iter().enumerate() {
            assert!(
                (*yaw - FRAC_PI_2 * weights[i]).abs() < 0.01,
                "bone {i} yaw mismatch"
            );
            assert!(pitch.abs() < 0.01, "bone {i} pitch should be ~0");
        }
    }

    #[test]
    fn distribute_spine_pitch_negated_for_vrm() {
        // Looking down 30° in OpenXR = negative pitch.
        // VRM model faces +Z so pitch must be negated for correct bone-local application.
        let head = Quat::from_euler(EulerRot::YXZ, 0.0, -FRAC_PI_2 / 3.0, 0.0);
        let weights = [0.25, 0.25, 0.25, 0.25];
        let result = distribute_spine(Quat::IDENTITY, head, &weights);
        let total_pitch: f32 = result.iter().map(|(_, p)| p).sum();
        // Negated: OpenXR -30° → bone-local +30° (positive = tilt toward model's +Z forward)
        assert!(
            (total_pitch - FRAC_PI_2 / 3.0).abs() < 0.01,
            "total pitch should be +π/6 (negated), got {total_pitch}"
        );
    }
}
