use bevy::{light::NotShadowCaster, prelude::*};

use super::BridgeAssets;
use crate::{constants::BRIDGE_EDGE_GAP, map::MapLevel};
use common::{
    constants::PHYSICS_EPSILON,
    protocol::{LightBridge, MapLayout, MapSettings},
};

#[derive(Component)]
pub struct LightBridgeMarker;

// One entity per `LightBridge` rectangle in the current `MapLayout`. Level
// focus drives `Visibility`; the powered state only moves the kind
// material's alpha, so an unpowered bridge still shows as a ghost.
pub fn bridges_spawn_system(
    mut commands: Commands,
    map_layout: Res<MapLayout>,
    map_settings: Res<MapSettings>,
    bridge_assets: Res<BridgeAssets>,
    existing: Query<Entity, With<LightBridgeMarker>>,
) {
    if !map_layout.is_changed() {
        return;
    }

    for entity in &existing {
        commands.entity(entity).despawn();
    }

    // The inset clears a floor's extension past its grid line (half the wall
    // thickness) by the visible gap.
    let inset = map_settings.geometry.wall_half_thickness() + BRIDGE_EDGE_GAP;
    for bridge in &map_layout.light_bridges {
        spawn_bridge(&mut commands, &bridge_assets, bridge, &map_layout.light_bridges, inset);
    }
}

fn spawn_bridge(commands: &mut Commands, assets: &BridgeAssets, bridge: &LightBridge, all: &[LightBridge], inset: f32) {
    let (min_x, max_x, min_z, max_z) = render_bounds(bridge, all, inset);
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
                bridge.y - bridge.thickness / 2.0,
                f32::midpoint(min_z, max_z),
            ),
            rotation: Quat::IDENTITY,
            scale: Vec3::new(max_x - min_x, 1.0, max_z - min_z),
        },
        Visibility::Visible,
    ));
}

// The slab's footprint: the rectangle inset on every side that meets no
// other bridge, so a merged walkway reads as one surface while a floor
// beside it shows a gap.
fn render_bounds(bridge: &LightBridge, all: &[LightBridge], inset: f32) -> (f32, f32, f32, f32) {
    let (min_x, max_x, min_z, max_z) = bridge.bounds_xz();
    let others = all.iter().filter(|other| other.level == bridge.level);
    let mut flush = [false; 4];
    for other in others {
        let (ox1, ox2, oz1, oz2) = other.bounds_xz();
        let z_overlap = spans_overlap(oz1, oz2, min_z, max_z);
        let x_overlap = spans_overlap(ox1, ox2, min_x, max_x);
        flush[0] |= z_overlap && (ox2 - min_x).abs() < PHYSICS_EPSILON;
        flush[1] |= z_overlap && (ox1 - max_x).abs() < PHYSICS_EPSILON;
        flush[2] |= x_overlap && (oz2 - min_z).abs() < PHYSICS_EPSILON;
        flush[3] |= x_overlap && (oz1 - max_z).abs() < PHYSICS_EPSILON;
    }
    let inset = |free: bool| if free { 0.0 } else { inset };
    (
        min_x + inset(flush[0]),
        max_x - inset(flush[1]),
        min_z + inset(flush[2]),
        max_z - inset(flush[3]),
    )
}

// Open-interval overlap, so rectangles touching only at a corner stay apart.
fn spans_overlap(a1: f32, a2: f32, b1: f32, b2: f32) -> bool {
    a1 < b2 - PHYSICS_EPSILON && b1 < a2 - PHYSICS_EPSILON
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_geometry::BRIDGE_THICKNESS;
    use common::protocol::BridgeKindId;

    const BRIDGE_EDGE_INSET: f32 = 0.35;

    fn bridge(x1: f32, z1: f32, x2: f32, z2: f32, level: u8) -> LightBridge {
        LightBridge {
            x1,
            z1,
            x2,
            z2,
            y: 0.0,
            thickness: BRIDGE_THICKNESS,
            level,
            kind: BridgeKindId(0),
        }
    }

    #[test]
    fn a_lone_slab_is_inset_on_every_side() {
        let lone = bridge(0.0, 0.0, 4.0, 2.0, 0);
        assert_eq!(
            render_bounds(&lone, &[lone], BRIDGE_EDGE_INSET),
            (
                BRIDGE_EDGE_INSET,
                4.0 - BRIDGE_EDGE_INSET,
                BRIDGE_EDGE_INSET,
                2.0 - BRIDGE_EDGE_INSET
            )
        );
    }

    #[test]
    fn a_shared_edge_stays_flush_and_the_rest_is_inset() {
        let walkway = bridge(0.0, 0.0, 4.0, 2.0, 0);
        let spur = bridge(4.0, 0.0, 6.0, 2.0, 0);
        let all = [walkway, spur];
        let (min_x, max_x, min_z, max_z) = render_bounds(&walkway, &all, BRIDGE_EDGE_INSET);
        assert_eq!((min_x, max_x), (BRIDGE_EDGE_INSET, 4.0));
        assert_eq!((min_z, max_z), (BRIDGE_EDGE_INSET, 2.0 - BRIDGE_EDGE_INSET));
        let (min_x, max_x, _, _) = render_bounds(&spur, &all, BRIDGE_EDGE_INSET);
        assert_eq!((min_x, max_x), (4.0, 6.0 - BRIDGE_EDGE_INSET));
    }

    #[test]
    fn a_corner_touch_or_another_level_does_not_count_as_a_neighbour() {
        let slab = bridge(0.0, 0.0, 2.0, 2.0, 0);
        let diagonal = bridge(2.0, 2.0, 4.0, 4.0, 0);
        let above = bridge(2.0, 0.0, 4.0, 2.0, 1);
        let (_, max_x, _, max_z) = render_bounds(&slab, &[slab, diagonal, above], BRIDGE_EDGE_INSET);
        assert_eq!((max_x, max_z), (2.0 - BRIDGE_EDGE_INSET, 2.0 - BRIDGE_EDGE_INSET));
    }
}
