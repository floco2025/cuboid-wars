use super::*;
use crate::config::fixtures;
use crate::{
    players::{CheckpointId, PlayerCheckpoint, PlayerInfo, PowerUpState, outcomes::Landing},
    test_geometry::geometry,
};
use common::protocol::{
    BarrierKindId, CarrierId, Checkpoint, CheckpointKind, Floor, Lane, MapLayout, PlayerGeneration, PortalMode,
    PowerUpKind,
};
use crossbeam_channel::unbounded;

const TEST_GRAVITY: f32 = 25.0;

#[test]
fn a_crushed_player_dies_at_the_reported_contact() {
    let server = fixtures::server_config();
    let gameplay = server.gameplay_config();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(gameplay)
        .insert_resource(server)
        .insert_resource(MapConfig::for_grid(Vec::new(), geometry(1, 1)))
        .insert_resource(Carriers::default())
        .insert_resource(CollisionWorld::from_map_layout(&MapLayout::default()))
        .init_resource::<MapLayout>()
        .insert_resource(PlayerMap::default())
        .insert_resource(Invincibility(false))
        .init_resource::<ServerTick>()
        .insert_resource(PortalAssignments::new(PortalMode::Both))
        .insert_resource(PendingExplosions::default())
        .add_systems(Update, players_fatal_outcomes_system);
    let id = PlayerId(1);
    let entity = app
        .world_mut()
        .spawn((PlayerMarker, id, Position::default(), Health(100.0)))
        .id();
    let (sender, receiver) = unbounded();
    let mut info = PlayerInfo::new(entity, sender);
    info.connection.logged_in = true;
    let contact = Position { x: 20.0, ..default() };
    info.life.outcomes.crushed = Some(contact);
    app.world_mut().resource_mut::<PlayerMap>().insert(id, info);

    app.update();

    assert!(
        app.world()
            .resource::<PlayerMap>()
            .get(&id)
            .is_some_and(PlayerInfo::is_dead)
    );
    assert!(app.world().get_entity(entity).is_err());
    let death = loop {
        match receiver.try_recv().expect("no death message reached the player") {
            ServerMessage::PlayerDeath(death) => break death,
            _ => continue,
        }
    };
    assert_eq!(death.id, id);
    assert_eq!(death.killer, None);
    assert_eq!(death.pos, contact);
}

#[test]
fn fall_damage_zero_at_safe_distance() {
    assert_eq!(fall_damage_for_distance(4.0, 4.0, 12.0, 100.0), 0.0);
    assert_eq!(fall_damage_for_distance(3.0, 4.0, 12.0, 100.0), 0.0);
}

#[test]
fn invincible_void_rescue_relocates_reliably_and_preserves_equipment() {
    let server = fixtures::server_config();
    let mut app = App::new();
    app.insert_resource(server.gameplay_config())
        .insert_resource(server)
        .insert_resource(MapConfig::for_grid(Vec::new(), geometry(1, 1)))
        .init_resource::<Carriers>()
        .insert_resource(CollisionWorld::from_map_layout(&MapLayout::default()))
        .init_resource::<MapLayout>()
        .init_resource::<PlayerMap>()
        .insert_resource(Invincibility(true))
        .insert_resource(ServerTick(42))
        .insert_resource(PortalAssignments::new(PortalMode::Both))
        .init_resource::<PendingExplosions>()
        .add_systems(Update, players_fatal_outcomes_system);
    let id = PlayerId(1);
    let pos = Position {
        y: CHARACTER_FALL_DEATH_Y - 1.0,
        ..default()
    };
    let entity = app.world_mut().spawn((PlayerMarker, id, pos, Health(37.0))).id();
    let (sender, receiver) = unbounded();
    let mut info = PlayerInfo::new(entity, sender);
    info.connection.logged_in = true;
    info.session.score = 5;
    info.life.missiles = 3;
    info.life.held_keys.push(BarrierKindId(1));
    info.life.power_ups[PowerUpKind::Speed.index()] = PowerUpState::Permanent;
    app.world_mut().resource_mut::<PlayerMap>().insert(id, info);
    app.update();
    assert!(receiver.try_recv().is_err());
    assert_eq!(*app.world().get::<Position>(entity).expect("position missing"), pos);
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .get_mut(&id)
        .expect("player missing")
        .life
        .outcomes
        .fell_out_of_world = true;
    app.update();
    let message @ ServerMessage::PlayerRelocated(_) = receiver.try_recv().expect("relocation missing") else {
        panic!("void rescue did not send a relocation")
    };
    assert_eq!(message.lane(), Lane::Reliable);
    let ServerMessage::PlayerRelocated(relocation) = message else {
        unreachable!()
    };
    assert_eq!(relocation.id, id);
    assert_eq!(relocation.tick, 42);
    assert_eq!(relocation.player.generation, PlayerGeneration(1));
    assert_eq!(relocation.player.health.0, 37.0);
    assert_eq!(relocation.player.score, 5);
    assert_eq!(relocation.player.missiles, 3);
    assert_eq!(relocation.player.held_keys, [BarrierKindId(1)]);
    assert!(relocation.player.power_up(PowerUpKind::Speed));
    assert_eq!(
        *app.world().get::<Position>(entity).expect("position missing"),
        relocation.player.movement.pos
    );
    assert!(receiver.try_recv().is_err());
}

#[test]
fn an_invincible_void_rescue_returns_to_the_saved_checkpoint() {
    let server = fixtures::server_config();
    let checkpoint = Checkpoint {
        kind: CheckpointKind::Individual,
        carrier: CarrierId::WORLD,
        level: 0,
        min_x: 10.0,
        max_x: 14.0,
        min_z: -2.0,
        max_z: 2.0,
        y: 0.0,
    };
    let layout = MapLayout {
        floors: vec![Floor {
            x1: 10.0,
            x2: 14.0,
            z1: -2.0,
            z2: 2.0,
            y: 0.0,
            thickness: 0.2,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        checkpoints: vec![checkpoint],
        ..default()
    };
    let mut app = App::new();
    app.insert_resource(server.gameplay_config())
        .insert_resource(server)
        .insert_resource(MapConfig::for_grid(Vec::new(), geometry(1, 1)))
        .init_resource::<Carriers>()
        .insert_resource(CollisionWorld::from_map_layout(&layout))
        .insert_resource(layout)
        .init_resource::<PlayerMap>()
        .insert_resource(Invincibility(true))
        .init_resource::<ServerTick>()
        .insert_resource(PortalAssignments::new(PortalMode::Both))
        .init_resource::<PendingExplosions>()
        .add_systems(Update, players_fatal_outcomes_system);
    let id = PlayerId(1);
    let pos = Position {
        y: CHARACTER_FALL_DEATH_Y - 1.0,
        ..default()
    };
    let entity = app.world_mut().spawn((PlayerMarker, id, pos, Health(37.0))).id();
    let (sender, _receiver) = unbounded();
    let mut info = PlayerInfo::new(entity, sender);
    info.connection.logged_in = true;
    info.session.checkpoint = Some(PlayerCheckpoint {
        id: CheckpointId(0),
        facing: Vec3::X,
    });
    info.life.outcomes.fell_out_of_world = true;
    app.world_mut().resource_mut::<PlayerMap>().insert(id, info);

    app.update();

    let landed = *app.world().get::<Position>(entity).expect("position missing");
    assert!(
        (10.0..=14.0).contains(&landed.x) && (-2.0..=2.0).contains(&landed.z),
        "rescued to {landed:?}, not the checkpoint"
    );
    let player = app.world().resource::<PlayerMap>();
    let info = player.get(&id).expect("player missing");
    assert_eq!(
        info.life.checkpoint_contact,
        Some(CheckpointId(0)),
        "the landing is no fresh entry"
    );
    assert_eq!(info.session.checkpoint.map(|c| c.id), Some(CheckpointId(0)));
}

#[test]
fn simultaneous_invincible_rescues_take_distinct_spots() {
    let server = fixtures::server_config();
    let checkpoint = Checkpoint {
        kind: CheckpointKind::Individual,
        carrier: CarrierId::WORLD,
        level: 0,
        min_x: 10.0,
        max_x: 14.0,
        min_z: -2.0,
        max_z: 2.0,
        y: 0.0,
    };
    let layout = MapLayout {
        floors: vec![Floor {
            x1: 10.0,
            x2: 14.0,
            z1: -2.0,
            z2: 2.0,
            y: 0.0,
            thickness: 0.2,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        checkpoints: vec![checkpoint],
        ..default()
    };
    let mut app = App::new();
    app.insert_resource(server.gameplay_config())
        .insert_resource(server)
        .insert_resource(MapConfig::for_grid(Vec::new(), geometry(1, 1)))
        .init_resource::<Carriers>()
        .insert_resource(CollisionWorld::from_map_layout(&layout))
        .insert_resource(layout)
        .init_resource::<PlayerMap>()
        .insert_resource(Invincibility(true))
        .init_resource::<ServerTick>()
        .insert_resource(PortalAssignments::new(PortalMode::Both))
        .init_resource::<PendingExplosions>()
        .add_systems(Update, players_fatal_outcomes_system);
    let mut entities = Vec::new();
    for id in [PlayerId(1), PlayerId(2)] {
        let pos = Position {
            x: id.0 as f32,
            y: CHARACTER_FALL_DEATH_Y - 1.0,
            z: 0.0,
        };
        let entity = app.world_mut().spawn((PlayerMarker, id, pos, Health(37.0))).id();
        let (sender, _receiver) = unbounded();
        let mut info = PlayerInfo::new(entity, sender);
        info.connection.logged_in = true;
        info.session.checkpoint = Some(PlayerCheckpoint {
            id: CheckpointId(0),
            facing: Vec3::X,
        });
        info.life.outcomes.fell_out_of_world = true;
        app.world_mut().resource_mut::<PlayerMap>().insert(id, info);
        entities.push(entity);
    }

    app.update();

    let landed: Vec<Position> = entities
        .iter()
        .map(|entity| *app.world().get::<Position>(*entity).expect("position missing"))
        .collect();
    for pos in &landed {
        assert!(
            (10.0..=14.0).contains(&pos.x) && (-2.0..=2.0).contains(&pos.z),
            "rescued to {pos:?}, not the checkpoint"
        );
    }
    let gap = (Vec3::from(landed[0]) - Vec3::from(landed[1])).length();
    let diameter = app
        .world()
        .resource::<GameplayConfig>()
        .player
        .physics()
        .movement_collider
        .diameter;
    assert!(gap >= diameter, "the two rescues overlap: {landed:?}");
}

#[test]
fn landing_damage_uses_impact_speed_and_map_thresholds() {
    for (safe, lethal, drop, low_gravity, max_health, initial_health, expected_health) in [
        (4.0, 12.0, 0.0, false, 100.0, 100.0, 100.0),
        (12.0, 16.0, 12.0, false, 100.0, 100.0, 100.0),
        (8.0, 16.0, 12.0, false, 100.0, 100.0, 50.0),
        (4.0, 12.0, 12.0, false, 100.0, 100.0, 0.0),
        (4.0, 12.0, 20.0, false, 100.0, 100.0, 0.0),
        (4.0, 12.0, 12.0, true, 100.0, 100.0, 75.0),
        (4.0, 12.0, 8.0, true, 100.0, 100.0, 100.0),
        (4.0, 12.0, 8.0, false, 100.0, 40.0, 0.0),
        (4.0, 12.0, 4.0625, false, 100.0, 100.0, 100.0),
        (4.0, 12.0, 4.0625, false, 1000.0, 1000.0, 992.1875),
        (0.0, 8.0, 1.0, false, 8.0, 8.0, 7.0),
        (0.0, 8.0, 100.0, false, 0.5, 0.5, 0.5),
    ] {
        let mut server = fixtures::server_config();
        server.combat.health.player.max = max_health;
        let mut settings = server.maps["hotel"].settings.clone();
        settings.movement.gravity = 2.0;
        settings.movement.low_gravity = 1.0;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(server)
            .insert_resource(settings)
            .insert_resource(FallDamageConfig {
                safe_distance: safe,
                lethal_distance: lethal,
            })
            .insert_resource(PlayerMap::default())
            .insert_resource(Invincibility(false))
            .insert_resource(PendingExplosions::default())
            .add_systems(Update, players_fall_damage_system);
        let id = PlayerId(1);
        let entity = app
            .world_mut()
            .spawn((PlayerMarker, id, Position::default(), Health(initial_health)))
            .id();
        let (sender, receiver) = unbounded();
        let mut info = PlayerInfo::new(entity, sender);
        info.connection.logged_in = true;
        info.life.outcomes.landings.push(Landing {
            pos: Position::default(),
            impact_speed: (2.0_f32 * if low_gravity { 1.0 } else { 2.0 } * drop).sqrt(),
        });
        app.world_mut().resource_mut::<PlayerMap>().insert(id, info);

        app.update();

        let dead = app
            .world()
            .resource::<PlayerMap>()
            .get(&id)
            .expect("player missing")
            .is_dead();
        assert_eq!(dead, expected_health == 0.0);
        if !dead {
            assert!(
                (app.world().get::<Health>(entity).expect("player health missing").0 - expected_health).abs() < 0.001
            );
        }
        app.update();
        if !dead {
            assert!(
                (app.world().get::<Health>(entity).expect("player health missing").0 - expected_health).abs() < 0.001
            );
        }
        let mut impacts = Vec::new();
        while let Ok(message) = receiver.try_recv() {
            match message {
                ServerMessage::PlayerFallDamage(impact) => {
                    assert_eq!(impact.id, id);
                    impacts.push(Some(impact.health.0));
                }
                ServerMessage::PlayerSoftLanding(impact) => {
                    assert_eq!(impact.id, id);
                    impacts.push(None);
                }
                _ => {}
            }
        }
        if drop == 0.0 {
            assert!(impacts.is_empty());
        } else {
            assert_eq!(impacts.len(), 1, "one landing must emit exactly one sound cue");
            assert_eq!(impacts[0].is_some(), expected_health < initial_health);
            if let Some(health) = impacts[0] {
                assert!((health - expected_health).abs() < 0.001);
            }
        }
    }
}

#[test]
fn fall_damage_lethal_at_lethal_distance() {
    assert_eq!(fall_damage_for_distance(12.0, 4.0, 12.0, 100.0), 100.0);
}

#[test]
fn fall_damage_lerps_midpoint() {
    // (8 - 4) / (12 - 4) = 0.5 → 50 dmg
    assert_eq!(fall_damage_for_distance(8.0, 4.0, 12.0, 100.0), 50.0);
}

#[test]
fn fall_damage_saturates_past_lethal() {
    assert_eq!(fall_damage_for_distance(100.0, 4.0, 12.0, 100.0), 100.0);
}

#[test]
fn impact_energy_determines_the_equivalent_drop() {
    assert_eq!(fall_distance_for_speed(0.0, TEST_GRAVITY), 0.0);
    assert_eq!(fall_distance_for_speed(10.0, TEST_GRAVITY), 2.0);
    assert_eq!(fall_distance_for_speed(20.0, TEST_GRAVITY), 8.0);
    assert_eq!(fall_distance_for_speed(25.0, TEST_GRAVITY), 12.5);
}
