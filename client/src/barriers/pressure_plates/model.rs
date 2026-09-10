use std::collections::HashMap;

use bevy::{gltf::Gltf, prelude::*, world_serialization::WorldAssetRoot};

use super::{PressurePlateMarker, animation::pressure_plate_ready};
use crate::config::AssetSet;

#[derive(Clone)]
pub(super) struct PlateAnimation {
    pub graph: Handle<AnimationGraph>,
    pub index: AnimationNodeIndex,
    pub duration: f32,
}

#[derive(Resource)]
pub(crate) struct PressurePlateModel {
    gltf: Handle<Gltf>,
    pub(super) animation: Option<PlateAnimation>,
    pub(super) materials: HashMap<(AssetId<StandardMaterial>, [u8; 3]), Handle<StandardMaterial>>,
}

impl FromWorld for PressurePlateModel {
    fn from_world(world: &mut World) -> Self {
        Self {
            gltf: world
                .resource::<AssetServer>()
                .load(world.resource::<AssetSet>().pressure_plate().scene.clone()),
            animation: None,
            materials: HashMap::new(),
        }
    }
}

pub(crate) fn pressure_plates_attach_system(
    mut commands: Commands,
    mut model: ResMut<PressurePlateModel>,
    server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    clips: Res<Assets<AnimationClip>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    pending: Query<Entity, (With<PressurePlateMarker>, Without<WorldAssetRoot>)>,
) {
    if pending.is_empty() || !server.is_loaded_with_dependencies(&model.gltf) {
        return;
    }
    let Some(gltf) = gltfs.get(&model.gltf) else { return };
    let source = gltf.scenes.first().zip(gltf.named_animations.get("Activate"));
    let Some((scene, clip)) = source else {
        error!("pressure plate model lacks its scene or Activate animation");
        for entity in &pending {
            commands.entity(entity).despawn();
        }
        return;
    };
    let Some(clip_asset) = clips.get(clip) else { return };
    model.animation.get_or_insert_with(|| {
        let (graph, index) = AnimationGraph::from_clip(clip.clone());
        PlateAnimation {
            graph: graphs.add(graph),
            index,
            duration: clip_asset.duration(),
        }
    });
    for entity in &pending {
        // Take the scene from the loaded GLB; concurrent labelled loads can respawn ready instances.
        commands
            .entity(entity)
            .insert(WorldAssetRoot(scene.clone()))
            .observe(pressure_plate_ready);
    }
}
