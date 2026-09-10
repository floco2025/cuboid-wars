use bevy::prelude::*;
use common::protocol::*;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use super::{MissileMap, handle_missile_detonated, handle_missile_moves, handle_missile_shot_message};
use crate::{
    combat::PendingExplosions,
    network::ServerToClient,
    players::{PlayerInfo, PlayerMap},
};

fn players() -> (PlayerMap, UnboundedReceiver<ServerToClient>) {
    let (sender, receiver) = unbounded_channel();
    let mut world = World::new();
    let mut info = PlayerInfo::new(world.spawn_empty().id(), sender);
    info.connection.logged_in = true;
    info.session.generation = PlayerGeneration(4);
    info.add_missiles(3, 3);
    let mut players = PlayerMap::default();
    players.insert(PlayerId(1), info);
    (players, receiver)
}

fn shot() -> CMissileShot {
    CMissileShot {
        generation: PlayerGeneration(4),
        target: Some(HomingTarget::Actor(ActorId(7))),
        movement: MissileMovementState::from_velocity(
            Position {
                x: 45.0,
                y: 8.0,
                z: -12.0,
            },
            Vec3::new(3.0, 5.0, -18.0),
        ),
    }
}

#[test]
fn launch_adopts_client_geometry_and_target_without_a_server_body_query() {
    let (mut players, mut receiver) = players();
    let mut missiles = MissileMap::default();
    let shot = shot();
    handle_missile_shot_message(PlayerId(1), shot.clone(), &mut players, &mut missiles, ServerTick(17));
    let ServerToClient::Send(ServerMessage::MissileLaunch(launch)) = receiver.try_recv().expect("launch missing")
    else {
        panic!("unexpected reply");
    };
    assert_eq!(launch.movement, shot.movement);
    assert_eq!(launch.target, shot.target);
    assert_eq!(launch.tick, 17);
    assert_eq!(ServerMessage::MissileLaunch(launch.clone()).lane(), Lane::Reliable);
    assert_eq!(
        missiles.get(&launch.id).expect("missile missing").movement,
        shot.movement
    );
    assert_eq!(players.get(&PlayerId(1)).expect("shooter missing").life.missiles, 2);
}

#[test]
fn stale_body_invalid_geometry_and_empty_ammo_do_not_launch() {
    let (mut players, mut receiver) = players();
    let mut missiles = MissileMap::default();
    let mut stale = shot();
    stale.generation = PlayerGeneration(3);
    let mut invalid = shot();
    invalid.movement.pos.x = f32::NAN;
    for shot in [stale, invalid] {
        handle_missile_shot_message(PlayerId(1), shot, &mut players, &mut missiles, ServerTick(1));
    }
    assert_eq!(players.get(&PlayerId(1)).expect("shooter missing").life.missiles, 3);
    players.get_mut(&PlayerId(1)).expect("shooter missing").life.missiles = 0;
    handle_missile_shot_message(PlayerId(1), shot(), &mut players, &mut missiles, ServerTick(1));
    assert!(missiles.iter().next().is_none());
    assert!(receiver.try_recv().is_err());
}

#[test]
fn moves_relay_only_fresh_owner_samples_across_wrap_and_during_death() {
    let (mut players, mut owner_receiver) = players();
    let (sender, mut receiver) = unbounded_channel();
    let mut observer = PlayerInfo::new(World::new().spawn_empty().id(), sender);
    observer.connection.logged_in = true;
    players.insert(PlayerId(2), observer);
    players
        .get_mut(&PlayerId(1))
        .expect("shooter missing")
        .begin_respawn(5.0);
    let id = MissileId(8);
    let mut missiles = MissileMap::default();
    missiles.insert(
        id,
        Missile {
            shooter: PlayerId(1),
            seq: u32::MAX - 1,
            movement: shot().movement,
        },
    );
    let update = MissileMove {
        id,
        seq: 1,
        movement: MissileMovementState::from_velocity(Position::default(), Vec3::X * 20.0),
    };
    for (shooter, seq) in [
        (PlayerId(2), 1),
        (PlayerId(1), u32::MAX - 2),
        (PlayerId(1), 1),
        (PlayerId(1), 1),
    ] {
        handle_missile_moves(
            shooter,
            CMissileMoves {
                moves: vec![MissileMove { seq, ..update.clone() }],
            },
            &mut missiles,
            &players,
        );
    }
    let ServerToClient::Send(ServerMessage::MissileMoves(message)) =
        receiver.try_recv().expect("movement relay missing")
    else {
        panic!("unexpected reply");
    };
    assert_eq!(message.moves.len(), 1);
    assert_eq!(message.moves[0].movement, update.movement);
    assert_eq!(missiles.get(&id).expect("missile missing").seq, 1);
    assert!(receiver.try_recv().is_err(), "stale or foreign report was relayed");
    assert!(
        owner_receiver.try_recv().is_err(),
        "owner received its own movement relay"
    );
}

#[test]
fn detonation_is_owner_bound_and_applies_once_even_without_any_movement_report() {
    let (mut players, mut receiver) = players();
    let mut missiles = MissileMap::default();
    handle_missile_shot_message(PlayerId(1), shot(), &mut players, &mut missiles, ServerTick(10));
    let id = *missiles.iter().next().expect("missile missing").0;
    let _ = receiver.try_recv();
    players
        .get_mut(&PlayerId(1))
        .expect("shooter missing")
        .begin_respawn(5.0);
    let message = CMissileDetonated {
        id,
        pos: Position {
            x: -75.0,
            y: 1.0,
            z: 0.0,
        },
        hits: vec![MissileBlastHit {
            target: HitTarget::Actor(ActorId(9)),
            falloff: 0.75,
            direction: [1.0, 0.0],
        }],
    };
    let mut pending = PendingExplosions::default();
    for shooter in [PlayerId(2), PlayerId(1), PlayerId(1)] {
        handle_missile_detonated(
            shooter,
            message.clone(),
            &mut missiles,
            &players,
            &mut pending,
            ServerTick(11),
        );
    }
    assert!(missiles.get(&id).is_none());
    assert_eq!(pending.0.len(), 1);
    let ServerToClient::Send(ServerMessage::MissileDetonated(detonation)) =
        receiver.try_recv().expect("detonation missing")
    else {
        panic!("unexpected reply");
    };
    assert_eq!(detonation.pos, message.pos);
    assert_eq!(ServerMessage::MissileDetonated(detonation).lane(), Lane::Reliable);
    assert!(receiver.try_recv().is_err());
    handle_missile_moves(
        PlayerId(1),
        CMissileMoves {
            moves: vec![MissileMove {
                id,
                seq: 12,
                movement: shot().movement,
            }],
        },
        &mut missiles,
        &players,
    );
    assert!(missiles.get(&id).is_none());
    assert!(receiver.try_recv().is_err());
}

#[test]
fn disconnect_removes_only_that_shooters_flights() {
    let mut missiles = MissileMap::default();
    for id in 1..=2 {
        missiles.insert(
            MissileId(id),
            Missile {
                shooter: PlayerId(id),
                seq: 0,
                movement: shot().movement,
            },
        );
    }
    missiles.remove_shooter(PlayerId(1));
    assert!(missiles.get(&MissileId(1)).is_none());
    assert!(missiles.get(&MissileId(2)).is_some());
}
