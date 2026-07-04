use crate::system_set::VrmSystemSets;
use crate::vrm::gltf::extensions::vrmc_spring_bone::{ColliderShape, Sphere};
use crate::vrm::spring_bone::{SpringJointProps, SpringJointState, SpringRoot};
use bevy::app::App;
use bevy::ecs::entity::EntityHashMap;
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
            update_spring_bones
                .in_set(VrmSystemSets::SpringBone)
                .after(VrmSystemSets::PropagateAfterExpressions),
        );
    }
}

/// frame 内で不変な collider の world 空間データ。
/// per-check の SRT 分解 + `transform_point` + Query lookup を frame 先頭 1 回に集約する。
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
        let (scale, _, _) = gtf.to_scale_rotation_translation();
        Self {
            center: gtf.transform_point(Vec3::from(sphere.offset)),
            world_radius: sphere.radius * scale.abs().max_element(),
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

#[derive(Default)]
struct PreparedColliders {
    map: EntityHashMap<PreparedSphere>,
    per_root: Vec<PreparedSphere>,
}

fn update_spring_bones(
    mut transforms: Query<(&mut Transform, &mut GlobalTransform)>,
    mut joints: Query<(&ChildOf, &mut SpringJointState, &SpringJointProps)>,
    spring_roots: Query<&SpringRoot>,
    time: Res<Time>,
    mut prepared: Local<PreparedColliders>,
) {
    let PreparedColliders { map, per_root } = &mut *prepared;
    map.clear();
    for spring_root in spring_roots.iter() {
        for (collider, shape) in spring_root.colliders.iter().copied() {
            let ColliderShape::Sphere(sphere) = shape else {
                continue;
            };
            if map.contains_key(&collider) {
                continue;
            }
            let Ok((_, gtf)) = transforms.get(collider) else {
                continue;
            };
            map.insert(collider, PreparedSphere::new(&sphere, gtf));
        }
    }

    let delta_time = time.delta_secs();
    for spring_root in spring_roots.iter() {
        let center_gtf = spring_root
            .center_node
            .and_then(|center| transforms.get(center).ok())
            .map(|(_, gtf)| gtf)
            .copied();
        per_root.clear();
        per_root.extend(
            spring_root
                .colliders
                .iter()
                .filter_map(|(entity, _)| map.get(entity).copied()),
        );
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
            state.current_tail = global_to_center_local(next_tail, &center_gtf);

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

fn global_to_center_local(
    tail_pos: Vec3,
    center_gtf: &Option<GlobalTransform>,
) -> Vec3 {
    if let Some(gtf) = center_gtf.as_ref() {
        gtf.to_matrix().inverse().transform_point3(tail_pos)
    } else {
        tail_pos
    }
}

#[cfg(test)]
mod tests {
    use super::PreparedSphere;
    use crate::vrm::gltf::extensions::vrmc_spring_bone::{ColliderShape, Sphere};
    use bevy::math::{Quat, Vec3};
    use bevy::prelude::{GlobalTransform, Transform};

    /// 旧 path (ColliderShape::apply_collision) と新 path (PreparedSphere) の
    /// next_tail 一致を assert する。期待値の手計算は不要 (= 等価性テスト)。
    fn assert_equivalent(
        sphere: Sphere,
        gtf: GlobalTransform,
        initial_tail: Vec3,
    ) -> Vec3 {
        let head = Vec3::new(1.0, 2.5, 3.0);
        let joint_radius = 0.05;
        let bone_length = 0.8;
        let mut old_tail = initial_tail;
        let mut new_tail = initial_tail;
        ColliderShape::Sphere(sphere).apply_collision(
            &mut old_tail,
            &gtf,
            head,
            joint_radius,
            bone_length,
        );
        PreparedSphere::new(&sphere, &gtf).apply_collision(
            &mut new_tail,
            head,
            joint_radius,
            bone_length,
        );
        assert_eq!(old_tail, new_tail);
        old_tail
    }

    #[test]
    fn collision_branch_fires_and_matches() {
        let sphere = Sphere {
            offset: [0.0, 0.0, 0.0],
            radius: 1.0,
        };
        let gtf = GlobalTransform::from(Transform::from_translation(Vec3::new(1.0, 2.0, 3.0)));
        let initial = Vec3::new(1.2, 2.2, 3.1);
        let result = assert_equivalent(sphere, gtf, initial);
        // 衝突分岐が実際に発火していること (= no-op 同士の空一致でない)
        assert_ne!(result, initial);
    }

    #[test]
    fn general_transform_matches() {
        let sphere = Sphere {
            offset: [0.1, 0.2, 0.3],
            radius: 0.5,
        };
        let gtf = GlobalTransform::from(Transform {
            translation: Vec3::new(1.0, 2.0, 3.0),
            rotation: Quat::from_rotation_y(0.7),
            scale: Vec3::new(1.5, 1.0, 1.0),
        });
        assert_equivalent(sphere, gtf, Vec3::new(1.3, 2.4, 3.2));
    }
}
