use super::*;
use crate::constants::{
    LADDER_BASE_OVERSHOOT, LADDER_FUNNEL_GAIN, LADDER_RAIL_INSET, LADDER_STANDOFF_CLEARANCE, LEVEL_HEIGHT,
    PHYSICS_EPSILON,
};

// Where the plane clamp holds the player against `test_ladder`, measured
// from the RAIL plane (a Z-facing ladder, so the collider's depth is the
// leading extent). The rails sit at z = -LADDER_RAIL_INSET.
fn player_hold_distance() -> f32 {
    player_physics().collider.depth / 2.0 + LADDER_STANDOFF_CLEARANCE
}

fn rail_plane_z() -> f32 {
    -LADDER_RAIL_INSET
}

fn ladder_step(
    world: &CollisionWorld,
    start: Position,
    vertical_velocity: f32,
    target_x: f32,
    target_z: f32,
) -> CharacterMovementResult {
    step_character_movement(
        character_step_toward(start, vertical_velocity, target_x, target_z, 0.1),
        &CharacterEnvironment {
            collision_world: world,
            gravity: TEST_GRAVITY,
            passable_kinds: &[],
            powered_bridges: &[],
            ladder_climb_ratio: test_ladders(),
            physics: player_physics(),
            portals: None,
        },
    )
}

fn ladder_step_with_external_displacement(
    world: &CollisionWorld,
    start: Position,
    control_velocity: Vec3,
    external_displacement: Vec3,
) -> CharacterMovementResult {
    step_character_movement(
        CharacterStep {
            start,
            vertical_velocity: 0.0,
            control_velocity,
            external_displacement,
            delta: 0.1,
        },
        &CharacterEnvironment {
            collision_world: world,
            gravity: TEST_GRAVITY,
            passable_kinds: &[],
            powered_bridges: &[],
            ladder_climb_ratio: test_ladders(),
            physics: player_physics(),
            portals: None,
        },
    )
}

#[test]
fn pushing_toward_ladder_face_climbs_at_into_speed() {
    let world = ladder_collision_world(&[ladder_front_base_floor()], &[test_ladder()]);
    let start = Position {
        x: 0.0,
        y: 0.0,
        z: -0.5,
    };

    // 0.2 toward the face over delta 0.1 = 2 m/s into it.
    let step = ladder_step(&world, start, 0.0, start.x, -0.3);

    let expected = 2.0 * test_ladders();
    assert!((step.vertical_velocity - expected).abs() < 1e-4);
    assert!(step.position.y > start.y);
    assert_eq!(step.support, CharacterSupport::Ladder);
}

#[test]
fn grazing_move_along_the_face_does_not_climb() {
    let world = ladder_collision_world(&[], &[test_ladder()]);
    let start = Position {
        x: 0.0,
        y: 2.0,
        z: -0.5,
    };

    // Mostly parallel to the face; the small toward-component fails the
    // facing gate, so this is a slide, not a climb.
    let step = ladder_step(&world, start, 0.0, start.x + 0.2, start.z + 0.05);

    assert!(step.vertical_velocity <= 0.0);
}

#[test]
fn slow_drift_into_the_face_does_not_climb() {
    let world = ladder_collision_world(&[], &[test_ladder()]);
    let start = Position {
        x: 0.0,
        y: 2.0,
        z: -0.5,
    };

    // Straight in but at 0.5 m/s — below the absolute climb-speed floor
    // (reconciliation nudges and knockback tails look like this).
    let step = ladder_step(&world, start, 0.0, start.x, start.z + 0.05);

    assert!(step.vertical_velocity <= 0.0);
}

#[test]
fn external_displacement_does_not_start_a_climb() {
    let world = ladder_collision_world(&[], &[test_ladder()]);
    let start = Position {
        x: 0.0,
        y: 2.0,
        z: -0.5,
    };

    let step = ladder_step_with_external_displacement(&world, start, Vec3::ZERO, Vec3::Z * 0.2);

    assert_eq!(step.vertical_velocity, 0.0);
    assert!((step.position.y - start.y).abs() < 1e-4);
}

#[test]
fn external_displacement_does_not_change_climb_speed() {
    let world = ladder_collision_world(&[], &[test_ladder()]);
    let start = Position {
        x: 0.0,
        y: 2.0,
        z: -0.5,
    };

    let step = ladder_step_with_external_displacement(&world, start, Vec3::Z * 2.0, Vec3::NEG_Z * 0.1);

    let expected = 2.0 * test_ladders();
    assert!((step.vertical_velocity - expected).abs() < 1e-4);
}

#[test]
fn base_walk_through_is_blocked_and_climbs() {
    // Ground on both sides of the plane: pushing into the ladder must climb,
    // never pass through to the floor across.
    let world = ladder_collision_world(&[ladder_front_base_floor(), ladder_back_base_floor()], &[test_ladder()]);
    let start = Position {
        x: 0.0,
        y: 0.0,
        z: -0.5,
    };

    let step = ladder_step(&world, start, 0.0, start.x, -0.1);

    assert!(step.position.z <= -player_hold_distance() + 0.01);
    assert!(step.vertical_velocity > 0.0);
}

#[test]
fn back_side_base_push_walks_through() {
    // The back of a ladder is not a ladder: with ground on both sides, a
    // back-side walker passes through the plane and keeps walking on the
    // front floor — no climb, no fence.
    let world = ladder_collision_world(&[ladder_front_base_floor(), ladder_back_base_floor()], &[test_ladder()]);
    let mut pos = Position { x: 0.0, y: 0.0, z: 0.5 };
    let mut vertical_velocity = 0.0;

    for _ in 0..6 {
        let step = ladder_step(&world, pos, vertical_velocity, pos.x, pos.z - 0.4);
        pos = step.position;
        vertical_velocity = step.vertical_velocity;
    }

    assert!(pos.z < -1.0);
    assert!(pos.y.abs() < 0.05);
}

#[test]
fn back_side_base_entry_over_void_hangs_at_last_rung() {
    // Floor behind the ladder only. Walking through the plane at the base
    // reads as a descent; it must end hanging on the bottom overshoot, not
    // slip off the volume's lower bound by a rounding error and free-fall.
    let world = ladder_collision_world(&[ladder_back_base_floor()], &[test_ladder()]);
    let bottom_y = -LADDER_BASE_OVERSHOOT;
    let mut pos = Position { x: 0.0, y: 0.0, z: 0.5 };
    let mut vertical_velocity = 0.0;
    let mut reached_bottom = false;

    for _ in 0..20 {
        let step = ladder_step(&world, pos, vertical_velocity, pos.x, pos.z - 0.6);
        pos = step.position;
        vertical_velocity = step.vertical_velocity;
        if reached_bottom {
            assert_eq!(step.support, CharacterSupport::Ladder);
            assert!((pos.y - bottom_y).abs() <= PHYSICS_EPSILON);
        }
        reached_bottom |= step.support == CharacterSupport::Ladder && pos.y <= bottom_y + PHYSICS_EPSILON;
    }

    assert!(reached_bottom);
    assert!(pos.z < rail_plane_z());
}

#[test]
fn climb_from_last_rung_hang_holds_at_the_plane() {
    // Hanging on the bottom overshoot over a void, pushing into the face
    // must climb against the plane, not walk through it onto the floor
    // behind.
    let world = ladder_collision_world(&[ladder_back_base_floor()], &[test_ladder()]);
    let mut pos = Position {
        x: 0.0,
        y: -LADDER_BASE_OVERSHOOT,
        z: rail_plane_z() - player_hold_distance(),
    };
    let mut vertical_velocity = 0.0;

    for _ in 0..6 {
        let step = ladder_step(&world, pos, vertical_velocity, pos.x, pos.z + 0.4);
        assert!(step.position.z <= rail_plane_z() - player_hold_distance() + 0.01);
        assert!(step.vertical_velocity > 0.0);
        assert_eq!(step.support, CharacterSupport::Ladder);
        pos = step.position;
        vertical_velocity = step.vertical_velocity;
    }

    assert!(pos.y > 0.0);
}

#[test]
fn back_side_is_not_a_ladder() {
    // Mid-air behind the plane: no latch, no ride — plain gravity.
    let world = ladder_collision_world(&[], &[test_ladder()]);
    let start = Position { x: 0.0, y: 2.0, z: 0.5 };

    let step = ladder_step(&world, start, 0.0, start.x, start.z - 0.2);

    assert!(step.vertical_velocity < 0.0);
    assert_eq!(step.support, CharacterSupport::Airborne);
}

#[test]
fn walking_off_back_side_balcony_mounts_and_descends() {
    // The mid-ladder mount: a balcony behind the two-storey ladder at its
    // midpoint landing. Walking off it passes through the plane, lands on
    // the front face, and keeps descending while the push is held — the
    // same input as mounting from the top landing.
    let world = ladder_collision_world(&[ladder_back_landing_floor()], &[test_ladder_two_storey()]);
    let mut pos = Position {
        x: 0.0,
        y: LEVEL_HEIGHT,
        z: 0.5,
    };
    let mut vertical_velocity = 0.0;

    for _ in 0..20 {
        let step = ladder_step(&world, pos, vertical_velocity, pos.x, pos.z - 0.4);
        assert!(step.position.y <= LEVEL_HEIGHT + 0.05);
        pos = step.position;
        vertical_velocity = step.vertical_velocity;
    }

    assert!(pos.z < rail_plane_z());
    assert!(pos.y < LEVEL_HEIGHT - 1.0);
}

#[test]
fn stepping_off_landing_onto_ladder_is_allowed() {
    // The descend mount: feet level with the band top are already past the
    // fence, so walking off the top landing onto the ladder just works.
    let world = ladder_collision_world(&[ladder_back_landing_floor()], &[test_ladder()]);
    let start = Position {
        x: 0.0,
        y: LEVEL_HEIGHT,
        z: 0.3,
    };

    let step = ladder_step(&world, start, 0.0, start.x, -0.05);

    assert!(step.position.z < 0.0);
}

#[test]
fn wide_side_climb_clears_the_landing_slab_underside() {
    // The ladder faces the collider's wide (1.0 m) axis. The face-based
    // stand-off must keep the body clear of the plane, or the climb sweeps
    // into the landing slab's underside and stalls below the top.
    let world = ladder_collision_world(&[ladder_back_landing_floor_x()], &[test_ladder_facing_x()]);
    // Feet placed so the collider top crosses the slab's bottom this tick.
    let start = Position {
        x: -0.55,
        y: 2.15,
        z: 0.0,
    };

    let step = ladder_step(&world, start, 2.0, -0.15, start.z);

    assert!(step.position.y > start.y + 0.1);
}

#[test]
fn fall_through_volume_latches() {
    let world = ladder_collision_world(&[], &[test_ladder()]);
    let start = Position {
        x: 0.0,
        y: 2.0,
        z: -0.5,
    };

    let step = ladder_step(&world, start, -10.0, start.x, start.z);

    assert_eq!(step.vertical_velocity, 0.0);
    assert_eq!(step.support, CharacterSupport::Ladder);
}

#[test]
fn idle_on_ladder_holds_position() {
    let world = ladder_collision_world(&[], &[test_ladder()]);
    let start = Position {
        x: 0.35,
        y: 2.0,
        z: -0.5,
    };

    let step = ladder_step(&world, start, 0.0, start.x, start.z);

    assert_eq!(step.vertical_velocity, 0.0);
    assert!((step.position.x - start.x).abs() < 1e-4);
    assert!((step.position.y - start.y).abs() < 1e-4);
    assert_eq!(step.support, CharacterSupport::Ladder);
}

#[test]
fn climbing_player_is_funneled_toward_ladder_axis() {
    let world = ladder_collision_world(&[], &[test_ladder()]);
    let start = Position {
        x: 0.35,
        y: 2.0,
        z: -0.5,
    };

    let step = ladder_step(&world, start, 0.0, start.x, start.z + 0.2);

    let expected_x = start.x - start.x * LADDER_FUNNEL_GAIN * 0.1;
    assert!((step.position.x - expected_x).abs() < 1e-4);
    assert!(step.vertical_velocity > 0.0);
    assert_eq!(step.support, CharacterSupport::Ladder);
}

#[test]
fn ladder_funnel_works_for_either_edge_axis() {
    let world = ladder_collision_world(&[], &[test_ladder_facing_x()]);
    let start = Position {
        x: -0.5,
        y: 2.0,
        z: 0.35,
    };

    let step = ladder_step(&world, start, 0.0, start.x + 0.2, start.z);

    let expected_z = start.z - start.z * LADDER_FUNNEL_GAIN * 0.1;
    assert!((step.position.z - expected_z).abs() < 1e-4);
    assert!(step.vertical_velocity > 0.0);
    assert_eq!(step.support, CharacterSupport::Ladder);
}

#[test]
fn diagonal_climb_receives_ladder_funnel_resistance() {
    let world = ladder_collision_world(&[], &[test_ladder()]);
    let start = Position {
        x: 0.3,
        y: 2.0,
        z: -0.5,
    };

    let step = ladder_step(&world, start, 0.0, start.x + 0.1, start.z + 0.2);

    let unassisted_x = start.x + 0.1;
    let expected_x = unassisted_x - start.x * LADDER_FUNNEL_GAIN * 0.1;
    assert!((step.position.x - expected_x).abs() < 1e-4);
    assert!(step.vertical_velocity > 0.0);
    assert_eq!(step.support, CharacterSupport::Ladder);
}

#[test]
fn strongly_sideways_climb_overpowers_ladder_funnel() {
    let world = ladder_collision_world(&[], &[test_ladder()]);
    let start = Position {
        x: 0.1,
        y: 2.0,
        z: -0.5,
    };

    // The sideways input is stronger than the additive centering pull, so
    // the player moves off-axis while still receiving some resistance.
    let step = ladder_step(&world, start, 0.0, start.x + 0.2, start.z + 0.15);

    assert!(step.position.x > start.x);
    assert!(step.position.x < 0.3);
    assert!(step.vertical_velocity > 0.0);
    assert_eq!(step.support, CharacterSupport::Ladder);
}

#[test]
fn grounded_player_near_ladder_is_not_funneled() {
    let world = ladder_collision_world(&[ladder_front_base_floor()], &[test_ladder()]);
    let start = Position {
        x: 0.3,
        y: 0.0,
        z: -0.5,
    };

    let step = ladder_step(&world, start, 0.0, start.x, start.z);

    assert!((step.position.x - start.x).abs() < 1e-4);
    assert_eq!(step.support, CharacterSupport::Ground);
}

#[test]
fn pressing_away_descends_at_input_speed() {
    let world = ladder_collision_world(&[], &[test_ladder()]);
    let start = Position {
        x: 0.0,
        y: 2.0,
        z: -0.5,
    };

    // 0.2 away from the face over delta 0.1 = 2 m/s; the descent mirrors the
    // ascent rate and the horizontal motion is pinned to the hold line.
    let step = ladder_step(&world, start, 0.0, start.x, -0.7);

    let expected = -2.0 * test_ladders();
    assert!((step.vertical_velocity - expected).abs() < 1e-4);
    assert!(step.position.y < start.y);
    assert!((step.position.z - (rail_plane_z() - player_hold_distance())).abs() < 0.01);
    assert_eq!(step.support, CharacterSupport::Ladder);
}

#[test]
fn pressing_away_at_base_walks_off() {
    let world = ladder_collision_world(&[ladder_front_base_floor()], &[test_ladder()]);
    let start = Position {
        x: 0.0,
        y: 0.0,
        z: -0.5,
    };

    let step = ladder_step(&world, start, 0.0, start.x, -0.7);

    assert!(step.position.z < -0.65);
    assert!(step.position.y.abs() < 0.05);
    assert_eq!(step.support, CharacterSupport::Ground);
}

#[test]
fn holding_forward_climbs_past_intermediate_landing() {
    let world = ladder_collision_world(&[ladder_back_landing_floor()], &[test_ladder_two_storey()]);
    let start = Position {
        x: 0.0,
        y: LEVEL_HEIGHT + 0.05,
        z: -player_hold_distance(),
    };

    // Ascending (positive vertical velocity): the intermediate landing must
    // not capture the climb.
    let step = ladder_step(&world, start, 2.0, start.x, -0.15);

    assert!(step.position.z <= -player_hold_distance() + 0.01);
    assert!(step.position.y > start.y);
}

#[test]
fn intermediate_landing_never_releases_the_fence() {
    let world = ladder_collision_world(&[ladder_back_landing_floor()], &[test_ladder_two_storey()]);
    let start = Position {
        x: 0.0,
        y: LEVEL_HEIGHT + 0.05,
        z: -player_hold_distance(),
    };

    // Even at rest at landing height, the band runs to the ladder's top —
    // forward keeps climbing instead of stepping off.
    let step = ladder_step(&world, start, 0.0, start.x, -0.15);

    assert!(step.position.z <= -player_hold_distance() + 0.01);
    assert!(step.vertical_velocity > 0.0);
}

#[test]
fn climb_holds_at_the_plane_mid_storey() {
    let world = ladder_collision_world(&[ladder_back_landing_floor()], &[test_ladder()]);
    let start = Position {
        x: 0.0,
        y: 2.0,
        z: -0.4,
    };

    let step = ladder_step(&world, start, 2.0, start.x, -0.1);

    assert!(step.vertical_velocity > 0.0);
    assert!(step.position.z <= -player_hold_distance() + 0.01);
    assert!(step.position.y > start.y);
}

#[test]
fn crest_above_the_top_landing_crosses_freely() {
    let world = ladder_collision_world(&[ladder_back_landing_floor()], &[test_ladder()]);
    let start = Position {
        x: 0.0,
        y: LEVEL_HEIGHT + 0.05,
        z: -player_hold_distance(),
    };

    // Feet above the top landing: the band has ended, the plane is open.
    let step = ladder_step(&world, start, 2.0, start.x, -0.15);

    assert!(step.position.z > -0.3);
}

#[test]
fn jump_detaches_mid_climb() {
    let world = ladder_collision_world(&[], &[test_ladder()]);
    let pos = Position {
        x: 0.0,
        y: 2.0,
        z: -0.5,
    };
    let vertical_velocity = player_jump_velocity(4.0, &world, player_physics(), 12.0, &pos, &[]);

    assert_eq!(vertical_velocity, Some(12.0));
}

#[test]
fn jump_refused_airborne_outside_ladder() {
    let world = ladder_collision_world(&[], &[test_ladder()]);
    let pos = Position {
        x: 0.0,
        y: 2.0,
        z: -3.0,
    };
    assert_eq!(
        player_jump_velocity(0.0, &world, player_physics(), 12.0, &pos, &[]),
        None
    );
}

#[test]
fn jump_refused_airborne_behind_ladder() {
    let world = ladder_collision_world(&[], &[test_ladder()]);
    let pos = Position { x: 0.0, y: 2.0, z: 0.4 };
    assert_eq!(
        player_jump_velocity(0.0, &world, player_physics(), 12.0, &pos, &[]),
        None
    );
}

#[test]
fn stepping_through_base_back_catches_the_ladder() {
    // A hanging ladder (base floor only behind it, void in front): walking
    // through the back at the base storey grabs the ladder — the volume's
    // base overshoot catches the crossing and the descent clamps at the
    // bottom, hanging at the last rung instead of falling off.
    let world = ladder_collision_world(&[ladder_back_base_floor()], &[test_ladder()]);
    let mut pos = Position { x: 0.0, y: 0.0, z: 0.5 };
    let mut vertical_velocity = 0.0;

    for _ in 0..15 {
        let step = ladder_step(&world, pos, vertical_velocity, pos.x, pos.z - 0.4);
        pos = step.position;
        vertical_velocity = step.vertical_velocity;
    }

    assert!(pos.z < rail_plane_z());
    assert!(pos.y >= -crate::constants::LADDER_BASE_OVERSHOOT - 0.05);
    assert!(pos.y < -0.1);

    // Pushing back toward the plane climbs up from the hang.
    let step = ladder_step(&world, pos, vertical_velocity, pos.x, pos.z + 0.2);
    assert!(step.vertical_velocity > 0.0);
}

#[test]
fn descending_stops_hanging_at_the_bottom() {
    let world = ladder_collision_world(&[], &[test_ladder()]);
    let mut pos = Position {
        x: 0.0,
        y: 2.0,
        z: -0.5,
    };
    let mut vertical_velocity = 0.0;

    for _ in 0..40 {
        let step = ladder_step(&world, pos, vertical_velocity, pos.x, pos.z - 0.2);
        pos = step.position;
        vertical_velocity = step.vertical_velocity;
    }

    assert!((pos.y - -crate::constants::LADDER_BASE_OVERSHOOT).abs() < 0.05);
}
