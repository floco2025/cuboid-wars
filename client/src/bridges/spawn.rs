use bevy::{light::NotShadowCaster, prelude::*};

use super::BridgeAssets;
use crate::map::MapLevel;
use common::{
    constants::BRIDGE_THICKNESS,
    protocol::{LightBridge, MapLayout},
};

#[derive(Component)]
pub struct LightBridgeMarker;

// One entity per `LightBridge` rectangle in the current `MapLayout`. Level
// focus drives `Visibility`; the powered state only moves the kind
// material's alpha, so an unpowered bridge still shows as a ghost.
pub fn bridges_spawn_system(
    mut commands: Commands,
    map_layout: Res<MapLayout>,
    bridge_assets: Res<BridgeAssets>,
    existing: Query<Entity, With<LightBridgeMarker>>,
) {
    if !map_layout.is_changed() {
        return;
    }

    for entity in &existing {
        commands.entity(entity).despawn();
    }

    for bridge in &map_layout.light_bridges {
        spawn_bridge(&mut commands, &bridge_assets, bridge);
    }
}

fn spawn_bridge(commands: &mut Commands, assets: &BridgeAssets, bridge: &LightBridge) {
    let (min_x, max_x, min_z, max_z) = bridge.bounds_xz();
    commands.spawn((
        LightBridgeMarker,
        MapLevel(bridge.level),
        Mesh3d(assets.mesh.clone()),
        MeshMaterial3d(assets.material_for(bridge.kind).clone()),
        // The ghost alpha is above the shadow pass's discard threshold, so
        // without this an unpowered bridge casts a fully opaque shadow.
        NotShadowCaster,
        Transform {
            translation: Vec3::new(
                f32::midpoint(min_x, max_x),
                bridge.y - BRIDGE_THICKNESS / 2.0,
                f32::midpoint(min_z, max_z),
            ),
            rotation: Quat::IDENTITY,
            scale: Vec3::new(max_x - min_x, 1.0, max_z - min_z),
        },
        Visibility::Visible,
    ));
}
