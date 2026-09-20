use super::*;
use crate::test_fixtures;
use bevy::ecs::system::SystemState;
use common::{constants::TICK_SECS, protocol::MapLayout};
use std::f32::consts::FRAC_PI_2;

#[test]
fn remote_bodies_stay_at_reported_positions_while_the_owner_simulates() {
    let gameplay = test_fixtures::gameplay_config();
    let settings = test_fixtures::map_settings();
    let collision = CollisionWorld::from_map_layout(&MapLayout::default());
    for is_local in [false, true] {
        let mut world = World::new();
        let position = Position {
            x: 0.0,
            y: 10.0,
            z: 0.0,
        };
        let entity = world
            .spawn((
                PlayerMarker,
                PlayerId(1),
                position,
                PlayerMoveIntent::Walking { direction: FRAC_PI_2 },
                CharacterVerticalVelocity(-3.0),
                AirborneMomentum(Vec3::X),
                KnockbackVelocity(Vec3::X),
                PlayerAnimationMotion::default(),
            ))
            .id();
        if is_local {
            world.entity_mut(entity).insert(LocalPlayerMarker);
        }
        let mut state = SystemState::<PlayerMovementQuery>::new(&mut world);
        let mut query = state.get_mut(&mut world).expect("movement query invalid");
        let plans = plan_player_moves(
            TICK_SECS,
            &collision,
            &settings,
            &gameplay,
            &PlayerMap::default(),
            &SwitchState::default(),
            &PortalSet::default(),
            &Carriers::default(),
            false,
            &mut query,
            &[],
        );
        state.apply(&mut world);
        if is_local {
            let plan = plans.first().expect("movement plan missing");
            assert!(plan.result.position.x > position.x);
            assert!(plan.result.position.y < position.y);
        } else {
            assert!(plans.is_empty());
            assert_eq!(*world.get::<Position>(entity).expect("position"), position);
            assert_eq!(
                world.get::<CharacterVerticalVelocity>(entity).expect("velocity").0,
                -3.0
            );
            assert_eq!(
                world
                    .get::<PlayerAnimationMotion>(entity)
                    .expect("animation missing")
                    .velocity,
                Vec3::ZERO
            );
        }
    }
}

#[test]
fn a_dead_local_player_gets_no_plan() {
    let gameplay = test_fixtures::gameplay_config();
    let settings = test_fixtures::map_settings();
    let collision = CollisionWorld::from_map_layout(&MapLayout::default());
    let mut world = World::new();
    let position = Position {
        x: 0.0,
        y: 10.0,
        z: 0.0,
    };
    let entity = world
        .spawn((
            PlayerMarker,
            LocalPlayerMarker,
            PlayerId(1),
            position,
            PlayerMoveIntent::Walking { direction: FRAC_PI_2 },
            CharacterVerticalVelocity(-3.0),
            AirborneMomentum(Vec3::X),
            KnockbackVelocity(Vec3::X),
            PlayerAnimationMotion::default(),
        ))
        .id();
    let mut state = SystemState::<PlayerMovementQuery>::new(&mut world);
    let mut query = state.get_mut(&mut world).expect("movement query invalid");
    let plans = plan_player_moves(
        TICK_SECS,
        &collision,
        &settings,
        &gameplay,
        &PlayerMap::default(),
        &SwitchState::default(),
        &PortalSet::default(),
        &Carriers::default(),
        true,
        &mut query,
        &[],
    );
    state.apply(&mut world);
    assert!(plans.is_empty());
    assert_eq!(*world.get::<Position>(entity).expect("position missing"), position);
}

fn planned_move(index: u32, start: Position, target: Position) -> CharacterMovePlan {
    let entity = Entity::from_raw_u32(index).expect("test entity index out of range");
    let physics = test_fixtures::gameplay_config().player.physics();
    CharacterMovePlan::from_target(entity, start, target, 0.0, physics, false)
}

#[test]
fn overlapping_planned_characters_can_separate() {
    let first = planned_move(
        1,
        Position { x: 0.0, y: 0.0, z: 0.0 },
        Position {
            x: -0.2,
            y: 0.0,
            z: 0.0,
        },
    );
    let second = planned_move(
        2,
        Position { x: 0.8, y: 0.0, z: 0.0 },
        Position { x: 1.0, y: 0.0, z: 0.0 },
    );
    let planned_moves = [first, second];

    assert!(overlapping_character(&first, &planned_moves).is_none());
    assert!(overlapping_character(&second, &planned_moves).is_none());
}

#[test]
fn overlapping_planned_characters_cannot_move_deeper_together() {
    let first = planned_move(
        1,
        Position { x: 0.0, y: 0.0, z: 0.0 },
        Position { x: 0.2, y: 0.0, z: 0.0 },
    );
    let second = planned_move(
        2,
        Position { x: 0.8, y: 0.0, z: 0.0 },
        Position { x: 0.6, y: 0.0, z: 0.0 },
    );
    let planned_moves = [first, second];

    assert!(overlapping_character(&first, &planned_moves).is_some());
    assert!(overlapping_character(&second, &planned_moves).is_some());
}

#[test]
fn character_blocking_commits_consistent_edges_landings_ramps_and_carrier_motion() {
    use super::super::{application::apply_player_moves, outcomes::LocalMovementStep};
    use common::{
        physics::CharacterSupport,
        protocol::{Carrier, CarrierId, CarrierMotion, Floor, Ramp, RampDirection, RampShape},
    };
    let mut gameplay = test_fixtures::gameplay_config();
    gameplay.player.movement_collider.diameter = 0.6;
    gameplay.player.movement_collider.height = 1.8;
    let physics = gameplay.player.physics();
    let mut settings = test_fixtures::map_settings();
    settings.movement.player.walk_speed = 12.0;
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
            control_velocity: Vec3::X * 12.0,
            external_displacement: Vec3::X * delta,
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
        let proposed = step_player_movement(request);
        let expected = step_player_movement(PlayerMovementStep {
            control_velocity: Vec3::ZERO,
            external_displacement: Vec3::ZERO,
            ..request
        });
        assert!(
            proposed.support != expected.support || (proposed.position.y - expected.position.y).abs() > 0.01,
            "{scene}: fixture must exercise a changed support result"
        );
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()));
        let entity = app
            .world_mut()
            .spawn((
                PlayerMarker,
                LocalPlayerMarker,
                PlayerId(1),
                start,
                PlayerMoveIntent::Walking { direction: FRAC_PI_2 },
                CharacterVerticalVelocity(vertical),
                AirborneMomentum(Vec3::X),
                KnockbackVelocity::default(),
                PlayerAnimationMotion::default(),
            ))
            .id();
        let blocker = CharacterMovePlan::stationary(
            Entity::from_raw_u32(1000).expect("entity"),
            proposed.position,
            0.0,
            physics,
        );
        let mut state = SystemState::<(Commands, PlayerMovementQuery)>::new(app.world_mut());
        let server = app.world().resource::<AssetServer>().clone();
        let (mut commands, mut query) = state.get_mut(app.world_mut()).expect("movement query");
        let plans = plan_player_moves(
            delta,
            &collision,
            &settings,
            &gameplay,
            &PlayerMap::default(),
            &SwitchState::default(),
            &portals,
            &carriers,
            false,
            &mut query,
            &[blocker],
        );
        assert!(plans[0].hits_character, "{scene}");
        assert_eq!(plans[0].result, expected, "{scene}");
        apply_player_moves(
            &mut commands,
            delta,
            &server,
            &test_fixtures::asset_set(),
            &test_fixtures::client_settings().audio,
            &mut query,
            &plans,
        );
        state.apply(app.world_mut());
        let world = app.world();
        assert_eq!(*world.get::<Position>(entity).expect("position"), expected.position);
        assert_eq!(
            world.get::<CharacterVerticalVelocity>(entity).expect("velocity").0,
            expected.vertical_velocity
        );
        assert_eq!(
            *world
                .get::<common::physics::GroundingDiagnostics>(entity)
                .expect("grounding"),
            expected.grounding
        );
        let outcome = world.get::<LocalMovementStep>(entity).expect("outcome");
        assert_eq!(
            (outcome.carrier, outcome.support, outcome.impact_speed, outcome.crushed),
            (
                expected.carrier,
                expected.support,
                expected.impact_speed,
                expected.crushed
            )
        );
        assert_eq!(world.get::<AirborneMomentum>(entity).expect("momentum").0, Vec3::ZERO);
        assert_eq!(
            world.get::<PlayerAnimationMotion>(entity).expect("animation").support,
            expected.support
        );
        if scene == "landing" {
            assert_eq!(expected.support, CharacterSupport::Airborne);
        }
        if scene == "carrier" {
            assert_eq!(expected.carrier, carrier);
            assert!(expected.position.x > start.x);
        }
    }
}
