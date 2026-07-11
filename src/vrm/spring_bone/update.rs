use crate::system_set::VrmSystemSets;
use crate::vrm::gltf::extensions::vrmc_spring_bone::{ColliderShape, Sphere};
use crate::vrm::spring_bone::{SpringJointProps, SpringJointState, SpringRoot};
use bevy::app::App;
use bevy::math::Vec3;
use bevy::prelude::*;
use bevy::time::Time;

pub struct SpringBoneUpdatePlugin;

impl Plugin for SpringBoneUpdatePlugin {
    fn build(
        &self,
        app: &mut App,
    ) {
        app.add_systems(
            PostUpdate,
            update_spring_bones.in_set(VrmSystemSets::SpringBone),
        );
    }
}

/// root の joint loop 中は不変な collider の world 空間データ。
/// per-check の SRT 分解 + `transform_point` + Query lookup を root 先頭 1 回に集約する。
/// root ごとに再構築するため、同一ノード上の複数 collider shape はそれぞれ保持され、
/// 先に処理された root の joint 書き込みも次 root の collider に反映される。
/// Capsule は narrow phase 未実装 (`vrmc_spring_bone.rs` の TODO) のため prepare 対象外。
#[derive(Copy, Clone, Debug)]
struct PreparedSphere {
    center: Vec3,
    world_radius: f32,
}

impl PreparedSphere {
    fn new(
        sphere: &Sphere,
        gtf: &GlobalTransform,
    ) -> Self {
        Self {
            center: gtf.transform_point(Vec3::from(sphere.offset)),
            world_radius: sphere.radius * gtf.scale().abs().max_element(),
        }
    }

    fn apply_collision(
        &self,
        next_tail: &mut Vec3,
        head_global_pos: Vec3,
        joint_radius: f32,
        bone_length: f32,
    ) {
        let r = joint_radius + self.world_radius;
        let delta = *next_tail - self.center;
        let distance_squared = delta.length_squared();
        if distance_squared > 0.0 && distance_squared <= r * r {
            let dir = delta.normalize();
            let pos_from_collider = self.center + dir * r;
            *next_tail =
                head_global_pos + (pos_from_collider - head_global_pos).normalize() * bone_length;
        }
    }
}

fn update_spring_bones(
    mut transforms: Query<(&mut Transform, &mut GlobalTransform)>,
    mut joints: Query<(&ChildOf, &mut SpringJointState, &SpringJointProps)>,
    spring_roots: Query<&SpringRoot>,
    time: Res<Time>,
    mut per_root: Local<Vec<PreparedSphere>>,
) {
    let delta_time = time.delta_secs();
    for spring_root in spring_roots.iter() {
        let center_gtf = spring_root
            .center_node
            .and_then(|center| transforms.get(center).ok())
            .map(|(_, gtf)| gtf)
            .copied();
        let center_inverse = center_gtf.map(|gtf| gtf.to_matrix().inverse());
        per_root.clear();
        per_root.extend(spring_root.colliders.iter().filter_map(|(entity, shape)| {
            let ColliderShape::Sphere(sphere) = shape else {
                return None;
            };
            let (_, gtf) = transforms.get(*entity).ok()?;
            Some(PreparedSphere::new(sphere, gtf))
        }));
        for joint in spring_root.joints.iter().copied() {
            let Ok((child_of, mut state, props)) = joints.get_mut(joint) else {
                continue;
            };
            let parent_gtf = transforms
                .get(child_of.parent())
                .map(|(_, gtf)| *gtf)
                .unwrap_or_default();
            let parent_global_rotation = parent_gtf.to_scale_rotation_translation().1;
            let Ok(head_global_pos) = transforms.get(joint).map(|(_, gtf)| gtf.translation())
            else {
                continue;
            };

            let current_tail = center_local_to_global(state.current_tail, &center_gtf);
            let prev_tail = center_local_to_global(state.prev_tail, &center_gtf);
            let inertia = (current_tail - prev_tail) * (1. - props.drag_force);
            let stiffness = delta_time
                * (parent_global_rotation
                    * state.initial_local_rotation
                    * state.bone_axis
                    * props.stiffness);
            let external = delta_time * props.gravity_dir * props.gravity_power;

            let next_tail = current_tail + inertia + stiffness + external;
            let mut next_tail =
                head_global_pos + (next_tail - head_global_pos).normalize() * state.bone_length;

            for sphere in per_root.iter() {
                sphere.apply_collision(
                    &mut next_tail,
                    head_global_pos,
                    props.hit_radius,
                    state.bone_length,
                );
            }

            state.prev_tail = state.current_tail;
            state.current_tail = global_to_center_local(next_tail, &center_inverse);

            let to = (parent_gtf.to_matrix() * state.initial_local_matrix)
                .inverse()
                .transform_point3(next_tail)
                .normalize();

            let Ok((mut tf, mut gtf)) = transforms.get_mut(joint) else {
                continue;
            };

            tf.rotation =
                state.initial_local_rotation * Quat::from_rotation_arc(state.bone_axis, to);
            *gtf = parent_gtf.mul_transform(*tf);
        }
    }
}

fn center_local_to_global(
    tail_pos: Vec3,
    center_gtf: &Option<GlobalTransform>,
) -> Vec3 {
    if let Some(gtf) = center_gtf.as_ref() {
        gtf.transform_point(tail_pos)
    } else {
        tail_pos
    }
}

/// `center_inverse` は root ごとに 1 回だけ計算した center 行列の逆行列
/// (joint loop 内で毎回逆行列を計算しないための hoist)。
fn global_to_center_local(
    tail_pos: Vec3,
    center_inverse: &Option<Mat4>,
) -> Vec3 {
    if let Some(inverse) = center_inverse.as_ref() {
        inverse.transform_point3(tail_pos)
    } else {
        tail_pos
    }
}

#[cfg(test)]
mod tests {
    use super::PreparedSphere;
    use crate::vrm::gltf::extensions::vrmc_spring_bone::Sphere;
    use bevy::math::{Quat, Vec3};
    use bevy::prelude::{GlobalTransform, Transform};

    const HEAD: Vec3 = Vec3::new(1.0, 2.5, 3.0);
    const JOINT_RADIUS: f32 = 0.05;
    const BONE_LENGTH: f32 = 0.8;

    /// 衝突解決後の不変条件を assert する:
    /// - tail は collider 中心から遠ざかる方向に押し出されている
    ///   (`bone_length` 再正規化があるため合成半径ちょうどまでは保証されない)
    /// - tail は head から `bone_length` の距離を維持している
    fn assert_collision_invariants(
        sphere: Sphere,
        gtf: GlobalTransform,
        initial_tail: Vec3,
    ) -> Vec3 {
        let prepared = PreparedSphere::new(&sphere, &gtf);
        let mut tail = initial_tail;
        prepared.apply_collision(&mut tail, HEAD, JOINT_RADIUS, BONE_LENGTH);
        assert!(
            tail.distance(prepared.center) > initial_tail.distance(prepared.center),
            "tail が collider から遠ざかっていない: {tail:?}"
        );
        assert!(
            (tail.distance(HEAD) - BONE_LENGTH).abs() < 1e-4,
            "bone_length が保存されていない: {}",
            tail.distance(HEAD)
        );
        tail
    }

    #[test]
    fn prepared_sphere_applies_offset_and_max_scale() {
        let sphere = Sphere {
            offset: [0.1, 0.2, 0.3],
            radius: 0.5,
        };
        let gtf = GlobalTransform::from(Transform {
            translation: Vec3::new(1.0, 2.0, 3.0),
            rotation: Quat::IDENTITY,
            scale: Vec3::new(1.5, 1.0, 1.0),
        });
        let prepared = PreparedSphere::new(&sphere, &gtf);
        assert!(
            prepared
                .center
                .abs_diff_eq(Vec3::new(1.0 + 0.1 * 1.5, 2.2, 3.3), 1e-5),
            "offset が world 空間に変換されていない: {:?}",
            prepared.center
        );
        assert!(
            (prepared.world_radius - 0.5 * 1.5).abs() < 1e-5,
            "radius に最大軸 scale が適用されていない: {}",
            prepared.world_radius
        );
    }

    #[test]
    fn collision_pushes_tail_away_from_collider() {
        let sphere = Sphere {
            offset: [0.0, 0.0, 0.0],
            radius: 1.0,
        };
        let gtf = GlobalTransform::from(Transform::from_translation(Vec3::new(1.0, 2.0, 3.0)));
        let initial = Vec3::new(1.2, 2.2, 3.1);
        let result = assert_collision_invariants(sphere, gtf, initial);
        // 衝突分岐が実際に発火していること
        assert_ne!(result, initial);
    }

    #[test]
    fn collision_invariants_hold_under_general_transform() {
        let sphere = Sphere {
            offset: [0.1, 0.2, 0.3],
            radius: 0.5,
        };
        let gtf = GlobalTransform::from(Transform {
            translation: Vec3::new(1.0, 2.0, 3.0),
            rotation: Quat::from_rotation_y(0.7),
            scale: Vec3::new(1.5, 1.0, 1.0),
        });
        assert_collision_invariants(sphere, gtf, Vec3::new(1.3, 2.4, 3.2));
    }

    #[test]
    fn no_collision_leaves_tail_untouched() {
        let sphere = Sphere {
            offset: [0.0, 0.0, 0.0],
            radius: 0.1,
        };
        let gtf = GlobalTransform::from(Transform::from_translation(Vec3::new(10.0, 10.0, 10.0)));
        let initial = Vec3::new(1.2, 2.2, 3.1);
        let mut tail = initial;
        PreparedSphere::new(&sphere, &gtf).apply_collision(
            &mut tail,
            HEAD,
            JOINT_RADIUS,
            BONE_LENGTH,
        );
        assert_eq!(tail, initial);
    }
}
