use bevy::{light::NotShadowCaster, prelude::*};

use super::BridgeAssets;
use crate::{
    carriers::{CarrierEntities, CarrierStoreys},
    map::MapLevel,
};
use common::protocol::{LightBridge, MapLayout};

#[derive(Component)]
pub struct LightBridgeMarker;

// Power only changes the kind material's alpha; the ghost surface stays visible.
pub fn bridges_spawn_system(
    mut commands: Commands,
    map_layout: Res<MapLayout>,
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

    for bridge in &map_layout.light_bridges {
        spawn_bridge(
            &mut commands,
            &bridge_assets,
            carrier_entities.get(bridge.carrier),
            storeys.tag(bridge.carrier, bridge.level, 0),
            bridge,
        );
    }
}

fn spawn_bridge(
    commands: &mut Commands,
    assets: &BridgeAssets,
    carrier: Entity,
    level: MapLevel,
    bridge: &LightBridge,
) {
    commands.spawn((
        LightBridgeMarker,
        level,
        ChildOf(carrier),
        Mesh3d(assets.mesh.clone()),
        MeshMaterial3d(assets.material_for(bridge.kind).clone()),
        // Ghosts must not cast the opaque shadow used by the shadow pass.
        NotShadowCaster,
        bridge_transform(bridge),
        Visibility::Visible,
    ));
}

fn bridge_transform(bridge: &LightBridge) -> Transform {
    let (min_x, max_x, min_z, max_z) = bridge.bounds_xz();
    Transform {
        translation: Vec3::new(f32::midpoint(min_x, max_x), bridge.y, f32::midpoint(min_z, max_z)),
        rotation: Quat::IDENTITY,
        scale: Vec3::new(max_x - min_x, 1.0, max_z - min_z),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::protocol::{BridgeKindId, CarrierId};

    #[test]
    fn rectangle_matches_collision_footprint_at_the_walking_surface() {
        let bridge = LightBridge {
            x1: 4.2,
            z1: 2.2,
            x2: -0.2,
            z2: -0.2,
            y: 5.0,
            thickness: 0.3,
            level: 1,
            kind: BridgeKindId(0),
            carrier: CarrierId(1),
        };
        let transform = bridge_transform(&bridge);
        for (local_x, x) in [(-0.5, -0.2), (0.5, 4.2)] {
            for (local_z, z) in [(-0.5, -0.2), (0.5, 2.2)] {
                let point = transform.transform_point(Vec3::new(local_x, 0.0, local_z));
                assert!(point.abs_diff_eq(Vec3::new(x, bridge.y, z), 1e-5));
            }
        }
    }
}
