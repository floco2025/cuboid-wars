use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;

use super::{BridgeAssets, surface::bridge_visuals};
use crate::{
    carriers::{CarrierEntities, CarrierStoreys},
    fields::{FieldMeshes, spawn_framed_surface},
};
use common::protocol::MapLayout;

#[derive(Component)]
pub struct LightBridgeMarker;

pub fn bridges_spawn_system(
    mut commands: Commands,
    map_layout: Res<MapLayout>,
    field_meshes: Res<FieldMeshes>,
    bridge_assets: Res<BridgeAssets>,
    carrier_entities: Res<CarrierEntities>,
    storeys: Res<CarrierStoreys>,
    existing: Query<Entity, With<LightBridgeMarker>>,
) {
    if !map_layout.is_changed() {
        return;
    }

    for entity in &existing {
        commands.entity(entity).despawn();
    }

    for visual in bridge_visuals(&map_layout) {
        let bridge = &visual.bridge;
        let (x1, x2, z1, z2) = bridge.bounds_xz();
        let center = Rect::new(x1, z1, x2, z2).center();
        commands
            .spawn((
                LightBridgeMarker,
                storeys.tag(bridge.carrier, bridge.level, 0),
                ChildOf(carrier_entities.get(bridge.carrier)),
                bridge_transform(center, bridge.y),
                Visibility::Inherited,
            ))
            .with_children(|parent| {
                spawn_framed_surface(
                    parent,
                    &field_meshes,
                    &bridge_assets.kinds[usize::from(bridge.kind.0)],
                    visual.surfaces,
                    visual.frames,
                    center,
                    bridge.thickness,
                );
            });
    }
}

fn bridge_transform(center: Vec2, y: f32) -> Transform {
    Transform::from_xyz(center.x, y, center.y).with_rotation(Quat::from_rotation_x(FRAC_PI_2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_matches_collision_footprint_at_the_walking_surface() {
        let surface = Rect::new(-0.2, -0.2, 4.2, 2.2);
        let transform =
            bridge_transform(surface.center(), 5.0).with_scale(Vec3::new(surface.width(), surface.height(), 1.0));
        for (local_x, x) in [(-0.5, -0.2), (0.5, 4.2)] {
            for (local_y, z) in [(-0.5, -0.2), (0.5, 2.2)] {
                let point = transform.transform_point(Vec3::new(local_x, local_y, 0.0));
                assert!(point.abs_diff_eq(Vec3::new(x, 5.0, z), 1e-5));
            }
        }
    }
}
