use bevy::prelude::*;
use common::protocol::{MapLayout, MapSettings};

use crate::{
    carriers::{CarrierEntities, CarrierStoreys},
    config::ClientSettings,
};

use super::{FieldMaterials, FieldMeshes, VisualField, merge_fields, spawn_field_visual};

#[derive(Component)]
pub struct EraserMarker;

pub fn erasers_spawn_system(
    mut commands: Commands,
    layout: Res<MapLayout>,
    settings: Res<MapSettings>,
    client_settings: Res<ClientSettings>,
    carriers: Res<CarrierEntities>,
    storeys: Res<CarrierStoreys>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    existing: Query<Entity, With<EraserMarker>>,
) {
    if !layout.is_changed() {
        return;
    }
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    if layout.erasers.is_empty() {
        return;
    }
    let meshes = FieldMeshes::new(&mut meshes);
    let config = client_settings.vfx.erasers;
    let materials = FieldMaterials::new(
        &mut materials,
        Color::srgb(0.7, 0.4, 1.0),
        config.opacity,
        config.emissive_brightness,
    );
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
            .with_children(|parent| spawn_field_visual(parent, &meshes, &materials, &field, &layout));
    }
}
