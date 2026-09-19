use bevy::prelude::*;

use crate::{
    carriers::{CarrierEntities, CarrierStoreys},
    config::ClientSettings,
    fields::{
        FieldAssets, FieldMeshes, FieldPiece, FieldSurface, FieldSurfaces, VisualField, fade_target, merge_fields,
        spawn_patterned_surface,
    },
    materials::{FieldMaterial, field_material},
};
use common::protocol::{MapLayout, MapSettings, SwitchState};

#[derive(Component)]
pub struct BarrierMarker;

#[expect(
    clippy::too_many_arguments,
    reason = "spawning threads the layout, the looks, and the surface registry"
)]
pub fn barriers_spawn_system(
    mut commands: Commands,
    map_layout: Res<MapLayout>,
    settings: Res<MapSettings>,
    client_settings: Res<ClientSettings>,
    field_meshes: Res<FieldMeshes>,
    field_assets: Res<FieldAssets>,
    switch_state: Res<SwitchState>,
    carrier_entities: Res<CarrierEntities>,
    storeys: Res<CarrierStoreys>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<FieldMaterial>>,
    mut surfaces: ResMut<FieldSurfaces>,
    existing: Query<Entity, With<BarrierMarker>>,
) {
    let layout = map_layout;
    if !layout.is_changed() {
        return;
    }

    for entity in &existing {
        commands.entity(entity).despawn();
    }
    surfaces.forget(FieldPiece::Barrier);

    let config = client_settings.vfx.fields;
    let fields = merge_fields(
        layout.barriers.iter().map(VisualField::from_barrier),
        &layout.floors,
        settings.geometry.floor_thickness,
    );
    for field in fields {
        let id = field.field.expect("barrier visual missing its field");
        let color = field_assets.base_color(id);
        let material = materials.add(field_material(
            color,
            fade_target(&switch_state, id, config),
            config.emissive_brightness,
        ));
        surfaces.0.push(FieldSurface {
            field: id,
            piece: FieldPiece::Barrier,
            material: material.clone(),
            base_color: color,
        });
        let root = field.transform();
        commands
            .spawn((
                BarrierMarker,
                storeys.tag(field.carrier, field.level, field.levels.saturating_sub(1)),
                ChildOf(carrier_entities.get(field.carrier)),
                root,
                Visibility::Inherited,
            ))
            .with_children(|parent| {
                spawn_patterned_surface(
                    parent,
                    &mut meshes,
                    &field_meshes,
                    &material,
                    field_assets.visual(id),
                    field.panel_rects(&layout),
                    field.frame_rects(&layout),
                    field.rect.center(),
                    field.thickness,
                    &root,
                );
            });
    }
}
