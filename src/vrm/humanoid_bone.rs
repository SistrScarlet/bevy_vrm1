//! This module handles humanoid bones.
//! Refer to [here](https://docs.unity3d.com/ja/2019.4/ScriptReference/HumanBodyBones.html) for the list of humanoid bones.
//!
//! After the VRM(A) is loaded, marker components are inserted for each bone.
//! For example, the entity of the hips bone will have [`Hips`] inserted.
//! Additionally, a component that holds the entity will be inserted into the VRM(A) entity.
//!
//! The setup of these is done after all bones have been spawned, so there may be a slight delay.

mod bones;
pub mod capsule_fit;

/// `VRMC_vrm::humanoid` の骨名定数 (camelCase、VRM 1.0 仕様の全 humanoid 骨)。
///
/// [`HumanoidBoneEntities::find`] や [`VrmBone`] のキーとして bare literal を書くと
/// typo がコンパイルを通り、silent に機能不発となる。必ずここを経由すること。
pub mod bone_names {
    pub const HIPS: &str = "hips";
    pub const SPINE: &str = "spine";
    pub const CHEST: &str = "chest";
    pub const UPPER_CHEST: &str = "upperChest";
    pub const NECK: &str = "neck";
    pub const HEAD: &str = "head";
    pub const LEFT_EYE: &str = "leftEye";
    pub const RIGHT_EYE: &str = "rightEye";
    pub const JAW: &str = "jaw";
    pub const LEFT_SHOULDER: &str = "leftShoulder";
    pub const LEFT_UPPER_ARM: &str = "leftUpperArm";
    pub const LEFT_LOWER_ARM: &str = "leftLowerArm";
    pub const LEFT_HAND: &str = "leftHand";
    pub const RIGHT_SHOULDER: &str = "rightShoulder";
    pub const RIGHT_UPPER_ARM: &str = "rightUpperArm";
    pub const RIGHT_LOWER_ARM: &str = "rightLowerArm";
    pub const RIGHT_HAND: &str = "rightHand";
    pub const LEFT_UPPER_LEG: &str = "leftUpperLeg";
    pub const LEFT_LOWER_LEG: &str = "leftLowerLeg";
    pub const LEFT_FOOT: &str = "leftFoot";
    pub const LEFT_TOES: &str = "leftToes";
    pub const RIGHT_UPPER_LEG: &str = "rightUpperLeg";
    pub const RIGHT_LOWER_LEG: &str = "rightLowerLeg";
    pub const RIGHT_FOOT: &str = "rightFoot";
    pub const RIGHT_TOES: &str = "rightToes";
    pub const LEFT_THUMB_METACARPAL: &str = "leftThumbMetacarpal";
    pub const LEFT_THUMB_PROXIMAL: &str = "leftThumbProximal";
    pub const LEFT_THUMB_DISTAL: &str = "leftThumbDistal";
    pub const LEFT_INDEX_PROXIMAL: &str = "leftIndexProximal";
    pub const LEFT_INDEX_INTERMEDIATE: &str = "leftIndexIntermediate";
    pub const LEFT_INDEX_DISTAL: &str = "leftIndexDistal";
    pub const LEFT_MIDDLE_PROXIMAL: &str = "leftMiddleProximal";
    pub const LEFT_MIDDLE_INTERMEDIATE: &str = "leftMiddleIntermediate";
    pub const LEFT_MIDDLE_DISTAL: &str = "leftMiddleDistal";
    pub const LEFT_RING_PROXIMAL: &str = "leftRingProximal";
    pub const LEFT_RING_INTERMEDIATE: &str = "leftRingIntermediate";
    pub const LEFT_RING_DISTAL: &str = "leftRingDistal";
    pub const LEFT_LITTLE_PROXIMAL: &str = "leftLittleProximal";
    pub const LEFT_LITTLE_INTERMEDIATE: &str = "leftLittleIntermediate";
    pub const LEFT_LITTLE_DISTAL: &str = "leftLittleDistal";
    pub const RIGHT_THUMB_METACARPAL: &str = "rightThumbMetacarpal";
    pub const RIGHT_THUMB_PROXIMAL: &str = "rightThumbProximal";
    pub const RIGHT_THUMB_DISTAL: &str = "rightThumbDistal";
    pub const RIGHT_INDEX_PROXIMAL: &str = "rightIndexProximal";
    pub const RIGHT_INDEX_INTERMEDIATE: &str = "rightIndexIntermediate";
    pub const RIGHT_INDEX_DISTAL: &str = "rightIndexDistal";
    pub const RIGHT_MIDDLE_PROXIMAL: &str = "rightMiddleProximal";
    pub const RIGHT_MIDDLE_INTERMEDIATE: &str = "rightMiddleIntermediate";
    pub const RIGHT_MIDDLE_DISTAL: &str = "rightMiddleDistal";
    pub const RIGHT_RING_PROXIMAL: &str = "rightRingProximal";
    pub const RIGHT_RING_INTERMEDIATE: &str = "rightRingIntermediate";
    pub const RIGHT_RING_DISTAL: &str = "rightRingDistal";
    pub const RIGHT_LITTLE_PROXIMAL: &str = "rightLittleProximal";
    pub const RIGHT_LITTLE_INTERMEDIATE: &str = "rightLittleIntermediate";
    pub const RIGHT_LITTLE_DISTAL: &str = "rightLittleDistal";
}

use crate::error::vrm_warn;
use crate::prelude::*;
use crate::vrm::gltf::extensions::VrmNode;
use crate::vrm::humanoid_bone::bones::BonesPlugin;
use crate::vrm::{RestGlobalTransform, RestTransform, VrmBone};
use crate::vrma::RetargetSource;
use bevy::animation::{AnimatedBy, AnimationTargetId};
use bevy::app::{App, Plugin};
use bevy::asset::{Assets, Handle};
use bevy::gltf::GltfNode;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

pub mod prelude {
    pub use crate::vrm::humanoid_bone::{
        HumanoidBoneEntities, bone_names,
        bones::*,
        capsule_fit::{
            HumanoidBonePositions, HumanoidCapsule, HumanoidCapsuleKind, HumanoidCapsuleRatios,
            fit_humanoid_capsules,
        },
    };
}

#[derive(EntityEvent)]
pub(crate) struct RequestInitializeHumanoidBones(pub(crate) Entity);

#[derive(Component, Deref, Reflect, Default)]
pub(crate) struct HumanoidBoneRegistry(HashMap<VrmBone, Name>);

/// VRM(A) ルート entity に挿入される、humanoid bone 名から bone entity への一括マップ。
///
/// 個別の `*BoneEntity` コンポーネント ([`HipsBoneEntity`](bones::HipsBoneEntity) 等) と
/// 同時に構築される。多数の骨をまとめて引く場合はこちらを使うと query が簡潔になる。
/// キーは `VRMC_vrm::humanoid` の bone 名 (`"hips"`, `"leftUpperArm"` 等)。
#[derive(Component, Debug, Deref, Reflect, Default)]
#[reflect(Component)]
pub struct HumanoidBoneEntities(pub HashMap<VrmBone, Entity>);

impl HumanoidBoneEntities {
    /// bone 名から bone entity を引く。
    pub fn find(
        &self,
        bone: &str,
    ) -> Option<Entity> {
        self.0.get(bone).copied()
    }
}

impl HumanoidBoneRegistry {
    pub fn new(
        bones: &HashMap<String, VrmNode>,
        node_assets: &Assets<GltfNode>,
        nodes: &[Handle<GltfNode>],
    ) -> Self {
        Self(
            bones
                .iter()
                .filter_map(|(name, target_node)| {
                    let node_handle = nodes.get(target_node.node)?;
                    let node = node_assets.get(node_handle)?;
                    Some((VrmBone(name.clone()), Name::new(node.name.clone())))
                })
                .collect(),
        )
    }
}

pub struct VrmHumanoidBonePlugin;

impl Plugin for VrmHumanoidBonePlugin {
    fn build(
        &self,
        app: &mut App,
    ) {
        app.register_type::<HumanoidBoneRegistry>()
            .register_type::<HumanoidBoneEntities>()
            .add_plugins(BonesPlugin)
            .add_observer(apply_insert_rest_transforms)
            .add_observer(apply_initialize_humanoid_bones);
    }
}

macro_rules! insert_bone {
    (
        $commands: expr,
        $vrm_entity: expr,
        $bone_entity: expr,
        $bone_name: expr,
        $($bone: ident),+$(,)?
    ) => {

        match $bone_name.0.to_uppercase(){
            $(
                x if x == stringify!($bone).to_uppercase() => {
                    paste::paste!{
                        $commands.entity($vrm_entity).insert([<$bone BoneEntity>]($bone_entity));
                    }
                    $commands.entity($bone_entity).insert($bone);
                }
            )+
            _ => {

            }
        }
    };
}

fn apply_insert_rest_transforms(
    trigger: On<RequestInitializeHumanoidBones>,
    mut commands: Commands,
    childrens: Query<&Children>,
    transforms: Query<(&Transform, &GlobalTransform)>,
) {
    let vrm = trigger.event_target();
    insert_rest_transforms_recursive(&mut commands, vrm, &childrens, &transforms);
}

fn insert_rest_transforms_recursive(
    commands: &mut Commands,
    entity: Entity,
    childrens: &Query<&Children>,
    transforms: &Query<(&Transform, &GlobalTransform)>,
) {
    let Ok(children) = childrens.get(entity) else {
        return;
    };
    for child in children {
        let Ok((tf, gtf)) = transforms.get(entity) else {
            continue;
        };
        commands
            .entity(entity)
            .insert((RestTransform(*tf), RestGlobalTransform(*gtf)));
        insert_rest_transforms_recursive(commands, *child, childrens, transforms);
    }
}

fn apply_initialize_humanoid_bones(
    trigger: On<RequestInitializeHumanoidBones>,
    mut commands: Commands,
    searcher: ChildSearcher,
    models: Query<&HumanoidBoneRegistry>,
    parents: Query<&ChildOf>,
    transforms: Query<(&Transform, &GlobalTransform)>,
    has_vrm: Query<Has<Vrm>>,
) {
    let model_entity = trigger.event_target();
    let Ok(registry) = models.get(model_entity) else {
        vrm_warn!("[HumanoidBone] HumanoidBoneRegistry not found on {model_entity}");
        return;
    };
    // hips は VRM 仕様上 required だが、不正な VRM(A) や node 解決に失敗した registry
    // では欠け得るため、panic せずスキップする。
    let Some(hips_name) = registry.get(&VrmBone::from("hips")) else {
        vrm_warn!(
            "[HumanoidBone] 'hips' not found in humanoid bones of {model_entity}; skipping initialization"
        );
        return;
    };
    let Some(hips) = searcher.find_from_name(model_entity, hips_name) else {
        vrm_warn!(
            "[HumanoidBone] hips bone entity '{hips_name}' not found under {model_entity}; skipping initialization"
        );
        return;
    };
    let Ok(ChildOf(root_bone)) = parents.get(hips) else {
        vrm_warn!(
            "[HumanoidBone] hips bone of {model_entity} has no parent; skipping initialization"
        );
        return;
    };
    let has_vrm = has_vrm.get(model_entity).is_ok_and(|h| h);
    commands
        .entity(*root_bone)
        .insert((AnimationPlayer::default(), AnimationTransitions::default()));
    if has_vrm {
        commands.entity(*root_bone).insert((
            Name::new(Vrm::ROOT_BONE),
            RetargetSource,
            AnimationTargetId::from_name(&Name::new(Vrm::ROOT_BONE)),
            AnimatedBy(*root_bone),
        ));
    }

    let mut bone_entities = HumanoidBoneEntities::default();
    for (bone, name) in registry.iter() {
        // 解決できない骨は HumanoidBoneEntities からも欠けるため、無警告だと利用側
        // (capsule_fit 等) が原因不明のまま機能しなくなる。必ずログを残す。
        let Some(bone_entity) = searcher.find_from_name(model_entity, name.as_str()) else {
            vrm_warn!("[HumanoidBone] bone entity '{name}' ({bone}) not found under {model_entity}");
            continue;
        };
        let Ok((tf, gtf)) = transforms.get(bone_entity) else {
            vrm_warn!("[HumanoidBone] bone entity '{name}' ({bone}) has no Transform; skipped");
            continue;
        };
        bone_entities.0.insert(bone.clone(), bone_entity);
        commands.entity(bone_entity).insert((
            bone.clone(),
            RestTransform(*tf),
            RestGlobalTransform(*gtf),
            RetargetSource,
        ));
        if has_vrm {
            commands
                .entity(bone_entity)
                .insert((AnimationTargetId::from_name(name), AnimatedBy(*root_bone)));
        }
        insert_bone!(
            commands,
            model_entity,
            bone_entity,
            bone,
            Hips,
            RightRingProximal,
            RightThumbDistal,
            RightRingIntermediate,
            RightUpperArm,
            LeftIndexProximal,
            LeftUpperLeg,
            LeftFoot,
            LeftIndexDistal,
            LeftThumbMetacarpal,
            RightLowerArm,
            LeftMiddleDistal,
            RightUpperLeg,
            LeftToes,
            LeftThumbDistal,
            RightShoulder,
            RightThumbMetacarpal,
            Spine,
            LeftLowerLeg,
            LeftShoulder,
            LeftUpperArm,
            UpperChest,
            RightToes,
            RightIndexDistal,
            LeftMiddleProximal,
            LeftRingProximal,
            LeftRingDistal,
            LeftThumbProximal,
            LeftIndexIntermediate,
            LeftLittleProximal,
            LeftLittleDistal,
            RightHand,
            RightLittleProximal,
            LeftRingIntermediate,
            RightIndexIntermediate,
            Chest,
            LeftHand,
            RightLittleIntermediate,
            RightFoot,
            RightLowerLeg,
            LeftLittleIntermediate,
            LeftLowerArm,
            RightLittleDistal,
            RightMiddleIntermediate,
            RightMiddleProximal,
            RightThumbProximal,
            Neck,
            Jaw,
            Head,
            LeftEye,
            RightEye,
            LeftMiddleIntermediate,
            RightRingDistal,
            RightIndexProximal,
            RightMiddleDistal,
        );
    }
    commands.entity(model_entity).insert(bone_entities);
}
