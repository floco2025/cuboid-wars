use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    protocol::{MapLayout, MapSettings},
};

use super::{FieldMeshes, KindVisual, VisualField, spawn_field_visual};
use crate::{
    carriers::{CarrierEntities, CarrierStoreys},
    config::ClientSettings,
};

const CHECKPOINT_COLOR: Color = Color::srgb(1.0, 0.7, 0.12);

#[derive(Component)]
pub struct CheckpointMarker;

#[derive(Resource)]
pub(crate) struct CheckpointAssets {
    visual: KindVisual,
}

impl FromWorld for CheckpointAssets {
    fn from_world(world: &mut World) -> Self {
        let config = world.resource::<ClientSettings>().vfx.erasers;
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        Self {
            visual: KindVisual::new(
                &mut materials,
                CHECKPOINT_COLOR,
                config.opacity,
                config.emissive_brightness,
            ),
        }
    }
}

pub(crate) fn checkpoints_spawn_system(
    mut commands: Commands,
    layout: Res<MapLayout>,
    settings: Res<MapSettings>,
    gameplay: Res<GameplayConfig>,
    carriers: Res<CarrierEntities>,
    storeys: Res<CarrierStoreys>,
    meshes: Res<FieldMeshes>,
    assets: Res<CheckpointAssets>,
    existing: Query<Entity, With<CheckpointMarker>>,
) {
    if !layout.is_changed() {
        return;
    }
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    let height = gameplay.player.physics().movement_collider.height * 0.5;
    let thickness = settings.geometry.barrier_thickness();
    for checkpoint in &layout.checkpoints {
        for field in VisualField::checkpoint_perimeter(checkpoint, height, thickness) {
            commands
                .spawn((
                    CheckpointMarker,
                    storeys.tag(field.carrier, field.level, 0),
                    ChildOf(carriers.get(field.carrier)),
                    field.transform(),
                    Visibility::Inherited,
                ))
                .with_children(|parent| spawn_field_visual(parent, &meshes, &assets.visual, &field, &layout));
        }
    }
}
