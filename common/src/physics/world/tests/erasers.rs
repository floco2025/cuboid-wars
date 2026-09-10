use super::*;
use crate::{
    config::gameplay::load_test_gameplay,
    protocol::{BarrierKindTable, Carrier, MapLayout, Wall},
};

fn field() -> Eraser {
    Eraser {
        x1: -2.0,
        z1: 0.0,
        x2: 2.0,
        z2: 0.0,
        width: 0.1,
        y: 0.0,
        height: 4.0,
        level: 0,
        carrier: CarrierId::WORLD,
    }
}

fn world(layout: &MapLayout) -> CollisionWorld {
    CollisionWorld::from_map_layout(layout, &BarrierKindTable::default())
}

#[test]
fn portal_segments_include_contacts_but_stop_at_the_endpoint() {
    let world = world(&MapLayout {
        erasers: vec![field()],
        ..Default::default()
    });
    let inside = Vec3::new(0.0, 1.0, 0.0);
    let outside = Vec3::new(0.0, 1.0, -3.0);
    let surface = Vec3::new(0.0, 1.0, -0.05);
    for (from, to, blocked) in [
        (outside, Vec3::new(0.0, 1.0, 3.0), true),
        (outside, Vec3::new(0.0, 1.0, -0.06), false),
        (outside, surface, true),
        (inside, outside, true),
        (inside, inside, true),
        (surface, surface, true),
        (outside, outside, false),
        (surface, surface + Vec3::X, true),
        (outside, outside + Vec3::X, false),
    ] {
        assert_eq!(world.eraser_blocks_segment(from, to), blocked, "{from:?} -> {to:?}");
    }
}

#[test]
fn fast_passes_and_body_overlaps_touch_without_a_solid_collision() {
    let layout = MapLayout {
        erasers: vec![field()],
        ..Default::default()
    };
    let world = world(&layout);
    let physics = load_test_gameplay()
        .expect("test gameplay config rejected")
        .player
        .physics();
    let front = Position::from(Vec3::Z * 10.0);
    let back = Position::from(Vec3::NEG_Z * 10.0);
    let contact = |start: &Position, end: &Position| {
        world
            .character_eraser_contacts(start, end, physics, None)
            .next()
            .is_some()
    };
    assert!(contact(&front, &back));
    assert!(contact(&back, &front));
    assert!(contact(&Position::default(), &Position::default()));
    let high = Position::from(Vec3::Y * 5.0);
    assert!(!contact(&high, &high));
    let low = Position::from(Vec3::NEG_Y * 10.0);
    assert!(!contact(&low, &low));
    assert!(!contact(
        &Position::from(Vec3::new(5.0, 0.0, 10.0)),
        &Position::from(Vec3::new(5.0, 0.0, -10.0)),
    ));
    assert!(world.projectile_path_clear(Vec3::new(0.0, 1.0, 10.0), Vec3::NEG_Z * 20.0, 0.3, &[]));
    assert!(world.colliders.is_empty());
}

#[test]
fn moving_field_sweeps_stationary_player_without_solid_carrier_geometry() {
    let layout = MapLayout {
        erasers: vec![Eraser {
            carrier: CarrierId(1),
            ..field()
        }],
        carriers: vec![Carrier {
            parent: CarrierId::WORLD,
            level: 0,
            levels: 1,
            from: Position::from(Vec3::Z * -3.0),
            to: Position::from(Vec3::Z * 3.0),
            travel_ticks: 1,
            pause_ticks: 0,
            phase_ticks: 0,
        }],
        ..Default::default()
    };
    let mut world = world(&layout);
    let mut carriers = Carriers::from_layout(&layout);
    let physics = load_test_gameplay()
        .expect("test gameplay config rejected")
        .player
        .physics();
    let pos = Position::default();
    let touches = |world: &CollisionWorld, carriers: Option<&Carriers>| {
        world
            .character_eraser_contacts(&pos, &pos, physics, carriers)
            .next()
            .is_some()
    };
    assert!(!touches(&world, None));
    carriers.advance(1);
    world.set_carrier_poses(&carriers);
    assert!(!touches(&world, None));
    assert!(touches(&world, Some(&carriers)));
}

#[test]
fn portal_shots_are_blocked_only_before_the_host() {
    let layout = MapLayout {
        erasers: vec![field()],
        walls: vec![Wall {
            x1: -2.0,
            z1: -3.0,
            x2: 2.0,
            z2: -3.0,
            width: 0.3,
            y: 0.0,
            height: 4.0,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let world = world(&layout);
    assert!(
        world
            .portal_surface_along_ray(Vec3::new(0.0, 1.0, 3.0), Vec3::NEG_Z, 10.0, &[])
            .is_none()
    );
    assert!(
        world
            .portal_surface_along_ray(Vec3::new(0.0, 1.0, -6.0), Vec3::Z, 10.0, &[])
            .is_some()
    );
}

#[test]
fn erasers_are_transparent_to_attacks_and_projectile_paths() {
    let world = world(&MapLayout {
        erasers: vec![field()],
        ..Default::default()
    });
    let from = Vec3::new(0.0, 1.0, -3.0);
    let to = Vec3::new(0.0, 1.0, 3.0);
    assert!(world.line_of_sight_clear(from, to));
    assert!(world.attack_path_clear(from, to, &[]));
    assert!(world.projectile_path_clear(from, to - from, 0.3, &[]));
    assert!(world.cast_moving_ball(from, to - from, 0.3).is_none());
    assert!(
        world
            .cast_moving_ball_against_fields(from, to - from, 0.3, &[])
            .is_none()
    );
}
