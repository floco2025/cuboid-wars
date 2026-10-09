use super::*;
use crate::test_fixtures::{self, FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_HEIGHT, WALL_THICKNESS};
use common::{
    config::MultiShotConfig,
    protocol::{CarrierId, Floor, MapLayout, Ramp, RampDirection, RampShape, Wall},
};

const RADIUS: f32 = 0.11;

// One allowed pattern, built the way the config loader builds them.
fn multi_shot(column_degrees: f32, row_degrees: f32, stencil: &[&str]) -> MultiShotConfig {
    serde_json::from_value(serde_json::json!({
        "spread_degrees": 1.0,
        "allowed_patterns": ["multi_shot"],
        "patterns": {
            "multi_shot": { "column_scale": column_degrees, "row_scale": row_degrees, "stencil": stencil }
        },
    }))
    .expect("stencil rejected")
}

fn wall(x1: f32, z1: f32, x2: f32, z2: f32) -> Wall {
    Wall {
        x1,
        z1,
        x2,
        z2,
        width: WALL_THICKNESS,
        level: 0,
        y: 0.0,
        height: WALL_HEIGHT,
        carrier: CarrierId::WORLD,
    }
}

fn collision_world(walls: &[Wall], ramps: &[Ramp], floors: &[Floor]) -> CollisionWorld {
    CollisionWorld::from_map_layout(&MapLayout {
        walls: walls.to_vec(),
        ramps: ramps.to_vec(),
        floors: floors.to_vec(),
        ..Default::default()
    })
}

fn blocked(world: &CollisionWorld, start: Position, end: Position) -> bool {
    projectile_spawn_is_blocked(&start, &end, RADIUS, world, &[])
}

#[test]
fn a_muzzle_starting_inside_a_wall_or_floor_is_blocked() {
    let eye = test_fixtures::gameplay_config().player.eye_height();
    let walled = collision_world(&[wall(-2.0, 1.0, 2.0, 1.0)], &[], &[]);
    assert!(blocked(
        &walled,
        Position { x: 0.0, y: eye, z: 1.0 },
        Position { x: 0.0, y: eye, z: 2.0 }
    ));
    let floored = collision_world(
        &[],
        &[],
        &[Floor {
            x1: -2.0,
            z1: -2.0,
            x2: 2.0,
            z2: 2.0,
            y: LEVEL_HEIGHT,
            thickness: FLOOR_THICKNESS,
            level: 1,
            carrier: CarrierId::WORLD,
        }],
    );
    assert!(blocked(
        &floored,
        Position {
            x: 0.0,
            y: LEVEL_HEIGHT,
            z: 0.0
        },
        Position {
            x: 0.0,
            y: LEVEL_HEIGHT + 1.0,
            z: 0.0
        }
    ));
}

#[test]
fn a_spawn_path_into_a_ramp_side_is_blocked_and_one_out_of_it_is_not() {
    let world = collision_world(
        &[],
        &[Ramp {
            x1: 0.0,
            z1: 0.0,
            x2: 4.0,
            z2: 8.0,
            y: 0.0,
            height: LEVEL_HEIGHT,
            direction: RampDirection::South,
            shape: RampShape::Solid,
            thickness: 0.4,
            level: 0,
            levels: 1,
            carrier: CarrierId::WORLD,
        }],
        &[],
    );
    let at = |x| Position { x, y: 1.4, z: 4.0 };
    assert!(!blocked(&world, at(0.2), at(0.05)), "escaping out of the side");
    assert!(blocked(&world, at(0.2), at(0.8)), "deeper into the ramp");
    assert!(blocked(&world, at(-0.2), at(0.2)), "entering from outside");
}

#[test]
fn multi_shot_fires_the_configured_stencil() {
    let mut gameplay = test_fixtures::gameplay_config();
    gameplay.projectiles.multi_shot = multi_shot(1.5, 1.5, &["x.x", ".o.", "x.x"]);
    let world = CollisionWorld::from_map_layout(&MapLayout::default());
    let shooter = Position { x: 0.0, y: 1.0, z: 0.0 };
    let (yaw, pitch) = (0.3, 0.1);
    let close = |a: f32, b: f32| (a - b).abs() < 1e-5;

    let single = calculate_projectile_spawns(&shooter, yaw, pitch, 0, &gameplay, &world, &[], MuzzleCheck::Enforced);
    assert_eq!(single.len(), 1);
    assert!(close(single[0].direction_yaw, yaw) && close(single[0].direction_pitch, pitch));

    let spread = 1.5_f32.to_radians();
    let multi = calculate_projectile_spawns(&shooter, yaw, pitch, 1, &gameplay, &world, &[], MuzzleCheck::Enforced);
    let offsets: Vec<(f32, f32)> = multi
        .iter()
        .map(|spawn| (spawn.direction_yaw - yaw, spawn.direction_pitch - pitch))
        .collect();
    // Row-major over the stencil; screen-right is negative yaw.
    let expected = [
        (spread, spread),
        (-spread, spread),
        (0.0, 0.0),
        (spread, -spread),
        (-spread, -spread),
    ];
    assert_eq!(offsets.len(), expected.len(), "{offsets:?}");
    for ((yaw_offset, pitch_offset), (want_yaw, want_pitch)) in offsets.iter().zip(expected) {
        assert!(
            close(*yaw_offset, want_yaw) && close(*pitch_offset, want_pitch),
            "{offsets:?}"
        );
    }
}

#[test]
fn a_relayed_volley_reproduces_the_shooters_spawn_set_through_a_blocking_muzzle() {
    let mut gameplay = test_fixtures::gameplay_config();
    gameplay.projectiles.multi_shot = multi_shot(30.0, 30.0, &["xox"]);
    let shooter = Position { x: 0.0, y: 1.0, z: 0.0 };
    let open = CollisionWorld::from_map_layout(&MapLayout::default());
    // A wall beside the shooter that only one muzzle of the volley clips.
    let blocked = collision_world(&[wall(0.5, -2.0, 0.5, 2.0)], &[], &[]);
    let spawns = |world: &CollisionWorld, check| {
        calculate_projectile_spawns(&shooter, 0.0, 0.0, 1, &gameplay, world, &[], check)
            .iter()
            .map(|spawn| spawn.direction_yaw)
            .collect::<Vec<_>>()
    };
    let shooter_set = spawns(&open, MuzzleCheck::Enforced);
    assert_eq!(shooter_set.len(), 3);
    assert_eq!(spawns(&blocked, MuzzleCheck::Enforced).len(), 2);
    assert_eq!(spawns(&blocked, MuzzleCheck::Skipped), shooter_set);
}
