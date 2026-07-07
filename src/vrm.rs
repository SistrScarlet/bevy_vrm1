pub mod body_tracking;
pub mod detach;
pub mod expressions;
pub(crate) mod gltf;
pub mod humanoid_bone;
pub mod initialize;
pub mod loader;
pub mod look_at;
mod mtoon;
pub mod node_constraint;
pub mod spring_bone;

use crate::macros::marker_component;
use crate::new_type;
use crate::system_set::VrmSystemSets;
use crate::vrm::body_tracking::BodyTrackingPlugin;
use crate::vrm::detach::VrmDetachPlugin;
use crate::vrm::humanoid_bone::VrmHumanoidBonePlugin;
use crate::vrm::initialize::VrmInitializePlugin;
use crate::vrm::loader::{VrmAsset, VrmLoaderPlugin};
use crate::vrm::look_at::LookAtPlugin;
use crate::vrm::node_constraint::VrmNodeConstraintPlugin;
use crate::vrm::spring_bone::VrmSpringBonePlugin;
use bevy::app::{AnimationSystems, App, Plugin};
use bevy::asset::AssetApp;
use bevy::prelude::*;
use bevy::transform::systems::{propagate_parent_transforms, sync_simple_transforms};
use expressions::VrmExpressionPlugin;
use mtoon::MtoonMaterialPlugin;
use std::path::PathBuf;

pub mod prelude {
    pub use crate::vrm::{
        Initialized, RestGlobalTransform, RestTransform, Vrm, VrmBone, VrmExpression, VrmPath,
        VrmPlugin,
        body_tracking::{BodyTracking, SmoothedGaze},
        detach::RequestDetachVrm,
        expressions::{
            BinaryExpression, ClearExpressions, ExpressionEntityMap, ExpressionOverride,
            ExpressionOverrideSettings, ExpressionOverrideType, ModifyExpressions, SetExpressions,
        },
        gltf::prelude::*,
        humanoid_bone::prelude::*,
        loader::{VrmAsset, VrmHandle},
        look_at::LookAt,
        mtoon::prelude::*,
        spring_bone::{SpringJointProps, SpringJoints, SpringRoot},
    };
}

new_type!(
    /// The bone name obtained from `VRMC_vrm::humanoid`.
    name: VrmBone,
    ty: String,
);

new_type!(
    /// The key name of `VRMC_vrm::expressions::preset`.
    name: VrmExpression,
    ty: String,
);

/// A marker component attached to the entity of VRM.
/// This component is automatically inserted after the [`VrmHandle`](crate::prelude::VrmHandle) is loaded.
#[derive(Debug, Component, Reflect, Copy, Clone)]
#[reflect(Component)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", reflect(Serialize, Deserialize))]
pub struct Vrm;

impl Vrm {
    pub const EXPRESSIONS_ROOT: &'static str = "VRMC_vrm.expressions";
    pub const ROOT_BONE: &'static str = "VRMC_vrm.root_bone";
}

/// The path to the VRM file.
/// This component is automatically inserted after the [`VrmHandle`](crate::prelude::VrmHandle) is loaded.
#[derive(Debug, Reflect, Clone, Component)]
#[reflect(Component)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", reflect(Serialize, Deserialize))]
pub struct VrmPath(pub PathBuf);

impl VrmPath {
    /// Creates a new [`VrmPath`] from the path.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }
}

/// The bone's initial transform.
#[derive(Debug, Copy, Clone, Component, Deref, Reflect, Default)]
#[reflect(Component)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", reflect(Serialize, Deserialize))]
pub struct RestTransform(pub Transform);

/// The bone's initial global transform.
#[derive(Debug, Copy, Clone, Component, Deref, Reflect, Default)]
#[reflect(Component)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", reflect(Serialize, Deserialize))]
pub struct RestGlobalTransform(pub GlobalTransform);

marker_component!(
    /// A marker component attached to the entity of VRM.
    /// This component is automatically inserted after the [`VrmHandle`](crate::prelude::VrmHandle) is loaded.
    Initialized
);
/// The main plugin for VRM support in Bevy.
///
/// Please refer to [`VrmHandle`](crate::prelude::VrmHandle) for more details.
pub struct VrmPlugin;

impl Plugin for VrmPlugin {
    fn build(
        &self,
        app: &mut App,
    ) {
        app.init_asset::<VrmAsset>().add_plugins((
            VrmLoaderPlugin,
            VrmInitializePlugin,
            VrmDetachPlugin,
            VrmSpringBonePlugin,
            VrmHumanoidBonePlugin,
            VrmExpressionPlugin,
            VrmNodeConstraintPlugin,
            MtoonMaterialPlugin,
            LookAtPlugin,
            BodyTrackingPlugin,
        ));

        // Add manual transform propagation systems to follow VRM spec update order
        // See: https://vrm.dev/api/api_update/
        app.add_systems(
            PostUpdate,
            (sync_simple_transforms, propagate_parent_transforms)
                .chain()
                .in_set(VrmSystemSets::PropagateAfterConstraints)
                .after(VrmSystemSets::Constraints)
                .before(VrmSystemSets::GazeControl),
        );
        // fork(bevy_ash_xr SP5D): VRM spec 更新順の 2 回目の全 scene propagation
        // (PropagateAfterExpressions) は登録しない。Expressions は MorphWeights のみ書き
        // transform を変えないため、SpringBone が必要とする propagated globals は
        // PropagateAfterConstraints で担保済み。GazeControl が書く目ボーン local は
        // Bevy 標準 PostUpdate propagation が拾う (最大 1 frame 遅延の品質 tradeoff を許容)。
        // N=50 VRM で全 scene propagation 1 回 ~1.1ms/frame の削減。
        // VrmSystemSets::PropagateAfterExpressions 自体は ordering anchor として残置 (空 set)。

        app.register_type::<Vrm>()
            .register_type::<VrmPath>()
            .register_type::<RestTransform>()
            .register_type::<RestGlobalTransform>()
            .register_type::<VrmBone>()
            .register_type::<VrmExpression>()
            .register_type::<Initialized>();
    }
}
