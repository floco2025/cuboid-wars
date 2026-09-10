use bevy::prelude::*;
use common::{
    config::{GameplayConfig, NetworkConfig},
    protocol::*,
};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use super::{MissileMap, expiry::missiles_expiry_system, handle_missile_moves};
use crate::{
    config::ServerGameplayConfig,
    network::ServerToClient,
    players::{PlayerInfo, PlayerMap},
    schedule::ticks_from_secs,
};

fn app() -> (App, UnboundedReceiver<ServerToClient>) {
    let server = ServerGameplayConfig::load_default().expect("server gameplay config missing");
    let mut app = App::new();
    app.insert_resource(server.gameplay_config())
        .insert_resource(NetworkConfig::default())
        .init_resource::<ServerTick>()
        .init_resource::<MissileMap>()
        .init_resource::<PlayerMap>()
        .add_systems(Update, missiles_expiry_system);
    let (sender, receiver) = unbounded_channel();
    let mut info = PlayerInfo::new(app.world_mut().spawn_empty().id(), sender);
    info.connection.logged_in = true;
    app.world_mut().resource_mut::<PlayerMap>().insert(PlayerId(1), info);
    (app, receiver)
}

fn missile(pos: Position) -> Missile {
    Missile {
        shooter: PlayerId(1),
        seq: 0,
        movement: MissileMovementState::from_velocity(pos, Vec3::X * 16.0),
    }
}

#[test]
fn an_unreported_missile_is_removed_after_its_lifetime_with_a_cue_at_its_last_position() {
    let (mut app, mut receiver) = app();
    let lifetime_ticks = {
        let gameplay = app.world().resource::<GameplayConfig>();
        ticks_from_secs(gameplay.missiles.lifetime_secs, 30)
    };
    let launched = Position { x: 1.0, ..default() };
    let reported = Position { x: 40.0, ..default() };
    app.world_mut()
        .resource_mut::<MissileMap>()
        .insert(MissileId(3), missile(launched), 10);
    app.world_mut().resource_scope(|_, mut missiles: Mut<MissileMap>| {
        let players = PlayerMap::default();
        handle_missile_moves(
            PlayerId(1),
            CMissileMoves {
                moves: vec![MissileMove {
                    id: MissileId(3),
                    seq: 5,
                    movement: missile(reported).movement,
                }],
            },
            &mut missiles,
            &players,
        );
    });
    app.world_mut().resource_mut::<ServerTick>().0 = 10 + lifetime_ticks + 30;
    app.update();
    assert!(app.world().resource::<MissileMap>().get(&MissileId(3)).is_some());
    assert!(receiver.try_recv().is_err());

    app.world_mut().resource_mut::<ServerTick>().0 = 10 + lifetime_ticks + 61;
    app.update();
    assert!(app.world().resource::<MissileMap>().get(&MissileId(3)).is_none());
    let ServerToClient::Send(ServerMessage::MissileDetonated(cue)) = receiver.try_recv().expect("expiry cue missing")
    else {
        panic!("unexpected message");
    };
    assert_eq!(cue.id, MissileId(3));
    assert_eq!(cue.pos, reported);
    assert_eq!(cue.tick, 10 + lifetime_ticks + 61);
    app.update();
    assert!(receiver.try_recv().is_err());
}

#[test]
fn launch_age_survives_tick_wrap() {
    let (mut app, mut receiver) = app();
    app.world_mut()
        .resource_mut::<MissileMap>()
        .insert(MissileId(1), missile(Position::default()), u32::MAX - 5);
    app.world_mut().resource_mut::<ServerTick>().0 = 20;
    app.update();
    assert!(app.world().resource::<MissileMap>().get(&MissileId(1)).is_some());
    assert!(receiver.try_recv().is_err());
}
