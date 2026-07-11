//! humanoid 骨のワールド座標からカプセル群を近似生成するユーティリティ。
//!
//! 当たり判定や簡易物理ボディの生成に使う。物理エンジンに依存しない幾何情報
//! ([`HumanoidCapsule`]) のみを返すため、利用側で任意のコライダ表現へ変換すること。

use crate::vrm::humanoid_bone::HumanoidBoneEntities;
use bevy::prelude::*;

/// カプセルが表す身体部位の種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect)]
pub enum HumanoidCapsuleKind {
    Head,
    Torso,
    Limb,
}

/// 1 本のカプセルの幾何情報。
///
/// ローカル Y 軸方向を軸とするカプセルを `rotation` で向け、`position` (中心) に
/// 置いたものとして解釈する。
#[derive(Debug, Clone, Copy, Reflect)]
pub struct HumanoidCapsule {
    pub kind: HumanoidCapsuleKind,
    pub position: Vec3,
    pub rotation: Quat,
    pub radius: f32,
    pub half_height: f32,
}

/// カプセル寸法を骨間距離から決めるための比率。
#[derive(Debug, Clone, Copy, Reflect)]
pub struct HumanoidCapsuleRatios {
    /// 四肢カプセルの半径 = 骨間距離 × この値。
    pub limb_radius_ratio: f32,
    /// 胴体カプセルの半径 = 骨間距離 × この値。
    pub torso_radius_ratio: f32,
    /// 頭カプセルの半径 = head-neck 距離 × この値。
    pub head_radius_factor: f32,
}

impl Default for HumanoidCapsuleRatios {
    fn default() -> Self {
        Self {
            limb_radius_ratio: 0.12,
            torso_radius_ratio: 0.20,
            head_radius_factor: 0.40,
        }
    }
}

/// [`fit_humanoid_capsules`] の入力となる骨のワールド座標一式。
///
/// chest は VRM 仕様上 optional のため欠けていてもよい (胴体が hips→neck の
/// 1 本にフォールバックする)。
#[derive(Debug, Clone, Copy)]
pub struct HumanoidBonePositions {
    pub head: Vec3,
    pub neck: Vec3,
    pub chest: Option<Vec3>,
    pub hips: Vec3,
    pub left_upper_arm: Vec3,
    pub left_lower_arm: Vec3,
    pub left_hand: Vec3,
    pub right_upper_arm: Vec3,
    pub right_lower_arm: Vec3,
    pub right_hand: Vec3,
    pub left_upper_leg: Vec3,
    pub left_lower_leg: Vec3,
    pub left_foot: Vec3,
    pub right_upper_leg: Vec3,
    pub right_lower_leg: Vec3,
    pub right_foot: Vec3,
}

impl HumanoidBonePositions {
    /// [`HumanoidBoneEntities`] と座標取得クロージャから構築する。
    /// chest 以外の骨が 1 つでも解決できなければ `None` を返す。
    pub fn from_bone_entities(
        bones: &HumanoidBoneEntities,
        mut position_of: impl FnMut(Entity) -> Option<Vec3>,
    ) -> Option<Self> {
        let mut pos = |bone: &str| bones.find(bone).and_then(&mut position_of);
        Some(Self {
            head: pos("head")?,
            neck: pos("neck")?,
            chest: pos("chest"),
            hips: pos("hips")?,
            left_upper_arm: pos("leftUpperArm")?,
            left_lower_arm: pos("leftLowerArm")?,
            left_hand: pos("leftHand")?,
            right_upper_arm: pos("rightUpperArm")?,
            right_lower_arm: pos("rightLowerArm")?,
            right_hand: pos("rightHand")?,
            left_upper_leg: pos("leftUpperLeg")?,
            left_lower_leg: pos("leftLowerLeg")?,
            left_foot: pos("leftFoot")?,
            right_upper_leg: pos("rightUpperLeg")?,
            right_lower_leg: pos("rightLowerLeg")?,
            right_foot: pos("rightFoot")?,
        })
    }
}

/// humanoid 骨の位置からカプセル群を近似生成する。
///
/// 内訳: 頭 1 本 + 胴体 (chest あり: hips→chest / chest→neck の 2 本、
/// なし: hips→neck の 1 本) + 四肢 8 本 (上腕/前腕/大腿/下腿 × 左右)。
pub fn fit_humanoid_capsules(
    bones: &HumanoidBonePositions,
    ratios: &HumanoidCapsuleRatios,
) -> Vec<HumanoidCapsule> {
    let mut capsules = Vec::with_capacity(11);

    // 頭: 球に近いカプセル
    let head_to_neck = (bones.neck - bones.head).length();
    let head_radius = (head_to_neck * ratios.head_radius_factor).max(0.05);
    capsules.push(HumanoidCapsule {
        kind: HumanoidCapsuleKind::Head,
        position: bones.head + Vec3::Y * head_radius * 0.5,
        rotation: Quat::IDENTITY,
        radius: head_radius,
        half_height: head_radius * 0.3,
    });

    match bones.chest {
        Some(chest) => {
            capsules.push(capsule_between(
                bones.hips,
                chest,
                ratios.torso_radius_ratio,
                HumanoidCapsuleKind::Torso,
            ));
            capsules.push(capsule_between(
                chest,
                bones.neck,
                ratios.torso_radius_ratio,
                HumanoidCapsuleKind::Torso,
            ));
        }
        None => {
            capsules.push(capsule_between(
                bones.hips,
                bones.neck,
                ratios.torso_radius_ratio,
                HumanoidCapsuleKind::Torso,
            ));
        }
    }

    let limbs = [
        (bones.left_upper_arm, bones.left_lower_arm),
        (bones.left_lower_arm, bones.left_hand),
        (bones.right_upper_arm, bones.right_lower_arm),
        (bones.right_lower_arm, bones.right_hand),
        (bones.left_upper_leg, bones.left_lower_leg),
        (bones.left_lower_leg, bones.left_foot),
        (bones.right_upper_leg, bones.right_lower_leg),
        (bones.right_lower_leg, bones.right_foot),
    ];
    for (a, b) in limbs {
        capsules.push(capsule_between(
            a,
            b,
            ratios.limb_radius_ratio,
            HumanoidCapsuleKind::Limb,
        ));
    }

    capsules
}

fn capsule_between(
    a: Vec3,
    b: Vec3,
    radius_ratio: f32,
    kind: HumanoidCapsuleKind,
) -> HumanoidCapsule {
    let center = (a + b) * 0.5;
    let dir = b - a;
    let length = dir.length();
    let half_height = (length * 0.5).max(0.01);
    let radius = (length * radius_ratio).max(0.01);
    let rotation = if length > 1e-6 {
        Quat::from_rotation_arc(Vec3::Y, dir.normalize())
    } else {
        Quat::IDENTITY
    };
    HumanoidCapsule {
        kind,
        position: center,
        rotation,
        radius,
        half_height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn standard_bones() -> HumanoidBonePositions {
        HumanoidBonePositions {
            head: Vec3::new(0.0, 1.6, 0.0),
            neck: Vec3::new(0.0, 1.45, 0.0),
            chest: Some(Vec3::new(0.0, 1.2, 0.0)),
            hips: Vec3::new(0.0, 0.95, 0.0),
            left_upper_arm: Vec3::new(-0.2, 1.4, 0.0),
            left_lower_arm: Vec3::new(-0.45, 1.4, 0.0),
            left_hand: Vec3::new(-0.7, 1.4, 0.0),
            right_upper_arm: Vec3::new(0.2, 1.4, 0.0),
            right_lower_arm: Vec3::new(0.45, 1.4, 0.0),
            right_hand: Vec3::new(0.7, 1.4, 0.0),
            left_upper_leg: Vec3::new(-0.1, 0.9, 0.0),
            left_lower_leg: Vec3::new(-0.1, 0.5, 0.0),
            left_foot: Vec3::new(-0.1, 0.1, 0.0),
            right_upper_leg: Vec3::new(0.1, 0.9, 0.0),
            right_lower_leg: Vec3::new(0.1, 0.5, 0.0),
            right_foot: Vec3::new(0.1, 0.1, 0.0),
        }
    }

    #[test]
    fn with_chest_produces_11_capsules() {
        let capsules = fit_humanoid_capsules(&standard_bones(), &HumanoidCapsuleRatios::default());
        assert_eq!(capsules.len(), 11);
        assert_eq!(
            capsules
                .iter()
                .filter(|c| c.kind == HumanoidCapsuleKind::Head)
                .count(),
            1
        );
        assert_eq!(
            capsules
                .iter()
                .filter(|c| c.kind == HumanoidCapsuleKind::Torso)
                .count(),
            2
        );
        assert_eq!(
            capsules
                .iter()
                .filter(|c| c.kind == HumanoidCapsuleKind::Limb)
                .count(),
            8
        );
    }

    #[test]
    fn without_chest_falls_back_to_single_torso() {
        let bones = HumanoidBonePositions {
            chest: None,
            ..standard_bones()
        };
        let capsules = fit_humanoid_capsules(&bones, &HumanoidCapsuleRatios::default());
        assert_eq!(capsules.len(), 10);
        assert_eq!(
            capsules
                .iter()
                .filter(|c| c.kind == HumanoidCapsuleKind::Torso)
                .count(),
            1
        );
    }

    #[test]
    fn degenerate_bones_clamp_to_min_dimensions() {
        let zero = Vec3::ZERO;
        let bones = HumanoidBonePositions {
            head: zero,
            neck: zero,
            chest: None,
            hips: zero,
            left_upper_arm: zero,
            left_lower_arm: zero,
            left_hand: zero,
            right_upper_arm: zero,
            right_lower_arm: zero,
            right_hand: zero,
            left_upper_leg: zero,
            left_lower_leg: zero,
            left_foot: zero,
            right_upper_leg: zero,
            right_lower_leg: zero,
            right_foot: zero,
        };
        for capsule in fit_humanoid_capsules(&bones, &HumanoidCapsuleRatios::default()) {
            assert!(capsule.radius > 0.0);
            assert!(capsule.half_height > 0.0);
        }
    }

    #[test]
    fn larger_skeleton_produces_larger_capsules() {
        let std_bones = standard_bones();
        let large_bones = {
            let mut b = std_bones;
            let scale = |v: Vec3| v * 1.5;
            b.head = scale(b.head);
            b.neck = scale(b.neck);
            b.chest = b.chest.map(scale);
            b.hips = scale(b.hips);
            b.left_upper_arm = scale(b.left_upper_arm);
            b.left_lower_arm = scale(b.left_lower_arm);
            b.left_hand = scale(b.left_hand);
            b.right_upper_arm = scale(b.right_upper_arm);
            b.right_lower_arm = scale(b.right_lower_arm);
            b.right_hand = scale(b.right_hand);
            b.left_upper_leg = scale(b.left_upper_leg);
            b.left_lower_leg = scale(b.left_lower_leg);
            b.left_foot = scale(b.left_foot);
            b.right_upper_leg = scale(b.right_upper_leg);
            b.right_lower_leg = scale(b.right_lower_leg);
            b.right_foot = scale(b.right_foot);
            b
        };
        let ratios = HumanoidCapsuleRatios::default();
        let std_capsules = fit_humanoid_capsules(&std_bones, &ratios);
        let large_capsules = fit_humanoid_capsules(&large_bones, &ratios);
        for (small, large) in std_capsules.iter().zip(large_capsules.iter()) {
            assert!(large.radius >= small.radius);
            assert!(large.half_height >= small.half_height);
        }
    }

    #[test]
    fn limb_capsule_aligns_with_bone_direction() {
        let bones = standard_bones();
        let capsules = fit_humanoid_capsules(&bones, &HumanoidCapsuleRatios::default());
        // 四肢の先頭 = 左上腕 (left_upper_arm → left_lower_arm、-X 方向)
        let left_upper = capsules
            .iter()
            .find(|c| c.kind == HumanoidCapsuleKind::Limb)
            .unwrap();
        let axis = left_upper.rotation * Vec3::Y;
        let expected = (bones.left_lower_arm - bones.left_upper_arm).normalize();
        assert!(axis.dot(expected) > 0.999);
    }
}
