use std::f32::consts::FRAC_PI_2;

use super::*;
use crate::map::carrier_offset_at;

#[test]
fn a_shot_at_a_carrier_floor_places_the_portal_on_the_carrier() {
    let layout = tile_wall_layout(false);
    let (world, carriers) = tile_world(&layout, 1);

    let placement = place_on_geometry(
        Vec3::new(0.3, 3.0, 0.0),
        Vec3::NEG_Y,
        0.0,
        40.0,
        &world,
        &layout,
        &carriers,
        &[],
    )
    .expect("a shot at the tile fizzled");

    assert_eq!(placement.carrier, TILE);
    assert!(
        (placement.pos - Vec3::new(0.3, 0.0, 0.0)).length() < 1e-3,
        "landed at {}",
        placement.pos
    );
    let portal = placement.portal(PortalPairId(1), PortalEnd::A, &carriers);
    let local = Vec3::from(portal.pos);
    assert!(
        (local - Vec3::new(0.3 - 4.0 / 60.0, 0.0, 0.0)).length() < 1e-3,
        "carrier-local pos was {local}"
    );
    let frame = PortalFrame::from_portal(&portal, &carriers);
    assert!((frame.center - placement.pos).length() < 1e-4);
}

#[test]
fn a_shot_that_does_not_fit_where_it_hits_nudges_onto_the_carrier() {
    let layout = tile_wall_layout(false);
    let (world, carriers) = tile_world(&layout, 1);

    let placement = place_on_geometry(
        Vec3::new(0.3, 3.0, 0.5),
        Vec3::NEG_Y,
        0.0,
        40.0,
        &world,
        &layout,
        &carriers,
        &[],
    )
    .expect("a shot near the tile's edge fizzled");

    assert_eq!(placement.carrier, TILE);
    let offset = placement.pos - tile_center(&carriers);
    assert!(
        offset.x.abs() + PORTAL_HALF_WIDTH * PORTAL_RIM_SCALE <= 1.5 + 1e-3
            && offset.z.abs() + PORTAL_HALF_HEIGHT * PORTAL_RIM_SCALE <= 1.5 + 1e-3,
        "aperture hangs past the tile at {}",
        placement.pos
    );
}

#[test]
fn a_shot_over_the_tile_edge_lands_fully_on_one_surface() {
    let layout = tile_wall_layout(true);
    let (world, carriers) = tile_world(&layout, 1);
    let tile_edge = tile_center(&carriers).x + 1.5;

    let over_edge = place_on_geometry(
        Vec3::new(1.3, 3.0, 0.0),
        Vec3::NEG_Y,
        0.0,
        40.0,
        &world,
        &layout,
        &carriers,
        &[],
    )
    .expect("a shot over the tile's edge fizzled");
    assert_eq!(over_edge.carrier, TILE);
    assert!(
        over_edge.pos.x + PORTAL_HALF_WIDTH * PORTAL_RIM_SCALE <= tile_edge + 1e-3,
        "aperture hangs past the tile's edge at {}",
        over_edge.pos
    );

    let on_floor = place_on_geometry(
        Vec3::new(3.0, 3.0, 0.0),
        Vec3::NEG_Y,
        0.0,
        40.0,
        &world,
        &layout,
        &carriers,
        &[],
    )
    .expect("a shot at the floor beside the tile fizzled");
    assert_eq!(on_floor.carrier, CarrierId::WORLD);
}

#[test]
fn placement_portal_is_carrier_local() {
    let layout = tile_wall_layout(false);
    let (_, carriers) = tile_world(&layout, 1);
    let placement = PortalPlacement {
        pos: Vec3::new(1.0, 2.0, 3.0),
        normal: Vec3::NEG_X,
        yaw: 1.25,
        carrier: TILE,
    };

    let portal = placement.portal(PortalPairId(7), PortalEnd::B, &carriers);
    assert_eq!(Vec3::from(portal.pos), placement.pos - tile_center(&carriers));
    assert_eq!(Vec3::new(portal.nx, portal.ny, portal.nz), placement.normal);
    assert_eq!(
        (portal.pair, portal.end, portal.yaw, portal.carrier),
        (PortalPairId(7), PortalEnd::B, 1.25, TILE)
    );

    let free = PortalPlacement {
        carrier: CarrierId::WORLD,
        ..placement
    }
    .portal(PortalPairId(7), PortalEnd::B, &carriers);
    assert_eq!(Vec3::from(free.pos), placement.pos);
}

#[test]
fn a_carried_portal_follows_its_carrier_after_a_refresh() {
    let layout = tile_wall_layout(false);
    let (mut world, mut carriers) = tile_world(&layout, 1);
    let mut set = PortalSet::rebuild(&[carried_portal(0.0), wall_portal()], &world, &carriers);
    assert!(set.has_carried());
    let before = frames(&set).0.center;
    assert!((before - tile_center(&carriers)).length() < 1e-5);

    advance_tile(&mut world, &mut carriers, &mut set, 2);

    let after = frames(&set).0.center;
    assert!(
        (after - before - Vec3::new(4.0 / 60.0, 0.0, 0.0)).length() < 1e-5,
        "frame moved from {before} to {after}"
    );
    assert!(!pair(Vec3::ZERO, Vec3::Y, Vec3::new(9.0, 0.0, 0.0), Vec3::Y).has_carried());
}

#[test]
fn a_body_dropped_into_a_sliding_aperture_exits_the_wall_portal() {
    let layout = tile_wall_layout(false);
    let (mut world, mut carriers) = tile_world(&layout, 1);
    let mut set = PortalSet::rebuild(&[carried_portal(0.0), wall_portal()], &world, &carriers);
    let physics = player_physics();
    let start = Position {
        x: tile_center(&carriers).x,
        y: 0.2,
        z: 0.0,
    };

    let (hop, _, _) = run_ticks(&mut world, &mut carriers, &mut set, physics, start, 2, 30);

    let (tick, hop) = hop.expect("the body never crossed the sliding aperture");
    // Carried along until it crossed: the entry is where the tile was then.
    let tile_at_crossing = carrier_offset_at(&layout.carriers[0], tick);
    assert!(
        (hop.entry.center - tile_at_crossing).length() < 1e-3,
        "entry {} vs tile {}",
        hop.entry.center,
        tile_at_crossing
    );
    assert!(
        hop.origin.z > -10.0 && hop.origin.z < -8.5 && hop.origin.x.abs() < 1.5,
        "did not exit at the wall portal: {}",
        hop.origin
    );
}

#[test]
fn a_body_dropped_where_the_aperture_was_lands_on_the_tile() {
    let layout = tile_wall_layout(false);
    let (mut world, mut carriers) = tile_world(&layout, 15);
    let mut set = PortalSet::rebuild(&[carried_portal(0.0), wall_portal()], &world, &carriers);
    let physics = player_physics();
    let start = Position { x: 0.0, y: 0.2, z: 0.0 };

    let (hop, pos, support) = run_ticks(&mut world, &mut carriers, &mut set, physics, start, 16, 30);

    assert!(hop.is_none(), "hopped at tick {:?}", hop.map(|(tick, _)| tick));
    assert_eq!(support, CharacterSupport::Ground);
    assert!(pos.y.abs() < 0.05, "did not land on the tile: {pos:?}");
    assert!(pos.x > 1.0, "did not ride the tile after landing: {pos:?}");
}

#[test]
fn a_rider_beside_the_aperture_rides_a_full_cycle_without_a_hop() {
    let layout = tile_wall_layout(false);
    let (mut world, mut carriers) = tile_world(&layout, 1);
    let mut set = PortalSet::rebuild(&[carried_portal(FRAC_PI_2), wall_portal()], &world, &carriers);
    let physics = player_physics();
    let start = Position {
        x: tile_center(&carriers).x,
        y: 0.0,
        z: 1.1,
    };

    let (hop, pos, support) = run_ticks(&mut world, &mut carriers, &mut set, physics, start, 2, 120);

    assert!(hop.is_none(), "hopped at tick {:?}", hop.map(|(tick, _)| tick));
    assert_eq!(support, CharacterSupport::Ground);
    assert!((pos.z - 1.1).abs() < 0.05, "drifted across the tile: {pos:?}");
    assert!(
        (pos.x - tile_center(&carriers).x).abs() < 0.05,
        "lost the tile: {pos:?} vs {}",
        tile_center(&carriers)
    );
}

#[test]
fn a_rising_plane_catches_a_crossing_the_stale_test_would_miss() {
    let mut layout = tile_wall_layout(false);
    layout.carriers[0].to = Position {
        x: 0.0,
        y: LEVEL_HEIGHT,
        z: 0.0,
    };
    layout.carriers[0].levels = 1;
    let (world, carriers) = tile_world(&layout, 1);
    let rise = LEVEL_HEIGHT / 60.0;
    let carried = PortalSet::rebuild(&[carried_portal(0.0), wall_portal()], &world, &carriers);
    let physics = player_physics();
    let half_y = physics.movement_collider.height / 2.0;
    // The body's center sat 0.05 above the plane last tick and moved down
    // 0.1 while the plane rose `rise`.
    let previous_top = tile_center(&carriers).y - rise;
    let from = Vec3::new(0.0, previous_top + 0.05 - half_y, 0.0);
    let to = from - Vec3::Y * 0.1;

    assert!(
        carried
            .character_hop(
                from,
                to,
                physics,
                CharacterHopBody {
                    control_velocity: Vec3::ZERO,
                    knockback: Vec3::ZERO,
                    portal_momentum: Vec3::ZERO,
                    vertical_velocity: -3.0,
                    yaw: 0.0,
                },
                CAP,
            )
            .is_some(),
        "the rising plane's crossing was missed"
    );

    // Judged only against where the plane is now, the same motion looks
    // like it started behind it.
    let stale_portal = Portal {
        carrier: CarrierId::WORLD,
        pos: tile_center(&carriers).into(),
        ..carried_portal(0.0)
    };
    let stale = PortalSet::rebuild(&[stale_portal, wall_portal()], &world, &carriers);
    assert!(
        stale
            .character_hop(
                from,
                to,
                physics,
                CharacterHopBody {
                    control_velocity: Vec3::ZERO,
                    knockback: Vec3::ZERO,
                    portal_momentum: Vec3::ZERO,
                    vertical_velocity: -3.0,
                    yaw: 0.0,
                },
                CAP,
            )
            .is_none()
    );
}

#[test]
fn a_static_portal_ignores_a_carrier_floor_passing_behind_it() {
    // A static floor portal at the origin, and a tile sitting just under
    // the plane, inside the aperture's backing volume, as the set is built.
    let mut layout = tile_wall_layout(false);
    layout.floors.push(Floor {
        x1: -3.0,
        z1: -3.0,
        x2: 3.0,
        z2: 3.0,
        y: 0.0,
        thickness: FLOOR_THICKNESS,
        level: 0,
        carrier: CarrierId::WORLD,
    });
    layout.carriers[0].from = Position {
        x: 0.0,
        y: -0.1,
        z: 0.0,
    };
    layout.carriers[0].to = Position {
        x: 6.0,
        y: -0.1,
        z: 0.0,
    };
    let (world, carriers) = tile_world(&layout, 0);
    let set = PortalSet::rebuild(
        &[portal(PortalEnd::A, Vec3::ZERO, Vec3::Y, 0.0), wall_portal()],
        &world,
        &carriers,
    );

    let excluded = set.collision_exclusions(Vec3::ZERO, player_physics());
    assert!(!excluded.is_empty(), "the static floor does not back its own portal");
    assert!(
        excluded.iter().all(|handle| world.carrier_of(*handle).is_world()),
        "the passing tile was taken as backing"
    );
}
