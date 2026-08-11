//! `VRMC_vrm::firstPerson` (mesh annotations) の handling.
//!
//! VRM の各 mesh が一人称視点でどう表示されるべきか (`FirstPersonType`) を、
//! glTF node index → Bevy entity へ解決したうえで [`VrmFirstPerson`] component として
//! 該当 mesh entity へ付与する。実際に一人称視点でメッシュを隠す/表示するレンダリング
//! 側の消費 (render filter 等) はこのクレートの責務外。

use crate::error::vrm_warn;
use crate::prelude::ChildSearcher;
use crate::vrm::gltf::extensions::VrmExtensions;
use crate::vrm::gltf::extensions::vrmc_vrm::FirstPersonType;
use bevy::asset::{Assets, Handle};
use bevy::gltf::GltfNode;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

/// 一人称視点でのメッシュの表示方針を保持する component。
///
/// `VRMC_vrm::firstPerson::meshAnnotations` で解決された mesh entity へ挿入される。
#[derive(Component, Clone, Copy, Debug)]
pub struct VrmFirstPerson(pub FirstPersonType);

/// VRM(A) ルート entity に挿入される、mesh 名から [`FirstPersonType`] への一括マップ。
///
/// [`VrmExpressionRegistry`](crate::vrm::expressions::VrmExpressionRegistry) と同様、
/// glTF node index の解決は construction 時 ([`FirstPersonMeshRegistry::new`]) に
/// 完了させ、entity への解決は spawn 後の observer ([`apply_initialize_first_person`])
/// で行う。
#[derive(Component, Deref, Reflect, Default)]
pub(crate) struct FirstPersonMeshRegistry(pub(crate) HashMap<Name, FirstPersonType>);

impl FirstPersonMeshRegistry {
    pub fn new(
        extensions: &VrmExtensions,
        node_assets: &Assets<GltfNode>,
        nodes: &[Handle<GltfNode>],
    ) -> Self {
        let Some(first_person) = extensions.vrmc_vrm.first_person.as_ref() else {
            return Self(HashMap::default());
        };
        Self(
            first_person
                .mesh_annotations
                .iter()
                .filter_map(|annotation| {
                    let node_handle = nodes.get(annotation.node as usize)?;
                    let node = node_assets.get(node_handle)?;
                    Some((Name::new(node.name.clone()), annotation.r#type))
                })
                .collect(),
        )
    }
}

#[derive(EntityEvent)]
pub(crate) struct RequestInitializeFirstPerson(pub(crate) Entity);

fn apply_initialize_first_person(
    trigger: On<RequestInitializeFirstPerson>,
    mut commands: Commands,
    searcher: ChildSearcher,
    models: Query<&FirstPersonMeshRegistry>,
) {
    let model_entity = trigger.event_target();
    // registry は spawn_vrm で常に insert されるため、取得失敗は construction bug の
    // みで起こり得る事象であり warn は不要。
    let Ok(registry) = models.get(model_entity) else {
        return;
    };
    for (name, fp_type) in registry.iter() {
        let Some(mesh_entity) = searcher.find_from_name(model_entity, name.as_str()) else {
            vrm_warn!(
                "[FirstPerson] mesh entity '{name}' not found under {model_entity}"
            );
            continue;
        };
        if matches!(fp_type, FirstPersonType::FirstPersonOnly) {
            commands
                .entity(mesh_entity)
                .insert((VrmFirstPerson(*fp_type), Visibility::Hidden));
        } else {
            commands.entity(mesh_entity).insert(VrmFirstPerson(*fp_type));
        }
    }
}

pub struct VrmFirstPersonPlugin;

impl Plugin for VrmFirstPersonPlugin {
    fn build(
        &self,
        app: &mut App,
    ) {
        app.register_type::<FirstPersonMeshRegistry>()
            .add_observer(apply_initialize_first_person);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vrm::gltf::extensions::vrmc_vrm::{
        FirstPerson, Humanoid, MeshAnnotation, VrmcVrm,
    };
    use bevy::platform::collections::HashMap as BevyHashMap;
    use bevy::transform::prelude::Transform;

    fn empty_vrmc_vrm(first_person: Option<FirstPerson>) -> VrmcVrm {
        VrmcVrm {
            expressions: None,
            first_person,
            humanoid: Humanoid {
                human_bones: BevyHashMap::default(),
            },
            look_at: None,
            meta: None,
            spec_version: "1.0".to_string(),
        }
    }

    fn test_gltf_node(
        index: usize,
        name: &str,
    ) -> GltfNode {
        GltfNode {
            index,
            name: name.to_string(),
            children: Vec::new(),
            mesh: None,
            skin: None,
            transform: Transform::IDENTITY,
            is_animation_root: false,
            extras: None,
        }
    }

    #[test]
    fn registry_construction_resolves_mesh_annotation() {
        let mut node_assets = Assets::<GltfNode>::default();
        let handle = node_assets.add(test_gltf_node(0, "SecondaryMesh"));
        let nodes = vec![handle];

        let extensions = VrmExtensions {
            vrmc_vrm: empty_vrmc_vrm(Some(FirstPerson {
                mesh_annotations: vec![MeshAnnotation {
                    node: 0,
                    r#type: FirstPersonType::ThirdPersonOnly,
                }],
            })),
            vrmc_spring_bone: None,
        };

        let registry = FirstPersonMeshRegistry::new(&extensions, &node_assets, &nodes);
        assert_eq!(registry.len(), 1);
        assert_eq!(
            registry.get(&Name::new("SecondaryMesh")),
            Some(&FirstPersonType::ThirdPersonOnly)
        );
    }

    #[test]
    fn registry_construction_without_first_person_is_empty() {
        let node_assets = Assets::<GltfNode>::default();
        let nodes: Vec<Handle<GltfNode>> = Vec::new();

        let extensions = VrmExtensions {
            vrmc_vrm: empty_vrmc_vrm(None),
            vrmc_spring_bone: None,
        };

        let registry = FirstPersonMeshRegistry::new(&extensions, &node_assets, &nodes);
        assert!(registry.is_empty());
    }

    #[test]
    fn unknown_first_person_type_falls_back_to_unknown_variant() {
        let parsed: FirstPersonType = serde_json::from_str("\"someFutureType\"").unwrap();
        assert_eq!(parsed, FirstPersonType::Unknown);
    }
}
