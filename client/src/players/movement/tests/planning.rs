use super::*;
use crate::test_fixtures;
use common::{
    map::Carriers,
    physics::{CharacterSupport, CollisionWorld, PortalSet},
    protocol::{
        Carrier, CarrierId, CarrierMotion, Floor, MapLayout, PlayerMoveIntent, Position, Ramp, RampDirection,
        RampShape, SwitchState,
    },
};
use std::f32::consts::FRAC_PI_2;

// Body `index` moving along +X from `start_x` to `target_x`.
fn planned_move(index: u32, start_x: f32, target_x: f32) -> CharacterMovePlan {
    let entity = Entity::from_raw_u32(index).expect("test entity index out of range");
    let physics = test_fixtures::gameplay_config().player.physics();
    let at = |x| Position { x, y: 0.0, z: 0.0 };
    CharacterMovePlan::from_target(entity, at(start_x), at(target_x), 0.0, physics, false)
}

#[test]
fn overlapping_bodies_can_separate_but_cannot_move_deeper_together() {
    let separating = [planned_move(1, 0.0, -0.2), planned_move(2, 0.8, 1.0)];
    let deepening = [planned_move(1, 0.0, 0.2), planned_move(2, 0.8, 0.6)];
    for plan in &separating {
        assert!(overlapping_character(plan, &separating).is_none());
    }
    for plan in &deepening {
        assert!(overlapping_character(plan, &deepening).is_some());
    }
}

#[test]
fn character_blocking_commits_consistent_edges_landings_ramps_and_carrier_motion() {
    let mut gameplay = test_fixtures::gameplay_config();
    gameplay.player.movement_collider.diameter = 0.6;
    gameplay.player.movement_collider.height = 1.8;
    let physics = gameplay.player.physics();
    let mut settings = test_fixtures::map_settings();
    settings.movement.player.move_speed = 12.0;
    let delta = 0.1;
    for scene in ["edge", "landing", "ramp", "carrier"] {
        let carrier = if scene == "carrier" {
            CarrierId(1)
        } else {
            CarrierId::WORLD
        };
        let mut layout = MapLayout::default();
        if scene == "ramp" {
            layout.ramps.push(Ramp {
                x1: 0.0,
                x2: 4.0,
                z1: -2.0,
                z2: 2.0,
                y: 0.0,
                height: 2.0,
                thickness: 0.2,
                direction: RampDirection::East,
                shape: RampShape::Solid,
                level: 0,
                levels: 1,
                carrier,
            });
        } else {
            layout.floors.push(Floor {
                x1: if scene == "landing" { 0.0 } else { -4.0 },
                x2: if scene == "landing" { 4.0 } else { 0.0 },
                z1: -3.0,
                z2: 3.0,
                y: 0.0,
                thickness: 0.2,
                level: 0,
                carrier,
            });
        }
        if scene == "carrier" {
            layout.carriers.push(Carrier {
                initially_on: true,
                motion: CarrierMotion::Cycle,
                parent: CarrierId::WORLD,
                level: 0,
                levels: 1,
                from: Position::default(),
                to: Position { x: 2.0, y: 1.0, z: 0.0 },
                travel_ticks: 10,
                pause_ticks: 0,
                phase_ticks: 0,
                switch: None,
            });
        }
        let mut collision = CollisionWorld::from_map_layout(&layout);
        let mut carriers = Carriers::from_layout(&layout);
        let portals = PortalSet::default();
        let start = match scene {
            "ramp" => Position {
                x: 1.0,
                y: 0.55,
                z: 0.0,
            },
            "landing" => Position {
                x: -0.7,
                y: 0.3,
                z: 0.0,
            },
            _ => Position {
                x: -0.3,
                y: 0.0,
                z: 0.0,
            },
        };
        let vertical = if scene == "landing" { -4.0 } else { 0.0 };
        if scene == "carrier" {
            carriers.advance(1, &SwitchState::default());
            collision.set_carrier_poses(&carriers);
        }
        let request = PlayerMovementStep {
            start,
            vertical_velocity: vertical,
            horizontal_velocity: Vec3::X * 12.0,
            stance: Default::default(),
            intent: PlayerMoveIntent::moving(FRAC_PI_2),
            has_speed: false,
            disabled: false,
            knockback_displacement: Vec3::X * delta,
            delta,
            has_low_gravity: false,
            held_keys: &[],
            open_fields: &[],
            collision_world: &collision,
            map_settings: &settings,
            gameplay_config: &gameplay,
            portal_set: &portals,
            carriers: &carriers,
        };
        let proposed = step_player_movement(request).movement;
        let expected = step_player_movement(PlayerMovementStep {
            horizontal_velocity: Vec3::ZERO,
            intent: PlayerMoveIntent::NONE,
            knockback_displacement: Vec3::ZERO,
            ..request
        })
        .movement;
        assert!(
            proposed.support != expected.support || (proposed.position.y - expected.position.y).abs() > 0.01,
            "{scene}: fixture must exercise a changed support result"
        );
        let entity = Entity::from_raw_u32(1).expect("entity");
        // The body straight ahead leaves nothing of a move along +X.
        let blocker = CharacterMovePlan::stationary(
            Entity::from_raw_u32(1000).expect("entity"),
            proposed.position,
            0.0,
            physics,
        );
        let planned = plan_player_move(entity, request, &[blocker]);
        assert!(planned.hits_character, "{scene}");
        assert_eq!(planned.step.movement, expected, "{scene}");
        assert_eq!(planned.step.horizontal_velocity, Vec3::ZERO, "{scene}");
        if scene == "landing" {
            assert_eq!(expected.support, CharacterSupport::Airborne);
        }
        if scene == "carrier" {
            assert_eq!(expected.carrier, carrier);
            assert!(expected.position.x > start.x);
        }
    }
}

// A body beside the path takes only the velocity into it, like a wall.
#[test]
fn a_body_clipped_in_passing_leaves_the_velocity_along_it() {
    let mut gameplay = test_fixtures::gameplay_config();
    gameplay.player.movement_collider.diameter = 0.6;
    gameplay.player.movement_collider.height = 1.8;
    let physics = gameplay.player.physics();
    let mut settings = test_fixtures::map_settings();
    settings.movement.player.move_speed = 6.0;
    let layout = MapLayout {
        floors: vec![Floor {
            x1: -4.0,
            x2: 4.0,
            z1: -4.0,
            z2: 4.0,
            y: 0.0,
            thickness: 0.2,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..default()
    };
    let collision = CollisionWorld::from_map_layout(&layout);
    let carriers = Carriers::default();
    let portals = PortalSet::default();
    let delta = 0.1;
    let start = Position::default();
    let request = PlayerMovementStep {
        start,
        vertical_velocity: 0.0,
        horizontal_velocity: Vec3::new(6.0, 0.0, 0.0),
        stance: Default::default(),
        intent: PlayerMoveIntent::NONE,
        has_speed: false,
        disabled: false,
        knockback_displacement: Vec3::ZERO,
        delta,
        has_low_gravity: false,
        held_keys: &[],
        open_fields: &[],
        collision_world: &collision,
        map_settings: &settings,
        gameplay_config: &gameplay,
        portal_set: &portals,
        carriers: &carriers,
    };
    let entity = Entity::from_raw_u32(1).expect("entity");
    // A body just off the path, at 45 degrees ahead and to the side.
    let beside = CharacterMovePlan::stationary(
        Entity::from_raw_u32(1000).expect("entity"),
        Position { x: 0.5, y: 0.0, z: 0.5 },
        0.0,
        physics,
    );
    let free = plan_player_move(entity, request, &[]);
    assert!(!free.hits_character);
    let clipped = plan_player_move(entity, request, &[beside]);
    assert!(clipped.hits_character);
    let velocity = clipped.step.horizontal_velocity;
    assert!(
        velocity.x > 0.0 && velocity.x < free.step.horizontal_velocity.x,
        "{velocity}"
    );
    assert!(velocity.z < 0.0, "pushed off the body's side: {velocity}");
    assert!(
        (velocity.x + velocity.z).abs() < 1e-3,
        "nothing left toward the body: {velocity}"
    );
    assert!(clipped.step.movement.position.x > start.x, "still moves along the body");
}
