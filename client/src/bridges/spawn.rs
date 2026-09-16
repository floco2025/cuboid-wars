use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;

use super::{BridgeAssets, surface::bridge_visuals};
use crate::{
    carriers::{CarrierEntities, CarrierStoreys},
    config::ClientSettings,
    fields::{FieldMeshes, FieldSurface, FieldSurfaces, fade_target, spawn_patterned_surface},
    materials::{FieldMaterial, field_material},
};
use common::{
    physics::FieldKind,
    protocol::{MapLayout, SwitchState},
};

#[derive(Component)]
pub struct LightBridgeMarker;

#[expect(
    clippy::too_many_arguments,
    reason = "spawning threads the layout, the looks, and the surface registry"
)]
pub fn bridges_spawn_system(
    mut commands: Commands,
    map_layout: Res<MapLayout>,
    client_settings: Res<ClientSettings>,
    field_meshes: Res<FieldMeshes>,
    bridge_assets: Res<BridgeAssets>,
    switch_state: Res<SwitchState>,
    carrier_entities: Res<CarrierEntities>,
    storeys: Res<CarrierStoreys>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<FieldMaterial>>,
    mut surfaces: ResMut<FieldSurfaces>,
    existing: Query<Entity, With<LightBridgeMarker>>,
) {
    if !map_layout.is_changed() {
        return;
    }

    for entity in &existing {
        commands.entity(entity).despawn();
    }
    surfaces.forget_bridges();

    let config = client_settings.vfx.fields;
    for visual in bridge_visuals(&map_layout) {
        let bridge = &visual.bridge;
        let (x1, x2, z1, z2) = bridge.bounds_xz();
        let center = Rect::new(x1, z1, x2, z2).center();
        // Every member of the group shares the switch, so one stands for all.
        let state = FieldKind::Bridge(bridge.id);
        let color = bridge_assets.field_color(bridge.id);
        let material = materials.add(field_material(
            color,
            fade_target(&switch_state, state, config),
            config.emissive_brightness,
        ));
        surfaces.0.push(FieldSurface {
            state,
            material: material.clone(),
            base_color: color,
        });
        let root = bridge_transform(center, bridge.y);
        commands
            .spawn((
                LightBridgeMarker,
                storeys.tag(bridge.carrier, bridge.level, 0),
                ChildOf(carrier_entities.get(bridge.carrier)),
                root,
                Visibility::Inherited,
            ))
            .with_children(|parent| {
                spawn_patterned_surface(
                    parent,
                    &mut meshes,
                    &field_meshes,
                    &material,
                    bridge_assets.kind(bridge.kind),
                    visual.surfaces,
                    visual.frames,
                    center,
                    bridge.thickness,
                    &root,
                );
            });
    }
}

fn bridge_transform(center: Vec2, y: f32) -> Transform {
    Transform::from_xyz(center.x, y, center.y).with_rotation(Quat::from_rotation_x(FRAC_PI_2))
}

#[cfg(test)]
#[path = "tests/spawn.rs"]
mod tests;
