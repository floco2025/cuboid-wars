use bevy::prelude::*;
use common::protocol::{MapLayout, MapSettings};

use crate::{
    carriers::{CarrierEntities, CarrierStoreys},
    config::ClientSettings,
    constants::ERASER_COLOR,
};

use super::{FieldMeshes, KindVisual, VisualField, merge_fields, spawn_field_visual};

#[derive(Component)]
pub struct EraserMarker;

// Every eraser shares one look; the map has no eraser kinds.
#[derive(Resource)]
pub(crate) struct EraserAssets {
    visual: KindVisual,
}

impl FromWorld for EraserAssets {
    fn from_world(world: &mut World) -> Self {
        let config = world.resource::<ClientSettings>().vfx.erasers;
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        Self {
            visual: KindVisual::new(&mut materials, ERASER_COLOR, config.opacity, config.emissive_brightness),
        }
    }
}

pub(crate) fn erasers_spawn_system(
    mut commands: Commands,
    layout: Res<MapLayout>,
    settings: Res<MapSettings>,
    carriers: Res<CarrierEntities>,
    storeys: Res<CarrierStoreys>,
    meshes: Res<FieldMeshes>,
    assets: Res<EraserAssets>,
    existing: Query<Entity, With<EraserMarker>>,
) {
    if !layout.is_changed() {
        return;
    }
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    let floor_thickness = settings.geometry.floor_thickness;
    let fields = merge_fields(
        layout
            .erasers
            .iter()
            .map(|eraser| VisualField::from_eraser(eraser, floor_thickness)),
        &layout.floors,
        floor_thickness,
    );
    for field in fields {
        commands
            .spawn((
                EraserMarker,
                storeys.tag(field.carrier, field.level, field.levels.saturating_sub(1)),
                ChildOf(carriers.get(field.carrier)),
                field.transform(),
                Visibility::Inherited,
            ))
            .with_children(|parent| spawn_field_visual(parent, &meshes, &assets.visual, &field, &layout));
    }
}
