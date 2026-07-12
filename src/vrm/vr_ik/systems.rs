//! VR IK ECS systems: キャッシュ初期化 + フレーム毎 IK 適用。
//!
//! [`init_vr_ik_chain_cache`]: [`VrIk`] を持つ entity に [`VrIkChainCache`] を遅延挿入する。
//! [`apply_vr_ik`]: [`VrIkTargets`] の外部 pose から skeleton を毎フレーム駆動する。

use bevy::prelude::*;

use crate::vrm::humanoid_bone::bone_names;
use crate::vrm::humanoid_bone::prelude::HumanoidBoneEntities;
use crate::vrm::vr_ik::calibration::{VrIkRestPositions, build_vr_ik_chain_cache};
use crate::vrm::vr_ik::solver::{distribute_spine, estimate_hip, two_bone_ik};
use crate::vrm::vr_ik::{VrIk, VrIkChainCache, VrIkTargets};
use crate::vrm::{RestGlobalTransform, RestTransform};

/// VRM は +Z 前方 (glTF 座標のまま無変換ロード)。Ry(π) で Bevy/OpenXR の -Z 前方に合わせる。
const MODEL_FLIP: Quat = Quat::from_xyzw(0.0, 1.0, 0.0, 0.0);

/// [`VrIk`] を持つが [`VrIkChainCache`] をまだ持たない entity に対してキャッシュを初期化する。
///
/// 必須骨の [`RestGlobalTransform`] が未 spawn の場合はその entity をスキップする
/// (次フレーム以降で骨が揃っていれば自動的にキャッシュが挿入される)。
/// 必須骨が [`HumanoidBoneEntities`] に無い場合 (malformed VRM) は初期化が完了しないため
/// 一度だけ warn する。
pub(crate) fn init_vr_ik_chain_cache(
    mut commands: Commands,
    vrms: Query<(Entity, &HumanoidBoneEntities), (With<VrIk>, Without<VrIkChainCache>)>,
    rest_globals: Query<&RestGlobalTransform>,
    rest_locals: Query<&RestTransform>,
) {
    for (entity, bones) in &vrms {
        let rest_pos = |bone: &str| -> Option<Vec3> {
            bones
                .find(bone)
                .and_then(|e| rest_globals.get(e).ok())
                .map(|gtf| gtf.translation())
        };
        // 必須骨用: HumanoidBoneEntities は骨解決完了後に一括構築されるため、map に
        // 名前が無い = 「未 spawn」ではなくこの VRM に該当骨が存在しない (malformed VRM)。
        // silent skip だと IK 不発の原因が特定不能になるため一度だけ warn する
        // (RestGlobalTransform 未 spawn の通常リトライは silent のまま)。
        let required_pos = |bone: &str| -> Option<Vec3> {
            if bones.find(bone).is_none() {
                #[cfg(feature = "log")]
                bevy::log::warn_once!(
                    "[VrIk] required humanoid bone {bone:?} missing from HumanoidBoneEntities \
                     (malformed VRM); IK init will never complete"
                );
            }
            rest_pos(bone)
        };

        let Some(head) = required_pos(bone_names::HEAD) else {
            continue;
        };
        let Some(hips) = required_pos(bone_names::HIPS) else {
            continue;
        };
        let Some(left_upper_arm) = required_pos(bone_names::LEFT_UPPER_ARM) else {
            continue;
        };
        let Some(left_lower_arm) = required_pos(bone_names::LEFT_LOWER_ARM) else {
            continue;
        };
        let Some(left_hand) = required_pos(bone_names::LEFT_HAND) else {
            continue;
        };
        let Some(right_upper_arm) = required_pos(bone_names::RIGHT_UPPER_ARM) else {
            continue;
        };
        let Some(right_lower_arm) = required_pos(bone_names::RIGHT_LOWER_ARM) else {
            continue;
        };
        let Some(right_hand) = required_pos(bone_names::RIGHT_HAND) else {
            continue;
        };

        let mut cache = build_vr_ik_chain_cache(&VrIkRestPositions {
            head,
            neck: rest_pos(bone_names::NECK),
            chest: rest_pos(bone_names::CHEST),
            spine: rest_pos(bone_names::SPINE),
            hips,
            left_shoulder: rest_pos(bone_names::LEFT_SHOULDER),
            left_upper_arm,
            left_lower_arm,
            left_hand,
            right_shoulder: rest_pos(bone_names::RIGHT_SHOULDER),
            right_upper_arm,
            right_lower_arm,
            right_hand,
            left_upper_leg: rest_pos(bone_names::LEFT_UPPER_LEG),
            left_lower_leg: rest_pos(bone_names::LEFT_LOWER_LEG),
            left_foot: rest_pos(bone_names::LEFT_FOOT),
            right_upper_leg: rest_pos(bone_names::RIGHT_UPPER_LEG),
            right_lower_leg: rest_pos(bone_names::RIGHT_LOWER_LEG),
            right_foot: rest_pos(bone_names::RIGHT_FOOT),
        });

        let bone_axis_from_rest = |bone: &str| -> Option<Vec3> {
            bones
                .find(bone)
                .and_then(|e| rest_locals.get(e).ok())
                .map(|rt| rt.translation.normalize_or_zero())
                .filter(|a| a.length() > 0.5)
        };

        let l_arm_axis = bone_axis_from_rest(bone_names::LEFT_LOWER_ARM).unwrap_or(Vec3::Y);
        let r_arm_axis = bone_axis_from_rest(bone_names::RIGHT_LOWER_ARM).unwrap_or(Vec3::Y);
        cache.arm_axis_correction = (
            Quat::from_rotation_arc(l_arm_axis, Vec3::Y),
            Quat::from_rotation_arc(r_arm_axis, Vec3::Y),
        );
        cache.arm_hand_correction = (
            Quat::from_rotation_arc(l_arm_axis, Vec3::NEG_Z),
            Quat::from_rotation_arc(r_arm_axis, Vec3::NEG_Z),
        );

        if let Some(ref mut legs) = cache.legs {
            let l_leg_axis = bone_axis_from_rest(bone_names::LEFT_LOWER_LEG).unwrap_or(Vec3::NEG_Y);
            let r_leg_axis =
                bone_axis_from_rest(bone_names::RIGHT_LOWER_LEG).unwrap_or(Vec3::NEG_Y);
            legs.leg_axis_correction = (
                Quat::from_rotation_arc(l_leg_axis, Vec3::Y),
                Quat::from_rotation_arc(r_leg_axis, Vec3::Y),
            );
        }

        commands.entity(entity).insert(cache);
    }
}

/// 毎フレーム [`VrIkTargets`] の外部 pose から VRM skeleton を駆動する。
///
/// `targets.head` が `None` (= HMD 未接続相当) の entity はスキップする。
/// 骨 Transform 取得失敗時はその骨をスキップする。
/// 必須骨が [`HumanoidBoneEntities`] に無い場合はその entity をスキップする
/// ([`VrIkChainCache`] 構築済み entity では実際には揃っている)。
pub(crate) fn apply_vr_ik(
    vrms: Query<(&VrIk, &VrIkTargets, &VrIkChainCache, &HumanoidBoneEntities)>,
    mut transforms: Query<&mut Transform>,
    rest_transforms: Query<(&RestTransform, &RestGlobalTransform)>,
) {
    for (vik, targets, cache, bones) in &vrms {
        let Some(head_target) = targets.head else {
            continue;
        };
        let Some(head_bone) = bones.find(bone_names::HEAD) else {
            continue;
        };
        let Some(hips_bone) = bones.find(bone_names::HIPS) else {
            continue;
        };
        let Some(left_upper_arm_bone) = bones.find(bone_names::LEFT_UPPER_ARM) else {
            continue;
        };
        let Some(left_lower_arm_bone) = bones.find(bone_names::LEFT_LOWER_ARM) else {
            continue;
        };
        let Some(left_hand_bone) = bones.find(bone_names::LEFT_HAND) else {
            continue;
        };
        let Some(right_upper_arm_bone) = bones.find(bone_names::RIGHT_UPPER_ARM) else {
            continue;
        };
        let Some(right_lower_arm_bone) = bones.find(bone_names::RIGHT_LOWER_ARM) else {
            continue;
        };
        let Some(right_hand_bone) = bones.find(bone_names::RIGHT_HAND) else {
            continue;
        };
        // 1. Hip 位置・姿勢を推定して書き込む
        let hip_xz_offset = Vec3::new(cache.hip_xz_offset.0, 0.0, cache.hip_xz_offset.1);
        let (hip_pos, hip_rot) = estimate_hip(
            head_target.translation,
            head_target.rotation,
            cache.hip_height_ratio,
            hip_xz_offset,
        );
        if let Ok(mut tf) = transforms.get_mut(hips_bone) {
            tf.translation = hip_pos;
            tf.rotation = hip_rot * MODEL_FLIP;
        }

        // 2. Spine chain に分配された回転差分を適用
        let deltas = distribute_spine(hip_rot, head_target.rotation, &vik.spine_weights);

        // 活性骨: spine → chest → neck → head 順 (spine/chest/neck は optional)
        let spine_chain_bones: &[Option<Entity>] = &[
            bones.find(bone_names::SPINE),
            bones.find(bone_names::CHEST),
            bones.find(bone_names::NECK),
            Some(head_bone),
        ];

        // distribute_spine は weights.len() = 4 で常に 4 要素返す。
        // 骨が None の分は delta をそのまま捨てる (total weight < 1 は許容)
        for (maybe_bone_entity, (yaw_delta, pitch_delta)) in
            spine_chain_bones.iter().zip(deltas.iter())
        {
            let Some(bone_entity) = maybe_bone_entity else {
                continue;
            };
            let Ok((rest_tf, _rest_gtf)) = rest_transforms.get(*bone_entity) else {
                continue;
            };
            let Ok(mut tf) = transforms.get_mut(*bone_entity) else {
                continue;
            };
            tf.rotation =
                rest_tf.rotation * Quat::from_euler(EulerRot::YXZ, *yaw_delta, *pitch_delta, 0.0);
        }

        // 3. Arm IK (各コントローラ独立で適用)
        // shoulder_offset は rest pose (+Z 前方) で計測。runtime は Ry(π) で骨格の
        // X,Z が反転するため、offset を model_flip で回転して実際のボーン位置に合わせる。
        if let Some(left_target) = targets.left_hand {
            let left_shoulder_world = head_target.translation
                + head_target.rotation * (MODEL_FLIP * cache.shoulder_offset.0);
            apply_arm_ik(
                left_shoulder_world,
                left_target.translation,
                left_target.rotation,
                cache.upper_arm_len.0,
                cache.lower_arm_len.0,
                Vec3::NEG_Y,
                hip_rot,
                cache.arm_axis_correction.0,
                cache.arm_hand_correction.0,
                left_upper_arm_bone,
                left_lower_arm_bone,
                left_hand_bone,
                &mut transforms,
                &rest_transforms,
            );
        }

        if let Some(right_target) = targets.right_hand {
            let right_shoulder_world = head_target.translation
                + head_target.rotation * (MODEL_FLIP * cache.shoulder_offset.1);
            apply_arm_ik(
                right_shoulder_world,
                right_target.translation,
                right_target.rotation,
                cache.upper_arm_len.1,
                cache.lower_arm_len.1,
                Vec3::NEG_Y,
                hip_rot,
                cache.arm_axis_correction.1,
                cache.arm_hand_correction.1,
                right_upper_arm_bone,
                right_lower_arm_bone,
                right_hand_bone,
                &mut transforms,
                &rest_transforms,
            );
        }

        // 4. Leg IK (脚骨が揃っている場合のみ)
        let leg_entities = (|| {
            Some((
                bones.find(bone_names::LEFT_UPPER_LEG)?,
                bones.find(bone_names::LEFT_LOWER_LEG)?,
                bones.find(bone_names::RIGHT_UPPER_LEG)?,
                bones.find(bone_names::RIGHT_LOWER_LEG)?,
            ))
        })();
        if let (Some(legs), Some((l_upper, l_lower, r_upper, r_lower))) =
            (&cache.legs, leg_entities)
        {
            let step = &targets.foot_step;

            apply_leg_ik(
                hip_pos,
                hip_rot,
                legs.upper_leg_offset.0,
                legs.foot_offset.0,
                legs.upper_leg_len.0,
                legs.lower_leg_len.0,
                legs.leg_axis_correction.0,
                l_upper,
                l_lower,
                step.left_offset_xz,
                step.left_height,
                &mut transforms,
            );

            apply_leg_ik(
                hip_pos,
                hip_rot,
                legs.upper_leg_offset.1,
                legs.foot_offset.1,
                legs.upper_leg_len.1,
                legs.lower_leg_len.1,
                legs.leg_axis_correction.1,
                r_upper,
                r_lower,
                step.right_offset_xz,
                step.right_height,
                &mut transforms,
            );

            // foot 骨は lower_leg の子として自動追従 (POC 品質)
        }
    }
}

/// 片脚の `two_bone_ik` を解いて Transform を書き込むヘルパ。
///
/// Bone axis correction: VRM 脚 bone axis = -Y (arm の ±X と異なる)。
/// キャッシュ済みの `axis_correction` を solver 出力に適用する。
///
/// `step_offset_xz`: 歩行 XZ オフセット (Y 成分は無視)。
/// `step_height`: 足上げ Y オフセット (foot target の Y 絶対値。床 y=0 前提)。
///
/// foot 骨は設定しない (`lower_leg` の子として自動追従、POC 品質)。
fn apply_leg_ik(
    hip_world_pos: Vec3,
    hip_rotation: Quat,
    upper_leg_offset: Vec3,
    foot_offset: Vec3,
    upper_len: f32,
    lower_len: f32,
    axis_correction: Quat,
    upper_leg_entity: Entity,
    lower_leg_entity: Entity,
    step_offset_xz: Vec3,
    step_height: f32,
    transforms: &mut Query<&mut Transform>,
) {
    // upper_leg joint の world 位置
    let upper_leg_joint = hip_world_pos + hip_rotation * MODEL_FLIP * upper_leg_offset;

    // 足先 target: rest offset を hip yaw + model_flip で回転 + step offset
    let foot_xz_rotated = hip_rotation * MODEL_FLIP * foot_offset;
    let foot_target = Vec3::new(
        hip_world_pos.x + foot_xz_rotated.x + step_offset_xz.x,
        step_height,
        hip_world_pos.z + foot_xz_rotated.z + step_offset_xz.z,
    );

    // pole_vector: hip 前方 (膝がキャラ前方に曲がる)
    let pole_vector = hip_rotation * Vec3::NEG_Z;

    let (upper_solver_rot, lower_solver_rot) = two_bone_ik(
        upper_leg_joint,
        foot_target,
        upper_len,
        lower_len,
        pole_vector,
    );

    let upper_leg_world = upper_solver_rot * axis_correction;
    let lower_leg_world = lower_solver_rot * axis_correction;

    // Upper leg: parent = hips 骨、hips の world rot = hip_rotation * model_flip
    if let Ok(mut tf) = transforms.get_mut(upper_leg_entity) {
        let hips_world_rot = hip_rotation * MODEL_FLIP;
        tf.rotation = hips_world_rot.inverse() * upper_leg_world;
    }

    // Lower leg: parent world rot = upper_leg world rot
    if let Ok(mut tf) = transforms.get_mut(lower_leg_entity) {
        tf.rotation = upper_leg_world.inverse() * lower_leg_world;
    }
}

/// 片腕の `two_bone_ik` を解いて Transform を書き込むヘルパ。
///
/// Bone axis correction: solver は Y = bone direction 規約だが、VRM 骨の子は
/// X 軸方向 (L: +X, R: -X) に配置されている。キャッシュ済みの `axis_correction` を
/// solver 出力に適用する。
///
/// Upper arm: world→local は `hip_rotation * model_flip * rest parent` 近似。
/// Lower arm / hand: parent world rot は補正済み solver 出力から正確に得られる。
/// Hand: キャッシュ済みの `hand_correction` で controller forward (-Z) に揃える。
fn apply_arm_ik(
    shoulder_world: Vec3,
    wrist_target: Vec3,
    controller_rotation: Quat,
    upper_len: f32,
    lower_len: f32,
    pole_vector: Vec3,
    hip_rotation: Quat,
    axis_correction: Quat,
    hand_correction: Quat,
    upper_arm_entity: Entity,
    lower_arm_entity: Entity,
    hand_entity: Entity,
    transforms: &mut Query<&mut Transform>,
    rest_transforms: &Query<(&RestTransform, &RestGlobalTransform)>,
) {
    let (upper_solver_rot, lower_solver_rot) = two_bone_ik(
        shoulder_world,
        wrist_target,
        upper_len,
        lower_len,
        pole_vector,
    );

    let upper_bone_world = upper_solver_rot * axis_correction;
    let lower_bone_world = lower_solver_rot * axis_correction;

    // Upper arm: hip_rotation * model_flip * rest parent 近似 (spine delta は無視)
    if let (Ok(mut tf), Ok((rest_tf, rest_gtf))) = (
        transforms.get_mut(upper_arm_entity),
        rest_transforms.get(upper_arm_entity),
    ) {
        let parent_rest_world = rest_gtf.rotation() * rest_tf.rotation.inverse();
        let corrected_parent = hip_rotation * MODEL_FLIP * parent_rest_world;
        tf.rotation = corrected_parent.inverse() * upper_bone_world;
    }

    // Lower arm: parent = upper arm 骨の world rotation
    if let Ok(mut tf) = transforms.get_mut(lower_arm_entity) {
        tf.rotation = upper_bone_world.inverse() * lower_bone_world;
    }

    // Hand: align VRM rest finger axis (±X) with controller forward (-Z)
    if let Ok(mut tf) = transforms.get_mut(hand_entity) {
        tf.rotation = lower_bone_world.inverse() * controller_rotation * hand_correction;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vrm::VrmBone;
    use crate::vrm::vr_ik::{VrIkFootStep, VrIkPlugin, VrIkPose};
    use bevy::app::App;
    use bevy::platform::collections::HashMap;

    /// 最小 humanoid 骨格 (T-pose、rest = 現在値) を spawn する共有 fixture。
    ///
    /// 骨は階層を持たない平坦な entity 群 (IK システムは propagation を使わず
    /// `RestTransform`/`RestGlobalTransform` のみ読むため十分)。
    /// 座標は calibration テストと同一 (head y=1.7 / hips y=1.0 / 腕長 0.3+0.3 / 脚長 0.45+0.5)。
    struct Skeleton {
        root: Entity,
        bones: HashMap<&'static str, Entity>,
    }

    struct SkeletonOptions {
        with_legs: bool,
        with_chest_neck: bool,
        with_rest_globals: bool,
    }

    impl Default for SkeletonOptions {
        fn default() -> Self {
            Self {
                with_legs: true,
                with_chest_neck: true,
                with_rest_globals: true,
            }
        }
    }

    fn bone_world_positions(opts: &SkeletonOptions) -> Vec<(&'static str, Vec3)> {
        let mut positions = vec![
            (bone_names::HIPS, Vec3::new(0.0, 1.0, 0.0)),
            (bone_names::SPINE, Vec3::new(0.0, 1.2, 0.0)),
            (bone_names::HEAD, Vec3::new(0.0, 1.7, 0.0)),
            (bone_names::LEFT_SHOULDER, Vec3::new(-0.15, 1.55, 0.0)),
            (bone_names::LEFT_UPPER_ARM, Vec3::new(-0.2, 1.5, 0.0)),
            (bone_names::LEFT_LOWER_ARM, Vec3::new(-0.5, 1.5, 0.0)),
            (bone_names::LEFT_HAND, Vec3::new(-0.8, 1.5, 0.0)),
            (bone_names::RIGHT_SHOULDER, Vec3::new(0.15, 1.55, 0.0)),
            (bone_names::RIGHT_UPPER_ARM, Vec3::new(0.2, 1.5, 0.0)),
            (bone_names::RIGHT_LOWER_ARM, Vec3::new(0.5, 1.5, 0.0)),
            (bone_names::RIGHT_HAND, Vec3::new(0.8, 1.5, 0.0)),
        ];
        if opts.with_chest_neck {
            positions.push((bone_names::CHEST, Vec3::new(0.0, 1.4, 0.0)));
            positions.push((bone_names::NECK, Vec3::new(0.0, 1.6, 0.0)));
        }
        if opts.with_legs {
            positions.extend([
                (bone_names::LEFT_UPPER_LEG, Vec3::new(-0.1, 0.95, 0.0)),
                (bone_names::LEFT_LOWER_LEG, Vec3::new(-0.1, 0.5, 0.0)),
                (bone_names::LEFT_FOOT, Vec3::new(-0.1, 0.0, 0.0)),
                (bone_names::RIGHT_UPPER_LEG, Vec3::new(0.1, 0.95, 0.0)),
                (bone_names::RIGHT_LOWER_LEG, Vec3::new(0.1, 0.5, 0.0)),
                (bone_names::RIGHT_FOOT, Vec3::new(0.1, 0.0, 0.0)),
            ]);
        }
        positions
    }

    /// 各骨の「親」(rest local translation の基準)。bone axis 補正の導出元。
    fn parent_of(name: &str) -> &'static str {
        match name {
            x if x == bone_names::SPINE => bone_names::HIPS,
            x if x == bone_names::CHEST => bone_names::SPINE,
            x if x == bone_names::NECK => bone_names::CHEST,
            x if x == bone_names::HEAD => bone_names::NECK,
            x if x == bone_names::LEFT_SHOULDER => bone_names::CHEST,
            x if x == bone_names::LEFT_UPPER_ARM => bone_names::LEFT_SHOULDER,
            x if x == bone_names::LEFT_LOWER_ARM => bone_names::LEFT_UPPER_ARM,
            x if x == bone_names::LEFT_HAND => bone_names::LEFT_LOWER_ARM,
            x if x == bone_names::RIGHT_SHOULDER => bone_names::CHEST,
            x if x == bone_names::RIGHT_UPPER_ARM => bone_names::RIGHT_SHOULDER,
            x if x == bone_names::RIGHT_LOWER_ARM => bone_names::RIGHT_UPPER_ARM,
            x if x == bone_names::RIGHT_HAND => bone_names::RIGHT_LOWER_ARM,
            x if x == bone_names::LEFT_UPPER_LEG => bone_names::HIPS,
            x if x == bone_names::LEFT_LOWER_LEG => bone_names::LEFT_UPPER_LEG,
            x if x == bone_names::LEFT_FOOT => bone_names::LEFT_LOWER_LEG,
            x if x == bone_names::RIGHT_UPPER_LEG => bone_names::HIPS,
            x if x == bone_names::RIGHT_LOWER_LEG => bone_names::RIGHT_UPPER_LEG,
            x if x == bone_names::RIGHT_FOOT => bone_names::RIGHT_LOWER_LEG,
            _ => bone_names::HIPS,
        }
    }

    fn spawn_skeleton(
        world: &mut World,
        opts: SkeletonOptions,
    ) -> Skeleton {
        let positions = bone_world_positions(&opts);
        let world_pos: HashMap<&'static str, Vec3> = positions.iter().copied().collect();
        let mut bones = HashMap::new();
        let mut map = HumanoidBoneEntities::default();
        for (name, pos) in &positions {
            // local = world - parent world (hips 自身は world = local)。
            // optional 骨を欠いた fixture では、存在する祖先まで親を遡る
            let local = if *name == bone_names::HIPS {
                *pos
            } else {
                let mut parent = parent_of(name);
                while !world_pos.contains_key(parent) && parent != bone_names::HIPS {
                    parent = parent_of(parent);
                }
                *pos - world_pos[parent]
            };
            let mut entity = world.spawn((
                Transform::from_translation(local),
                RestTransform(Transform::from_translation(local)),
            ));
            if opts.with_rest_globals {
                entity.insert(RestGlobalTransform(GlobalTransform::from_translation(
                    *pos,
                )));
            }
            let id = entity.id();
            bones.insert(*name, id);
            map.0.insert(VrmBone::from(*name), id);
        }
        let root = world.spawn((VrIk::default(), map)).id();
        Skeleton { root, bones }
    }

    fn test_ik_app() -> App {
        let mut app = App::new();
        app.add_plugins(VrIkPlugin);
        app
    }

    fn head_pose_upright() -> VrIkPose {
        VrIkPose {
            translation: Vec3::new(0.0, 1.7, 0.0),
            rotation: Quat::IDENTITY,
        }
    }

    fn set_targets(
        app: &mut App,
        root: Entity,
        f: impl FnOnce(&mut VrIkTargets),
    ) {
        let mut targets = app
            .world_mut()
            .get_mut::<VrIkTargets>(root)
            .expect("VrIkTargets should exist (required component)");
        f(&mut targets);
    }

    fn bone_rotation_of(
        app: &App,
        skeleton: &Skeleton,
        name: &str,
    ) -> Quat {
        app.world()
            .get::<Transform>(skeleton.bones[name])
            .unwrap()
            .rotation
    }

    // === キャリブレーション自動挿入 ===

    #[test]
    fn cache_inserted_when_bones_ready() {
        let mut app = test_ik_app();
        let skeleton = spawn_skeleton(app.world_mut(), SkeletonOptions::default());
        app.update();
        let cache = app.world().get::<VrIkChainCache>(skeleton.root);
        let cache = cache.expect("cache should be inserted");
        assert!((cache.hip_height_ratio - 1.0 / 1.7).abs() < 0.01);
        assert!((cache.upper_arm_len.0 - 0.3).abs() < 0.01);
        assert!(cache.legs.is_some());
    }

    #[test]
    fn cache_insertion_retries_until_rest_globals_spawn() {
        let mut app = test_ik_app();
        let skeleton = spawn_skeleton(
            app.world_mut(),
            SkeletonOptions {
                with_rest_globals: false,
                ..Default::default()
            },
        );
        app.update();
        assert!(
            app.world().get::<VrIkChainCache>(skeleton.root).is_none(),
            "cache should not be inserted before RestGlobalTransform spawns"
        );
        // RestGlobalTransform を後から挿入 → 次の update で cache が入る (リトライ)
        let positions = bone_world_positions(&SkeletonOptions::default());
        for (name, pos) in positions {
            let entity = skeleton.bones[name];
            app.world_mut()
                .entity_mut(entity)
                .insert(RestGlobalTransform(GlobalTransform::from_translation(pos)));
        }
        app.update();
        assert!(
            app.world().get::<VrIkChainCache>(skeleton.root).is_some(),
            "cache should be inserted after RestGlobalTransform spawns"
        );
    }

    #[test]
    fn cache_not_inserted_when_required_bone_missing() {
        let mut app = test_ik_app();
        let skeleton = spawn_skeleton(app.world_mut(), SkeletonOptions::default());
        // 必須骨 leftHand を HumanoidBoneEntities から欠落させる
        app.world_mut()
            .get_mut::<HumanoidBoneEntities>(skeleton.root)
            .unwrap()
            .0
            .remove(&VrmBone::from(bone_names::LEFT_HAND));
        app.update();
        app.update();
        assert!(
            app.world().get::<VrIkChainCache>(skeleton.root).is_none(),
            "cache should not be inserted when a required bone is missing"
        );
    }

    #[test]
    fn cache_without_legs_when_leg_bones_missing() {
        let mut app = test_ik_app();
        let skeleton = spawn_skeleton(
            app.world_mut(),
            SkeletonOptions {
                with_legs: false,
                ..Default::default()
            },
        );
        app.update();
        let cache = app
            .world()
            .get::<VrIkChainCache>(skeleton.root)
            .expect("cache should be inserted without legs");
        assert!(cache.legs.is_none());
    }

    #[test]
    fn vr_ik_requires_targets() {
        let mut app = test_ik_app();
        let entity = app.world_mut().spawn(VrIk::default()).id();
        assert!(
            app.world().get::<VrIkTargets>(entity).is_some(),
            "VrIkTargets should be auto-inserted as a required component"
        );
    }

    #[test]
    fn default_spine_weights() {
        let vik = VrIk::default();
        assert_eq!(vik.spine_weights, [0.15, 0.2, 0.25, 0.4]);
    }

    #[test]
    fn cache_survives_vr_ik_removal_and_reinsert_resumes() {
        let mut app = test_ik_app();
        let skeleton = spawn_skeleton(app.world_mut(), SkeletonOptions::default());
        set_targets(&mut app, skeleton.root, |t| {
            t.head = Some(head_pose_upright());
        });
        app.update();
        assert!(app.world().get::<VrIkChainCache>(skeleton.root).is_some());

        app.world_mut().entity_mut(skeleton.root).remove::<VrIk>();
        app.update();
        assert!(
            app.world().get::<VrIkChainCache>(skeleton.root).is_some(),
            "cache should survive VrIk removal"
        );

        // hips を rest に戻してから再 insert → 再キャリブレーションなしで即再開
        let hips = skeleton.bones[bone_names::HIPS];
        app.world_mut().get_mut::<Transform>(hips).unwrap().rotation = Quat::IDENTITY;
        app.world_mut()
            .entity_mut(skeleton.root)
            .insert(VrIk::default());
        app.update();
        let hips_rot = bone_rotation_of(&app, &skeleton, bone_names::HIPS);
        assert!(
            hips_rot.angle_between(Quat::from_rotation_y(std::f32::consts::PI)) < 0.01,
            "IK should resume immediately after re-insert"
        );
    }

    // === apply (毎フレーム適用) ===

    #[test]
    fn hips_written_from_head_target() {
        let mut app = test_ik_app();
        let skeleton = spawn_skeleton(app.world_mut(), SkeletonOptions::default());
        set_targets(&mut app, skeleton.root, |t| {
            t.head = Some(head_pose_upright());
        });
        app.update();
        let hips = app
            .world()
            .get::<Transform>(skeleton.bones[bone_names::HIPS])
            .unwrap();
        // hmd.y=1.7 × ratio(1/1.7) = 1.0、XZ オフセットなし
        assert!(
            (hips.translation - Vec3::new(0.0, 1.0, 0.0)).length() < 0.01,
            "hips translation: {:?}",
            hips.translation
        );
        // yaw=0 → hips 回転 = model_flip (Ry(π))。VRM は +Z 前方なので Bevy -Z 前方を向く
        assert!(
            hips.rotation
                .angle_between(Quat::from_rotation_y(std::f32::consts::PI))
                < 0.01,
            "hips rotation should be Ry(π), got {:?}",
            hips.rotation
        );
    }

    #[test]
    fn spine_distribution_follows_weights() {
        let mut app = test_ik_app();
        let skeleton = spawn_skeleton(app.world_mut(), SkeletonOptions::default());
        let pitch = -std::f32::consts::FRAC_PI_6; // OpenXR 前傾 30°
        set_targets(&mut app, skeleton.root, |t| {
            t.head = Some(VrIkPose {
                translation: Vec3::new(0.0, 1.7, 0.0),
                rotation: Quat::from_euler(EulerRot::YXZ, 0.0, pitch, 0.0),
            });
        });
        app.update();
        // 各骨は rest (identity) × X 軸回転 (+30° × weight)。pitch は VRM +Z 前方のため符号反転
        let weights = VrIk::default().spine_weights;
        for (i, name) in [
            bone_names::SPINE,
            bone_names::CHEST,
            bone_names::NECK,
            bone_names::HEAD,
        ]
        .iter()
        .enumerate()
        {
            let rot = bone_rotation_of(&app, &skeleton, name);
            let expected = Quat::from_rotation_x(-pitch * weights[i]);
            assert!(
                rot.angle_between(expected) < 0.01,
                "{name}: got {rot:?}, expected {expected:?}"
            );
        }
    }

    #[test]
    fn spine_distribution_drops_missing_optional_bones() {
        let mut app = test_ik_app();
        let skeleton = spawn_skeleton(
            app.world_mut(),
            SkeletonOptions {
                with_chest_neck: false,
                ..Default::default()
            },
        );
        let pitch = -std::f32::consts::FRAC_PI_6;
        set_targets(&mut app, skeleton.root, |t| {
            t.head = Some(VrIkPose {
                translation: Vec3::new(0.0, 1.7, 0.0),
                rotation: Quat::from_euler(EulerRot::YXZ, 0.0, pitch, 0.0),
            });
        });
        app.update();
        // chest/neck 分の delta は捨てられ、spine と head は自分の weight のまま (再分配されない)
        let weights = VrIk::default().spine_weights;
        let spine_rot = bone_rotation_of(&app, &skeleton, bone_names::SPINE);
        let head_rot = bone_rotation_of(&app, &skeleton, bone_names::HEAD);
        assert!(
            spine_rot.angle_between(Quat::from_rotation_x(-pitch * weights[0])) < 0.01,
            "spine should keep its own weight"
        );
        assert!(
            head_rot.angle_between(Quat::from_rotation_x(-pitch * weights[3])) < 0.01,
            "head should keep its own weight (no redistribution)"
        );
    }

    #[test]
    fn no_bone_written_when_head_target_none() {
        #[derive(Resource, Default)]
        struct ChangedBones(usize);
        fn count_changed(
            changed: Query<(), Changed<Transform>>,
            mut counter: ResMut<ChangedBones>,
        ) {
            counter.0 = changed.iter().count();
        }

        let mut app = test_ik_app();
        app.init_resource::<ChangedBones>();
        app.add_systems(
            PostUpdate,
            count_changed.after(crate::vrm::vr_ik::VrIkSystems),
        );
        let skeleton = spawn_skeleton(app.world_mut(), SkeletonOptions::default());
        app.update(); // spawn フレーム (spawn 由来の Changed は無視)
        app.update(); // head: None のまま → IK は何も書かない
        assert_eq!(
            app.world().resource::<ChangedBones>().0,
            0,
            "no Transform should be changed when head target is None"
        );
        let _ = skeleton;
    }

    #[test]
    fn arm_skipped_per_side_when_hand_target_none() {
        for (given_side, skipped_bones, written_bone) in [
            (
                "left",
                [
                    bone_names::RIGHT_UPPER_ARM,
                    bone_names::RIGHT_LOWER_ARM,
                    bone_names::RIGHT_HAND,
                ],
                bone_names::LEFT_UPPER_ARM,
            ),
            (
                "right",
                [
                    bone_names::LEFT_UPPER_ARM,
                    bone_names::LEFT_LOWER_ARM,
                    bone_names::LEFT_HAND,
                ],
                bone_names::RIGHT_UPPER_ARM,
            ),
        ] {
            let mut app = test_ik_app();
            let skeleton = spawn_skeleton(app.world_mut(), SkeletonOptions::default());
            let hand_pose = VrIkPose {
                translation: Vec3::new(0.0, 1.2, -0.3),
                rotation: Quat::IDENTITY,
            };
            set_targets(&mut app, skeleton.root, |t| {
                t.head = Some(head_pose_upright());
                if given_side == "left" {
                    t.left_hand = Some(hand_pose);
                } else {
                    t.right_hand = Some(hand_pose);
                }
            });
            app.update();
            for name in skipped_bones {
                let rot = bone_rotation_of(&app, &skeleton, name);
                assert!(
                    rot.angle_between(Quat::IDENTITY) < 1e-6,
                    "[{given_side}] {name} should stay at rest"
                );
            }
            let rot = bone_rotation_of(&app, &skeleton, written_bone);
            assert!(
                rot.angle_between(Quat::IDENTITY) > 0.01,
                "[{given_side}] {written_bone} should be written"
            );
        }
    }

    /// fixture の rest 階層 (identity 回転) から、書かれた local 回転を world 合成する。
    fn composed_world_rotation(
        app: &App,
        skeleton: &Skeleton,
        chain: &[&str],
    ) -> Quat {
        let mut rot = Quat::IDENTITY;
        for name in chain {
            rot *= bone_rotation_of(app, skeleton, name);
        }
        rot
    }

    #[test]
    fn arm_ik_reaches_target_with_bone_axis_correction() {
        let mut app = test_ik_app();
        let skeleton = spawn_skeleton(app.world_mut(), SkeletonOptions::default());
        // 左肩 world: head(0,1.7,0) + flip(shoulder_offset(-0.15,-0.15,0)) = (0.15,1.55,0)
        let shoulder_world = Vec3::new(0.15, 1.55, 0.0);
        let wrist_target = shoulder_world + Vec3::new(0.0, 0.0, -0.4); // 前方 (到達可能: 0.4 < 0.6)
        set_targets(&mut app, skeleton.root, |t| {
            t.head = Some(head_pose_upright());
            t.left_hand = Some(VrIkPose {
                translation: wrist_target,
                rotation: Quat::IDENTITY,
            });
        });
        app.update();

        // upper arm world 回転を再構成: corrected_parent × local
        // (親 = shoulder。fixture は rest 回転が全て identity なので
        // parent_rest_world = identity、corrected_parent = model_flip)
        let flip = Quat::from_rotation_y(std::f32::consts::PI);
        let upper_local = bone_rotation_of(&app, &skeleton, bone_names::LEFT_UPPER_ARM);
        let upper_world = flip * upper_local;
        // bone axis = lower_arm rest local translation = -X (左腕)
        let bone_axis = Vec3::NEG_X;
        let upper_dir = upper_world * bone_axis;
        let elbow = shoulder_world + upper_dir * 0.3;
        // 肘は肩から upper_len、手首 target から lower_len の距離にある (IK の幾何性質)
        assert!(
            ((elbow - shoulder_world).length() - 0.3).abs() < 0.02,
            "shoulder-elbow distance"
        );
        assert!(
            ((elbow - wrist_target).length() - 0.3).abs() < 0.02,
            "elbow-wrist distance: elbow={elbow:?}"
        );
    }

    #[test]
    fn arm_ik_correct_with_hip_yaw() {
        let mut app = test_ik_app();
        let skeleton = spawn_skeleton(app.world_mut(), SkeletonOptions::default());
        let yaw = std::f32::consts::FRAC_PI_4; // 45° 右回転
        let head_rot = Quat::from_rotation_y(yaw);
        // 肩 world: head + head_rot * (MODEL_FLIP * shoulder_offset)
        let flip = Quat::from_rotation_y(std::f32::consts::PI);
        let shoulder_offset = Vec3::new(-0.15, -0.15, 0.0); // rest 左 shoulder offset
        let shoulder_world =
            Vec3::new(0.0, 1.7, 0.0) + head_rot * (flip * shoulder_offset);
        // wrist を肩の前方 0.4 (到達可能) に配置
        let arm_forward = head_rot * Vec3::NEG_Z;
        let wrist_target = shoulder_world + arm_forward * 0.4;
        set_targets(&mut app, skeleton.root, |t| {
            t.head = Some(VrIkPose {
                translation: Vec3::new(0.0, 1.7, 0.0),
                rotation: head_rot,
            });
            t.left_hand = Some(VrIkPose {
                translation: wrist_target,
                rotation: Quat::IDENTITY,
            });
        });
        app.update();

        // 復元: corrected_parent = hip_rot * MODEL_FLIP * parent_rest_world
        // fixture は rest 全 identity なので corrected_parent = hip_rot * flip
        let hip_rot = head_rot; // estimate_hip: yaw=head_yaw
        let corrected_parent = hip_rot * flip;
        let upper_local = bone_rotation_of(&app, &skeleton, bone_names::LEFT_UPPER_ARM);
        let upper_world = corrected_parent * upper_local;
        let bone_axis = Vec3::NEG_X; // 左腕
        let upper_dir = upper_world * bone_axis;
        let elbow = shoulder_world + upper_dir * 0.3;
        assert!(
            ((elbow - shoulder_world).length() - 0.3).abs() < 0.02,
            "shoulder-elbow distance with yaw: elbow={elbow:?}"
        );
        assert!(
            ((elbow - wrist_target).length() - 0.3).abs() < 0.02,
            "elbow-wrist distance with yaw: elbow={elbow:?}, wrist={wrist_target:?}"
        );
    }

    #[test]
    fn hand_aligned_to_controller_forward() {
        let mut app = test_ik_app();
        let skeleton = spawn_skeleton(app.world_mut(), SkeletonOptions::default());
        let controller_rotation = Quat::from_rotation_y(0.4);
        set_targets(&mut app, skeleton.root, |t| {
            t.head = Some(head_pose_upright());
            t.left_hand = Some(VrIkPose {
                translation: Vec3::new(0.15, 1.55, -0.4),
                rotation: controller_rotation,
            });
        });
        app.update();

        // hand world 回転 = upper_world × lower_local × hand_local
        let flip = Quat::from_rotation_y(std::f32::consts::PI);
        let hand_world = flip
            * composed_world_rotation(
                &app,
                &skeleton,
                &[
                    bone_names::LEFT_UPPER_ARM,
                    bone_names::LEFT_LOWER_ARM,
                    bone_names::LEFT_HAND,
                ],
            );
        // VRM rest finger axis (-X) がコントローラ前方 (-Z) に揃う
        let finger_dir = hand_world * Vec3::NEG_X;
        let expected = controller_rotation * Vec3::NEG_Z;
        assert!(
            (finger_dir - expected).length() < 0.01,
            "finger axis should align to controller forward: got {finger_dir:?}, expected {expected:?}"
        );
    }

    /// 脚 IK の足首位置を書かれた local 回転から再構成する。
    /// `upper_leg_offset` は rest の hips 基準オフセット (flip 前の生値)。
    fn reconstruct_ankle(
        app: &App,
        skeleton: &Skeleton,
        hip_pos: Vec3,
        upper_leg_offset: Vec3,
        side: &str,
    ) -> Vec3 {
        let flip = Quat::from_rotation_y(std::f32::consts::PI);
        let (upper_name, lower_name) = if side == "left" {
            (bone_names::LEFT_UPPER_LEG, bone_names::LEFT_LOWER_LEG)
        } else {
            (bone_names::RIGHT_UPPER_LEG, bone_names::RIGHT_LOWER_LEG)
        };
        // hips world rot = hip_rot(identity yaw) * flip
        let hips_world_rot = flip;
        let upper_world = hips_world_rot * bone_rotation_of(app, skeleton, upper_name);
        let lower_world = upper_world * bone_rotation_of(app, skeleton, lower_name);
        let bone_axis = Vec3::NEG_Y; // lower_leg rest local translation 方向
        let joint = hip_pos + hips_world_rot * upper_leg_offset;
        let knee = joint + (upper_world * bone_axis) * 0.45;
        knee + (lower_world * bone_axis) * 0.5
    }

    #[test]
    fn leg_ik_reaches_floor_and_foot_untouched() {
        let mut app = test_ik_app();
        let skeleton = spawn_skeleton(app.world_mut(), SkeletonOptions::default());
        set_targets(&mut app, skeleton.root, |t| {
            t.head = Some(head_pose_upright());
        });
        app.update();

        // デフォルト foot_step (全ゼロ): foot target = rest XZ オフセット (flip 済み)・y=0
        // 左足: hip(0,1,0) + flip*foot_offset(-0.1,-1,0) → target (0.1, 0, 0)
        let ankle = reconstruct_ankle(
            &app,
            &skeleton,
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(-0.1, -0.05, 0.0), // rest upper_leg_offset (左、flip 前)
            "left",
        );
        let expected = Vec3::new(0.1, 0.0, 0.0);
        assert!(
            (ankle - expected).length() < 0.02,
            "left ankle should reach floor target: got {ankle:?}, expected {expected:?}"
        );
        // foot 骨は書かれない (lower_leg 追従、POC 品質)
        let foot_rot = bone_rotation_of(&app, &skeleton, bone_names::LEFT_FOOT);
        assert!(
            foot_rot.angle_between(Quat::IDENTITY) < 1e-6,
            "foot bone should not be written"
        );
    }

    #[test]
    fn foot_step_offsets_shift_targets() {
        let mut app = test_ik_app();
        let skeleton = spawn_skeleton(app.world_mut(), SkeletonOptions::default());
        set_targets(&mut app, skeleton.root, |t| {
            t.head = Some(head_pose_upright());
            t.foot_step = VrIkFootStep {
                // Y 成分は無視される (0.5 を入れても target に影響しない)
                left_offset_xz: Vec3::new(0.0, 0.5, -0.2),
                left_height: 0.1,
                right_offset_xz: Vec3::ZERO,
                right_height: 0.0,
            };
        });
        app.update();

        let ankle = reconstruct_ankle(
            &app,
            &skeleton,
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(-0.1, -0.05, 0.0),
            "left",
        );
        // target = rest 由来 (0.1, 0, 0) + step (0, 0, -0.2)、y = height 0.1
        let expected = Vec3::new(0.1, 0.1, -0.2);
        assert!(
            (ankle - expected).length() < 0.02,
            "left ankle should reach shifted target: got {ankle:?}, expected {expected:?}"
        );
    }

    #[test]
    fn legs_skipped_when_no_leg_cache() {
        let mut app = test_ik_app();
        let skeleton = spawn_skeleton(
            app.world_mut(),
            SkeletonOptions {
                with_legs: false,
                ..Default::default()
            },
        );
        set_targets(&mut app, skeleton.root, |t| {
            t.head = Some(head_pose_upright());
            t.left_hand = Some(VrIkPose {
                translation: Vec3::new(0.15, 1.55, -0.4),
                rotation: Quat::IDENTITY,
            });
        });
        app.update();
        // 腕・hips は動く
        let hips = app
            .world()
            .get::<Transform>(skeleton.bones[bone_names::HIPS])
            .unwrap();
        assert!((hips.translation.y - 1.0).abs() < 0.01);
        let upper_arm = bone_rotation_of(&app, &skeleton, bone_names::LEFT_UPPER_ARM);
        assert!(upper_arm.angle_between(Quat::IDENTITY) > 0.01);
    }

    #[test]
    fn despawned_bone_is_skipped_without_panic() {
        let mut app = test_ik_app();
        let skeleton = spawn_skeleton(app.world_mut(), SkeletonOptions::default());
        set_targets(&mut app, skeleton.root, |t| {
            t.head = Some(head_pose_upright());
            t.left_hand = Some(VrIkPose {
                translation: Vec3::new(0.15, 1.55, -0.4),
                rotation: Quat::IDENTITY,
            });
            t.right_hand = Some(VrIkPose {
                translation: Vec3::new(-0.15, 1.55, -0.4),
                rotation: Quat::IDENTITY,
            });
        });
        app.update(); // cache 構築 + 初回適用
        app.world_mut()
            .entity_mut(skeleton.bones[bone_names::LEFT_LOWER_ARM])
            .despawn();
        // 反対腕を rest に戻して再適用を観測
        app.world_mut()
            .get_mut::<Transform>(skeleton.bones[bone_names::RIGHT_UPPER_ARM])
            .unwrap()
            .rotation = Quat::IDENTITY;
        app.update(); // panic せず、他の骨は書かれる
        let right = bone_rotation_of(&app, &skeleton, bone_names::RIGHT_UPPER_ARM);
        assert!(
            right.angle_between(Quat::IDENTITY) > 0.01,
            "other bones should still be written"
        );
    }

    #[test]
    fn no_write_after_vr_ik_removed() {
        let mut app = test_ik_app();
        let skeleton = spawn_skeleton(app.world_mut(), SkeletonOptions::default());
        set_targets(&mut app, skeleton.root, |t| {
            t.head = Some(head_pose_upright());
        });
        app.update();
        app.world_mut().entity_mut(skeleton.root).remove::<VrIk>();
        // hips を rest に戻す → IK が動いていれば再度書かれるはず
        let hips = skeleton.bones[bone_names::HIPS];
        {
            let mut tf = app.world_mut().get_mut::<Transform>(hips).unwrap();
            tf.translation = Vec3::new(0.0, 1.0, 0.0);
            tf.rotation = Quat::IDENTITY;
        }
        app.update();
        let rot = bone_rotation_of(&app, &skeleton, bone_names::HIPS);
        assert!(
            rot.angle_between(Quat::IDENTITY) < 1e-6,
            "no bone should be written after VrIk removal"
        );
    }

    // === 複数 VRM ===

    #[test]
    fn multiple_vrms_solved_independently() {
        let mut app = test_ik_app();
        let a = spawn_skeleton(app.world_mut(), SkeletonOptions::default());
        let b = spawn_skeleton(app.world_mut(), SkeletonOptions::default());
        set_targets(&mut app, a.root, |t| {
            t.head = Some(VrIkPose {
                translation: Vec3::new(1.0, 1.7, 0.0),
                rotation: Quat::IDENTITY,
            });
        });
        set_targets(&mut app, b.root, |t| {
            t.head = Some(VrIkPose {
                translation: Vec3::new(-2.0, 1.7, 0.0),
                rotation: Quat::IDENTITY,
            });
        });
        app.update();
        let hips_a = app
            .world()
            .get::<Transform>(a.bones[bone_names::HIPS])
            .unwrap()
            .translation;
        let hips_b = app
            .world()
            .get::<Transform>(b.bones[bone_names::HIPS])
            .unwrap()
            .translation;
        assert!((hips_a.x - 1.0).abs() < 0.01, "hips_a: {hips_a:?}");
        assert!((hips_b.x - (-2.0)).abs() < 0.01, "hips_b: {hips_b:?}");
    }
}
