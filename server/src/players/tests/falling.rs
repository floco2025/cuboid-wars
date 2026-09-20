use super::*;
use crate::config::{FallDamageConfig, fixtures};
use crate::{
    map::{CellGrid, EdgeGrid, LevelGrid},
    players::{CheckpointId, PlayerCheckpoint, PlayerInfo, PowerUpState, outcomes::Landing},
    test_geometry::geometry,
};
use common::protocol::{
    CarrierId, Checkpoint, CheckpointKind, FieldId, Floor, Lane, MapLayout, PlayerGeneration, PortalMode, PowerUpKind,
};
use crossbeam_channel::unbounded;

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
    info.life.held_keys.push(FieldId(1));
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
    assert_eq!(relocation.player.held_keys, [FieldId(1)]);
    assert!(relocation.player.power_up(PowerUpKind::Speed));
    assert_eq!(
        *app.world().get::<Position>(entity).expect("position missing"),
        relocation.player.movement.pos
    );
    assert!(receiver.try_recv().is_err());
}

// One checkpoint over four floored cells, with the grid its spawns sample.
fn checkpoint_fixture() -> (Checkpoint, Floor, MapConfig) {
    let geometry = geometry(2, 2);
    let mut cells = CellGrid::new(2, 2);
    for cell in cells.rows.iter_mut().flatten() {
        cell.has_floor = true;
    }
    let checkpoint = Checkpoint {
        kind: CheckpointKind::Individual,
        number: 1,
        carrier: CarrierId::WORLD,
        level: 0,
        cols: [0, 2],
        rows: [0, 2],
        min_x: geometry.cell_to_world_x(0),
        max_x: geometry.cell_to_world_x(2),
        min_z: geometry.cell_to_world_z(0),
        max_z: geometry.cell_to_world_z(2),
        y: 0.0,
    };
    let floor = Floor {
        x1: checkpoint.min_x,
        x2: checkpoint.max_x,
        z1: checkpoint.min_z,
        z2: checkpoint.max_z,
        y: 0.0,
        thickness: 0.2,
        level: 0,
        carrier: CarrierId::WORLD,
    };
    let map_config = MapConfig::for_grid(
        vec![LevelGrid {
            cells,
            edges: EdgeGrid::new(2, 2),
        }],
        geometry,
    );
    (checkpoint, floor, map_config)
}

fn in_checkpoint(checkpoint: &Checkpoint, pos: &Position) -> bool {
    (checkpoint.min_x..=checkpoint.max_x).contains(&pos.x) && (checkpoint.min_z..=checkpoint.max_z).contains(&pos.z)
}

#[test]
fn an_invincible_void_rescue_returns_to_the_saved_checkpoint() {
    let server = fixtures::server_config();
    let (checkpoint, floor, map_config) = checkpoint_fixture();
    let layout = MapLayout {
        floors: vec![floor],
        checkpoints: vec![checkpoint.clone()],
        ..default()
    };
    let mut app = App::new();
    app.insert_resource(server.gameplay_config())
        .insert_resource(server)
        .insert_resource(map_config)
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
    info.session.checkpoint = PlayerCheckpoint::numbered(1);
    info.life.outcomes.fell_out_of_world = true;
    app.world_mut().resource_mut::<PlayerMap>().insert(id, info);

    app.update();

    let landed = *app.world().get::<Position>(entity).expect("position missing");
    assert!(
        in_checkpoint(&checkpoint, &landed),
        "rescued to {landed:?}, not the checkpoint"
    );
    let player = app.world().resource::<PlayerMap>();
    let info = player.get(&id).expect("player missing");
    assert_eq!(
        info.life.checkpoint_contact,
        Some(CheckpointId(0)),
        "the landing is no fresh entry"
    );
    assert_eq!(info.session.checkpoint, PlayerCheckpoint::numbered(1));
}

#[test]
fn simultaneous_invincible_rescues_take_distinct_spots() {
    let server = fixtures::server_config();
    let (checkpoint, floor, map_config) = checkpoint_fixture();
    let layout = MapLayout {
        floors: vec![floor],
        checkpoints: vec![checkpoint.clone()],
        ..default()
    };
    let mut app = App::new();
    app.insert_resource(server.gameplay_config())
        .insert_resource(server)
        .insert_resource(map_config)
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
        info.session.checkpoint = PlayerCheckpoint::numbered(1);
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
            in_checkpoint(&checkpoint, pos),
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
        let mut settings = server.settings.clone();
        settings.movement.gravity = 2.0;
        settings.movement.low_gravity = 1.0;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(server)
            .insert_resource(settings)
            .insert_resource(FallDamageConfigs {
                player: FallDamageConfig {
                    safe_distance: safe,
                    lethal_distance: lethal,
                },
                actor: FallDamageConfig {
                    safe_distance: 0.0,
                    lethal_distance: 1.0,
                },
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
