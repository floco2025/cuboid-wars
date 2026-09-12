use super::*;
use crate::{
    actors::{
        resources::AwarePlayer,
        test_kinds::{self, BEAM, CONTACT},
    },
    config::ActorKindServerConfig,
    map::{ActorSpawnZone, CarrierGrid, CellGrid, EdgeGrid, LevelGrid},
    test_geometry::geometry,
};
use common::{
    config::{ActorLocomotion, CharacterPhysicsConfig},
    physics::CharacterSupport,
    protocol::{CarrierId, Floor, MapLayout},
};
use rand::{SeedableRng, rngs::StdRng};

fn flying_kind(kind: &str) -> ActorKindServerConfig {
    let mut config = test_kinds::kind(kind);
    config.character.locomotion = ActorLocomotion::Flying;

    config
}

fn home(physics: CharacterPhysicsConfig, world: &CollisionWorld) -> AirHome {
    let grid = CarrierGrid::new(
        CarrierId::WORLD,
        geometry(2, 2),
        vec![LevelGrid {
            cells: CellGrid::new(2, 2),
            edges: EdgeGrid::new(2, 2),
            barrier_edges: EdgeGrid::new(2, 2),
        }],
    );
    let zone = ActorSpawnZone {
        carrier: CarrierId::WORLD,
        level: 0,
        levels: 1,
        roam_distance: 0.0,
        cols: [0, 2],
        rows: [0, 2],
        kind: CONTACT.into(),
        count: 1,
        respawn_secs: None,
        switch: None,
        switch_inverted: false,
    };
    let mut home = AirHome::new(&zone, &grid, physics, 2.0, CarrierPose::IDENTITY, &[]);
    home.advance(world, physics, &mut 20000);
    assert!(home.ready());
    home
}

fn aware(pos: Position) -> AwarePlayer {
    AwarePlayer {
        id: PlayerId(1),
        pos,
        visible: true,
        support: CharacterSupport::Airborne,
        forget_remaining_secs: 1.0,
    }
}

#[test]
fn flyers_pursue_airborne_targets_outside_home_then_return_when_forgotten() {
    let kind = flying_kind(CONTACT);
    let world = CollisionWorld::from_map_layout(&MapLayout::default());
    let home = home(kind.character.physics(), &world);
    let mut info = ActorInfo::new(Entity::from_bits(1), 0, CONTACT.into(), CarrierId::WORLD);
    let mut flight = FlightState::default();
    let mut rng = StdRng::seed_from_u64(3);
    let mut context = BeamContext {
        tick: 0,
        world_pos: Position::default(),
        kind_config: &kind,
        player_physics: test_kinds::physics(CONTACT),
        collision_world: &world,
        open_barriers: &[],
    };
    info.awareness.push(aware(Vec3::new(100.0, 25.0, -40.0).into()));
    decide_flight(
        &mut info,
        &mut flight,
        &context,
        &home,
        CarrierPose::IDENTITY,
        true,
        &mut rng,
    );
    assert_eq!(flight.task, Some(FlightTask::Pursue(PlayerId(1))));
    assert!(flight.search.is_some());
    context.world_pos = Vec3::new(80.0, 24.0, -35.0).into();
    info.awareness.clear();
    decide_flight(
        &mut info,
        &mut flight,
        &context,
        &home,
        CarrierPose::IDENTITY,
        true,
        &mut rng,
    );
    assert_eq!(info.mode, ActorMode::ReturnHome);
    assert_eq!(flight.task, Some(FlightTask::Return));
    let moved = CarrierPose::from_translation(Vec3::new(100.0, 20.0, 0.0));
    let search = flight.search.as_mut().expect("return search missing");
    let result = search.advance_to_goal(
        &world,
        kind.character.physics(),
        &[],
        &mut 20000,
        |_, _| true,
        |p| home.contains(p, CarrierPose::IDENTITY),
    );
    let SearchResult::Found(path) = result else {
        panic!("return route missing");
    };
    assert!(home.contains(
        Vec3::from(*path.back().expect("return endpoint missing")),
        CarrierPose::IDENTITY
    ));
    assert!(!home.contains(Vec3::from(context.world_pos), moved));
}

#[test]
fn flying_beam_actor_holds_position_during_burst_and_evades_in_three_dimensions() {
    let kind = flying_kind(BEAM);
    let world = CollisionWorld::from_map_layout(&MapLayout::default());
    let home = home(kind.character.physics(), &world);
    let mut info = ActorInfo::new(Entity::from_bits(1), 0, BEAM.into(), CarrierId::WORLD);
    let mut flight = FlightState::default();
    let mut rng = StdRng::seed_from_u64(8);
    let context = BeamContext {
        tick: 10,
        world_pos: Position::default(),
        kind_config: &kind,
        player_physics: test_kinds::physics(CONTACT),
        collision_world: &world,
        open_barriers: &[],
    };
    info.awareness.push(aware(Vec3::new(8.0, 4.0, 0.0).into()));
    flight.route.push_back(Vec3::Y.into());
    decide_flight(
        &mut info,
        &mut flight,
        &context,
        &home,
        CarrierPose::IDENTITY,
        true,
        &mut rng,
    );
    assert!(matches!(info.beam, BeamState::Firing { .. }));
    assert!(flight.route.is_empty());
    info.beam = BeamState::Cooldown { remaining_secs: 5.0 };
    decide_flight(
        &mut info,
        &mut flight,
        &context,
        &home,
        CarrierPose::IDENTITY,
        true,
        &mut rng,
    );
    assert!(matches!(info.mode, ActorMode::Evade { .. }));
    assert_eq!(flight.task, Some(FlightTask::Evade));
}

#[test]
fn fleeing_flyer_retreats_when_selected_cover_is_below_a_solid_floor() {
    let kind = flying_kind(BEAM);
    let world = CollisionWorld::from_map_layout(&MapLayout {
        floors: vec![Floor {
            x1: -30.0,
            z1: -30.0,
            x2: 30.0,
            z2: 30.0,
            y: 0.0,
            thickness: 0.4,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    });
    let physics = kind.character.physics();
    let home = home(physics, &world);
    let start = Position::from(Vec3::Y);
    let target = Position::from(Vec3::new(-2.0, -3.0, 0.0));
    let threat = Position::from(Vec3::new(4.0, 1.0, 0.0));
    let mut info = ActorInfo::new(Entity::from_bits(1), 0, BEAM.into(), CarrierId::WORLD);
    info.beam = BeamState::Cooldown { remaining_secs: 5.0 };
    info.mode = ActorMode::Evade { fleeing: false };
    info.awareness.push(aware(threat));
    let mut flight = FlightState::default();
    request_route(&mut flight, FlightTask::Evade, start, target, home.spacing);
    let context = BeamContext {
        tick: 0,
        world_pos: start,
        kind_config: &kind,
        player_physics: test_kinds::physics(CONTACT),
        collision_world: &world,
        open_barriers: &[],
    };
    assert!(covered(Vec3::from(target), &[threat], &context));
    assert!(!world.character_overlaps_solid(&target, physics, &[]));
    let mut rng = StdRng::seed_from_u64(8);
    for _ in 0..6 {
        decide_flight(
            &mut info,
            &mut flight,
            &context,
            &home,
            CarrierPose::IDENTITY,
            true,
            &mut rng,
        );
        advance_search(&mut info, &mut flight, &context, &home, CarrierPose::IDENTITY, &mut 64);
        if !flight.route.is_empty() {
            break;
        }
    }
    assert!(
        !flight.route.is_empty(),
        "flyer stalled searching for cover beneath the floor"
    );
    let end = *flight.route.back().expect("escape endpoint missing");
    assert!(end.distance_sq(&threat) > start.distance_sq(&threat));
    assert!(matches!(info.mode, ActorMode::Evade { fleeing: true }));
    let mut pos = start;
    for point in flight.route {
        assert!(point.y > 0.0);
        assert!(world.character_flight_path_clear(pos, point, physics, &[]));
        let step = world.move_flying_character(
            pos,
            Vec3::from(point) - Vec3::from(pos),
            0.1,
            physics,
            &[],
            &Carriers::default(),
        );
        assert!(step.position.distance_sq(&point) < 0.0001);
        pos = step.position;
    }
}

#[test]
fn pursuing_flyer_keeps_its_live_route_while_replacement_search_is_pending() {
    let start = Position::default();
    let old = Vec3::new(3.0, 0.0, 0.0).into();
    let mut flight = FlightState {
        task: Some(FlightTask::Pursue(PlayerId(1))),
        ..Default::default()
    };
    flight.route.push_back(old);
    request_route(
        &mut flight,
        FlightTask::Pursue(PlayerId(1)),
        start,
        Vec3::new(8.0, 3.0, 0.0).into(),
        0.5,
    );
    assert_eq!(flight.route.front(), Some(&old));
    assert!(flight.search.is_some());
}

#[test]
fn forgotten_pursuit_is_canceled_before_spending_search_work() {
    let kind = flying_kind(CONTACT);
    let world = CollisionWorld::from_map_layout(&MapLayout::default());
    let home = home(kind.character.physics(), &world);
    let mut info = ActorInfo::new(Entity::from_bits(1), 0, CONTACT.into(), CarrierId::WORLD);
    let mut flight = FlightState::default();
    request_route(
        &mut flight,
        FlightTask::Pursue(PlayerId(1)),
        Position::default(),
        Vec3::X.into(),
        home.spacing,
    );
    let context = BeamContext {
        tick: 0,
        world_pos: Position::default(),
        kind_config: &kind,
        player_physics: test_kinds::physics(CONTACT),
        collision_world: &world,
        open_barriers: &[],
    };
    let mut budget = 7;
    advance_search(
        &mut info,
        &mut flight,
        &context,
        &home,
        CarrierPose::IDENTITY,
        &mut budget,
    );
    assert!(flight.search.is_none());
    assert_eq!(budget, 7);
}
