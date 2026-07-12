pub mod body_tracking;
pub mod bone_overlay;
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
pub mod vr_ik;

use crate::macros::marker_component;
use crate::new_type;
use crate::system_set::VrmSystemSets;
use crate::vrm::body_tracking::BodyTrackingPlugin;
use crate::vrm::bone_overlay::{BoneOverlayPlugin, BoneOverlaySystems};
use crate::vrm::detach::VrmDetachPlugin;
use crate::vrm::humanoid_bone::VrmHumanoidBonePlugin;
use crate::vrm::initialize::VrmInitializePlugin;
use crate::vrm::loader::{VrmAsset, VrmLoaderPlugin};
use crate::vrm::look_at::LookAtPlugin;
use crate::vrm::node_constraint::VrmNodeConstraintPlugin;
use crate::vrm::spring_bone::VrmSpringBonePlugin;
use crate::vrm::vr_ik::{VrIkPlugin, VrIkSystems};
use bevy::app::{AnimationSystems, App, Plugin};
use bevy::asset::AssetApp;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy::transform::systems::{propagate_parent_transforms, sync_simple_transforms};
use expressions::VrmExpressionPlugin;
use mtoon::MtoonMaterialPlugin;
use std::path::PathBuf;

pub mod prelude {
    pub use crate::vrm::{
        Initialized, RestGlobalTransform, RestTransform, Vrm, VrmBone, VrmCorePlugin,
        VrmExpression, VrmPath, VrmPlugin,
        body_tracking::{BodyTracking, SmoothedGaze},
        bone_overlay::{BoneOverlaySystems, BoneRotationOverlay},
        detach::RequestDetachVrm,
        expressions::{
            BinaryExpression, ClearExpressions, ExpressionCategory, ExpressionEntityMap,
            ExpressionOverride, ExpressionOverrideSettings, ExpressionOverrideType,
            ModifyExpressions, SetExpressions,
        },
        gltf::prelude::*,
        humanoid_bone::prelude::*,
        loader::{VrmAsset, VrmHandle},
        look_at::LookAt,
        mtoon::prelude::*,
        spring_bone::{SpringJointProps, SpringJoints, SpringRoot},
        vr_ik::{
            VrIk, VrIkChainCache, VrIkFootStep, VrIkLegChainCache, VrIkPose, VrIkSystems,
            VrIkTargets,
            calibration::{VrIkRestPositions, build_vr_ik_chain_cache},
            solver::{distribute_spine, estimate_hip, two_bone_ik},
        },
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
        app.add_plugins((VrmCorePlugin, MtoonMaterialPlugin));
    }
}

/// `MToon` レンダリング ([`MaterialPlugin`] 経由で wgpu `RenderApp` を要求する) を除いた
/// VRM コア構成。ロード・初期化・SpringBone・Expression・LookAt・NodeConstraint 等、
/// 描画バックエンドに依存しない全機能を含む。
///
/// 独自レンダラを使うなど wgpu `RenderPlugin` を持たないアプリはこちらを使い、
/// `MToon` 描画は自前のパイプラインで行う ([`VrmcMaterialRegistry`] から `MToon` 拡張
/// 情報を読める)。通常のアプリは [`VrmPlugin`] を使うこと。
pub struct VrmCorePlugin;

impl Plugin for VrmCorePlugin {
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
            LookAtPlugin,
            BodyTrackingPlugin,
            BoneOverlayPlugin,
            VrIkPlugin,
        ));

        // VRM spec 更新順 (https://vrm.dev/api/api_update/) を set chain で保証する。
        // 空になり得る set への .after() は Bevy ではエッジを生まないため、この chain が
        // set 間順序の唯一の担保 (個々のシステムの .after/.before は意図の明示用)。
        // chain 全体を Bevy 標準 propagation (TransformSystems::Propagate) の前に置くことで、
        // chain 内の transform 書込 (GazeControl / BoneOverlay / SpringBone) が同フレームの
        // 描画ポーズへ確実に反映されることを保証する (この edge がないと相対順序は
        // schedule build 依存になる)。
        app.configure_sets(
            PostUpdate,
            (
                VrIkSystems,
                VrmSystemSets::Constraints,
                VrmSystemSets::PropagateAfterConstraints,
                VrmSystemSets::GazeControl,
                BoneOverlaySystems,
                VrmSystemSets::Expressions,
                VrmSystemSets::PropagateAfterExpressions,
                VrmSystemSets::SpringBone,
                VrmSystemSets::DetermineRedraw,
            )
                .chain()
                .after(AnimationSystems)
                .before(TransformSystems::Propagate),
        );

        // Add manual transform propagation systems to follow VRM spec update order
        // See: https://vrm.dev/api/api_update/
        app.add_systems(
            PostUpdate,
            (sync_simple_transforms, propagate_parent_transforms)
                .chain()
                .in_set(VrmSystemSets::PropagateAfterConstraints),
        );
        // fork(bevy_ash_xr): VRM spec 更新順の 2 回目の全 scene propagation
        // (PropagateAfterExpressions) は無条件には登録しない (N=50 VRM で ~1.1ms/frame の削減)。
        // 品質 tradeoff として以下を許容する:
        // - Expressions は MorphWeights のみ書き transform を変えないため影響なし。
        // - GazeControl が書く transform (LookAt の目ボーン local、および BodyTracking の
        //   頭部チェーン回転) は chain 直後の Bevy 標準 PostUpdate propagation が拾うため
        //   描画には同フレームで反映されるが、頭部チェーン配下の collider・joint anchor の
        //   GlobalTransform は SpringBone 実行時点では今フレームの回転を反映せず、
        //   SpringBone の物理入力は最大 1 frame 遅延する。
        // 例外: BoneRotationOverlay がアクティブなフレームだけは BoneOverlayPlugin が
        // PropagateAfterExpressions に条件付き propagation を登録しており (bone_overlay.rs)、
        // SpringBone は同フレームでオーバーレイ適用後の GlobalTransform を読む。

        app.register_type::<Vrm>()
            .register_type::<VrmPath>()
            .register_type::<RestTransform>()
            .register_type::<RestGlobalTransform>()
            .register_type::<VrmBone>()
            .register_type::<VrmExpression>()
            .register_type::<Initialized>();

        // MToon 抜き構成では mesh entity が MeshMaterial3d<StandardMaterial> のまま
        // 保持されるが、generics 型は reflect の自動登録対象外のため、未登録だと
        // scene spawn 時に panic する。MToon 系コンポーネント型の登録も MaterialPlugin
        // とは独立にここで行う (VrmcMaterialRegistry は MToon 構成に関わらず挿入される)。
        // 登録一覧の実体は mtoon.rs 側の 1 箇所のみ (リストの二重管理による drift 防止)。
        app.register_type::<MeshMaterial3d<StandardMaterial>>();
        mtoon::register_mtoon_reflect_types(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::AssetPlugin;
    use bevy::gltf::GltfNode;
    use bevy::scene::ScenePlugin;

    /// `VrmCorePlugin` の set chain (`VrIkSystems` / `BoneOverlaySystems` 込み) と
    /// `.after(AnimationSystems)` / `.before(TransformSystems::Propagate)` の
    /// 順序エッジが cycle なく schedule を構築・実行できることの smoke test。
    #[test]
    fn vrm_core_plugin_schedule_builds_without_cycles() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            TransformPlugin,
            ScenePlugin,
        ));
        app.init_asset::<GltfNode>().init_asset::<AnimationClip>();
        app.add_plugins(VrmCorePlugin);
        app.update();
        app.update();
    }

    /// `VrIkSystems` が `AnimationSystems` の後・`VrmSystemSets::Constraints` の前に
    /// 走ることの検証 (chain 組込みエッジ。IK がアニメーション出力を上書きし、
    /// node constraint が IK 結果を同フレームで読める位置にあること)。
    #[test]
    fn vr_ik_runs_between_animation_and_constraints() {
        #[derive(Resource, Default)]
        struct Order(Vec<&'static str>);

        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            TransformPlugin,
            ScenePlugin,
        ));
        app.init_asset::<GltfNode>().init_asset::<AnimationClip>();
        app.add_plugins(VrmCorePlugin);
        app.init_resource::<Order>();
        app.add_systems(
            PostUpdate,
            (
                (|mut o: ResMut<Order>| o.0.push("animation")).in_set(AnimationSystems),
                (|mut o: ResMut<Order>| o.0.push("vr_ik")).in_set(VrIkSystems),
                (|mut o: ResMut<Order>| o.0.push("constraints"))
                    .in_set(VrmSystemSets::Constraints),
            ),
        );
        app.update();
        let order = &app.world().resource::<Order>().0;
        assert_eq!(order, &["animation", "vr_ik", "constraints"]);
    }
}
