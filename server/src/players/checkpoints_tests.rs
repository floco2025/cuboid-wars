use bevy::prelude::*;
use common::{
    map::Carriers,
    physics::{CharacterSupport, CollisionWorld},
    protocol::{
        Barrier, BarrierKindId, BarrierKindTable, Carrier, CarrierId, Checkpoint, FaceYaw, Floor, Health, MapLayout,
        PlayerId, Position, ServerMessage,
    },
};

use super::{
    CheckpointId, PlayerMap, PowerUpState, checkpoint_spawn_position, players_checkpoints_system,
    respawn_tests::{add_player, advance, kill, respawn_app},
};
use crate::{
    config::{ActorRespawnScope, PlayerRespawnMode, ServerGameplayConfig},
    map::MapConfig,
    network::ServerToClient,
    schedule::ServerSet,
};

fn checkpoint(min_x: f32) -> Checkpoint {
    Checkpoint {
        carrier: CarrierId::WORLD,
        level: 0,
        min_x,
        max_x: min_x + 4.0,
        min_z: -2.0,
        max_z: 2.0,
        y: 0.0,
    }
}

fn floor(c: Checkpoint) -> Floor {
    Floor {
        x1: c.min_x,
        x2: c.max_x,
        z1: c.min_z,
        z2: c.max_z,
        y: c.y,
        thickness: 0.2,
        level: c.level,
        carrier: c.carrier,
    }
}

fn app(mode: PlayerRespawnMode) -> App {
    let mut app = respawn_app(mode, ActorRespawnScope::Dead);
    let checkpoints = vec![checkpoint(10.0), checkpoint(20.0)];
    let layout = MapLayout {
        floors: checkpoints.iter().copied().map(floor).collect(),
        ..default()
    };
    app.world_mut().resource_mut::<MapConfig>().checkpoints = checkpoints;
    app.insert_resource(CollisionWorld::from_map_layout(&layout, &Default::default()));
    app.add_systems(Update, players_checkpoints_system.in_set(ServerSet::Maintenance));
    app
}

fn stand(app: &mut App, id: PlayerId, pos: Position, support: CharacterSupport) {
    let entity = {
        let mut players = app.world_mut().resource_mut::<PlayerMap>();
        let player = players.get_mut(&id).expect("player missing");
        player.life.fall_state.record_movement(support, false);
        player.entity().expect("player body missing")
    };
    app.world_mut().entity_mut(entity).insert((pos, FaceYaw(0.7)));
}

fn saved(app: &App, id: PlayerId) -> Option<CheckpointId> {
    app.world()
        .resource::<PlayerMap>()
        .get(&id)
        .expect("player missing")
        .session
        .checkpoint
        .map(|c| c.id)
}

#[test]
fn only_grounded_players_activate_and_notifications_do_not_repeat() {
    let mut app = app(PlayerRespawnMode::Individual);
    let id = PlayerId(1);
    let (_, mut receiver) = add_player(&mut app, id);
    add_player(&mut app, PlayerId(2));
    for (y, support) in [
        (1.0, CharacterSupport::Airborne),
        (0.0, CharacterSupport::Airborne),
        (4.0, CharacterSupport::Ground),
        (0.0, CharacterSupport::Ladder),
    ] {
        stand(&mut app, id, Position { x: 12.0, y, z: 0.0 }, support);
        advance(&mut app, 0.0);
        assert!(saved(&app, id).is_none());
    }
    for (x, expected) in [(12.0, 0), (12.0, 0), (22.0, 1), (12.0, 0)] {
        stand(&mut app, id, Position { x, y: 0.0, z: 0.0 }, CharacterSupport::Ground);
        advance(&mut app, 0.0);
        assert_eq!(saved(&app, id), Some(CheckpointId(expected)));
    }
    assert!(saved(&app, PlayerId(2)).is_none());
    let mut notifications = 0;
    while let Ok(message) = receiver.try_recv() {
        if matches!(message, ServerToClient::Send(ServerMessage::CheckpointReached(_))) {
            notifications += 1;
        }
    }
    assert_eq!(notifications, 3);
    let delay = app.world().resource::<ServerGameplayConfig>().player.respawn_secs;
    app.world_mut().resource_mut::<PlayerMap>().disconnect(&id, delay);
    add_player(&mut app, id);
    assert!(saved(&app, id).is_none());
}

#[test]
fn deaths_preserve_checkpoints_clear_equipment_and_retry_blocked_group_or_individual_respawns() {
    for mode in [PlayerRespawnMode::Individual, PlayerRespawnMode::Group] {
        let mut app = app(mode);
        let id = PlayerId(1);
        let (_, mut receiver) = add_player(&mut app, id);
        stand(
            &mut app,
            id,
            Position {
                x: 12.0,
                y: 0.0,
                z: 0.0,
            },
            CharacterSupport::Ground,
        );
        advance(&mut app, 0.0);
        let full_health = app.world().resource::<ServerGameplayConfig>().combat.health.player.max;
        {
            let mut players = app.world_mut().resource_mut::<PlayerMap>();
            let player = players.get_mut(&id).expect("player missing");
            player.life.held_keys = vec![BarrierKindId(0)];
            player.life.power_ups.fill(PowerUpState::Permanent);
        }
        kill(&mut app, id);
        app.insert_resource(CollisionWorld::from_map_layout(
            &MapLayout::default(),
            &Default::default(),
        ));
        advance(&mut app, 2.1);
        assert!(
            app.world()
                .resource::<PlayerMap>()
                .get(&id)
                .expect("player missing")
                .is_dead()
        );
        let layout = MapLayout {
            floors: vec![floor(checkpoint(10.0))],
            ..default()
        };
        app.insert_resource(CollisionWorld::from_map_layout(&layout, &Default::default()));
        advance(&mut app, 0.1);
        let players = app.world().resource::<PlayerMap>();
        let player = players.get(&id).expect("player missing");
        let entity = player.entity().expect("blocked respawn did not retry");
        assert_eq!(player.life.missiles, 0);
        assert!(player.life.held_keys.is_empty());
        assert!(
            player
                .life
                .power_ups
                .iter()
                .all(|power| matches!(power, PowerUpState::Inactive))
        );
        assert_eq!(
            app.world().get::<Health>(entity).expect("health missing").0,
            full_health
        );
        assert_eq!(app.world().get::<Position>(entity).expect("position missing").x, 12.0);
        assert!((app.world().get::<FaceYaw>(entity).expect("facing missing").0 - 0.7).abs() < 1e-5);
        assert_eq!(saved(&app, id), Some(CheckpointId(0)));
        stand(
            &mut app,
            id,
            Position {
                x: 12.0,
                y: 0.0,
                z: 0.0,
            },
            CharacterSupport::Ground,
        );
        advance(&mut app, 0.0);
        let mut notifications = 0;
        while let Ok(message) = receiver.try_recv() {
            if matches!(message, ServerToClient::Send(ServerMessage::CheckpointReached(_))) {
                notifications += 1;
            }
        }
        assert_eq!(notifications, 1);
    }
}

#[test]
fn a_player_killed_at_a_checkpoint_does_not_activate_it() {
    let mut app = app(PlayerRespawnMode::Individual);
    let id = PlayerId(1);
    add_player(&mut app, id);
    stand(
        &mut app,
        id,
        Position {
            x: 12.0,
            y: 0.0,
            z: 0.0,
        },
        CharacterSupport::Ground,
    );
    kill(&mut app, id);
    advance(&mut app, 0.0);
    assert!(saved(&app, id).is_none());
}

#[test]
fn checkpoint_spawns_follow_carriers_and_avoid_players_and_barriers() {
    let physics = ServerGameplayConfig::load_default()
        .expect("gameplay config rejected")
        .gameplay_config()
        .player
        .physics();
    let mut c = checkpoint(0.0);
    c.carrier = CarrierId(1);
    let mut layout = MapLayout {
        floors: vec![floor(c)],
        carriers: vec![Carrier {
            parent: CarrierId::WORLD,
            level: 0,
            levels: 1,
            from: Position {
                x: 20.0,
                y: 5.0,
                z: 10.0,
            },
            to: Position {
                x: 40.0,
                y: 8.0,
                z: 10.0,
            },
            travel_ticks: 30,
            pause_ticks: 0,
            phase_ticks: 0,
        }],
        ..default()
    };
    let mut carriers = Carriers::from_layout(&layout);
    let pose = carriers.pose(c.carrier);
    let mut world = CollisionWorld::from_map_layout(&layout, &Default::default());
    let center = checkpoint_spawn_position(&c, &pose, &world, &[], physics).expect("clear checkpoint rejected");
    assert_eq!(
        center,
        Position {
            x: 22.0,
            y: 5.0,
            z: 10.0
        }
    );
    let other =
        checkpoint_spawn_position(&c, &pose, &world, &[center], physics).expect("unoccupied checkpoint area rejected");
    assert!(other.horizontal_distance_sq(&center) >= physics.movement_collider.diameter.powi(2));
    carriers.advance(15);
    world.set_carrier_poses(&carriers);
    let moved_pose = carriers.pose(c.carrier);
    let moved = checkpoint_spawn_position(&c, &moved_pose, &world, &[], physics).expect("moving checkpoint rejected");
    assert_eq!(
        moved,
        Position {
            x: 32.0,
            y: 6.5,
            z: 10.0
        }
    );
    layout.barriers.push(Barrier {
        x1: 0.0,
        z1: 0.0,
        x2: 4.0,
        z2: 0.0,
        y: 0.0,
        height: 3.0,
        width: 8.0,
        kind: BarrierKindId(0),
        level: 0,
        levels: 1,
        carrier: c.carrier,
    });
    let kinds = BarrierKindTable::from_ids(vec!["gate".into()]).expect("barrier catalog rejected");
    let world = CollisionWorld::from_map_layout(&layout, &kinds);
    assert!(checkpoint_spawn_position(&c, &pose, &world, &[], physics).is_none());
}
