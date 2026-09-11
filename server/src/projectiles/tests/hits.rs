use crate::config::fixtures;
use bevy::prelude::*;
use common::protocol::*;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use super::{PendingProjectileHits, handle_projectile_shot_message, hits::projectile_hits_system};
use crate::{
    actors::{ActorInfo, ActorMap},
    combat::PendingExplosions,
    config::ServerGameplayConfig,
    players::{Invincibility, PlayerInfo, PlayerMap},
    quests::{QuestBoard, QuestCatalog},
};

fn app() -> App {
    let mut app = App::new();
    let mut config = fixtures::server_config();
    config.combat.damage.projectile = 10.0;
    let catalog = QuestCatalog::from_quests(&[]);
    app.insert_resource(QuestBoard::from_catalog(&catalog, None))
        .insert_resource(catalog)
        .insert_resource(config)
        .insert_resource(Invincibility(false))
        .init_resource::<PlayerMap>()
        .init_resource::<ActorMap>()
        .init_resource::<PendingProjectileHits>()
        .init_resource::<PendingExplosions>()
        .add_systems(Update, projectile_hits_system);
    app
}

fn player(app: &mut App, id: PlayerId) -> (Entity, UnboundedReceiver<ServerMessage>) {
    let entity = app
        .world_mut()
        .spawn((
            PlayerMarker,
            Position {
                x: 500.0,
                y: 10.0,
                z: 0.0,
            },
            Health(25.0),
        ))
        .id();
    let (sender, receiver) = unbounded_channel();
    let mut info = PlayerInfo::new(entity, sender);
    info.connection.logged_in = true;
    info.session.generation = PlayerGeneration(4);
    app.world_mut().resource_mut::<PlayerMap>().insert(id, info);
    (entity, receiver)
}

fn hit(app: &mut App, target: HitTarget) {
    app.world_mut().resource_mut::<PendingProjectileHits>().push(
        PlayerId(1),
        CProjectileHit {
            target,
            direction: [1.0, 0.0],
        },
    );
}

#[test]
fn lost_volley_and_shooter_death_or_respawn_do_not_cancel_hits() {
    for respawned in [false, true] {
        let mut app = app();
        let (body, _owner) = player(&mut app, PlayerId(1));
        let (victim, mut receiver) = player(&mut app, PlayerId(2));
        let mut players = app.world_mut().resource_mut::<PlayerMap>();
        let shooter = players.get_mut(&PlayerId(1)).expect("shooter missing");
        shooter.begin_respawn(5.0);
        if respawned {
            shooter.finish_respawn(body);
        }
        let target = HitTarget::Player {
            id: PlayerId(2),
            generation: PlayerGeneration(4),
        };
        hit(&mut app, target);
        app.update();
        assert_eq!(
            *app.world().get::<Health>(victim).expect("victim missing"),
            Health(15.0)
        );
        assert!(
            std::iter::from_fn(|| receiver.try_recv().ok())
                .any(|message| matches!(message, ServerMessage::PlayerHit(hit) if hit.health == Health(15.0)))
        );
        app.update();
        assert_eq!(
            *app.world().get::<Health>(victim).expect("victim missing"),
            Health(15.0)
        );
    }
}

#[test]
fn stale_victim_generation_and_disconnected_shooter_cannot_damage_a_body() {
    let mut app = app();
    let (_owner, _rx) = player(&mut app, PlayerId(1));
    let (victim, _rx) = player(&mut app, PlayerId(2));
    hit(
        &mut app,
        HitTarget::Player {
            id: PlayerId(2),
            generation: PlayerGeneration(3),
        },
    );
    app.update();
    assert_eq!(
        *app.world().get::<Health>(victim).expect("victim missing"),
        Health(25.0)
    );
    hit(
        &mut app,
        HitTarget::Player {
            id: PlayerId(2),
            generation: PlayerGeneration(4),
        },
    );
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .disconnect(&PlayerId(1), 5.0);
    app.update();
    assert_eq!(
        *app.world().get::<Health>(victim).expect("victim missing"),
        Health(25.0)
    );
}

#[test]
fn multishot_pellets_award_actor_damage_and_one_kill_without_server_flight_entities() {
    let mut app = app();
    let (_owner, _rx) = player(&mut app, PlayerId(1));
    let entity = app
        .world_mut()
        .spawn((ActorMarker, Position::default(), Health(25.0)))
        .id();
    app.world_mut().resource_mut::<ActorMap>().insert(
        ActorId(7),
        ActorInfo::new(entity, 0, "bruiser".into(), CarrierId::WORLD),
    );
    for _ in 0..4 {
        hit(&mut app, HitTarget::Actor(ActorId(7)));
    }
    app.update();
    let config = app.world().resource::<ServerGameplayConfig>();
    assert_eq!(
        app.world()
            .resource::<PlayerMap>()
            .get(&PlayerId(1))
            .expect("shooter missing")
            .session
            .score,
        3 * config.scoring.actor_hit["bruiser"] + config.scoring.actor_kill["bruiser"]
    );
    assert_eq!(*app.world().get::<Health>(entity).expect("actor missing"), Health(0.0));
    assert_eq!(
        app.world()
            .resource::<ActorMap>()
            .get(&ActorId(7))
            .expect("actor missing")
            .last_damager,
        Some(PlayerId(1))
    );
}

#[test]
fn cosmetic_volley_relays_its_origin_and_numeric_pattern_while_shooter_is_dead() {
    let mut app = app();
    let (_owner, mut owner_receiver) = player(&mut app, PlayerId(1));
    let (_observer, mut receiver) = player(&mut app, PlayerId(2));
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .get_mut(&PlayerId(1))
        .expect("shooter missing")
        .begin_respawn(5.0);
    let shot = CProjectileShot {
        origin: Position {
            x: -43.0,
            y: 7.0,
            z: 21.0,
        },
        face_yaw: 0.5,
        face_pitch: -0.2,
        pattern: 1,
    };
    handle_projectile_shot_message(
        PlayerId(1),
        shot,
        app.world().resource::<PlayerMap>(),
        &app.world().resource::<ServerGameplayConfig>().weapons.projectiles,
    );
    let ServerMessage::ProjectileShot(relay) = receiver.try_recv().expect("volley relay missing") else {
        panic!("unexpected relay")
    };
    assert_eq!(relay.id, PlayerId(1));
    assert_eq!(relay.shot.origin, shot.origin);
    assert_eq!(relay.shot.pattern, shot.pattern);
    assert!(owner_receiver.try_recv().is_err());
    assert_eq!(ServerMessage::ProjectileShot(relay).lane(), Lane::Unreliable);
    assert_eq!(ClientMessage::ProjectileShot(shot).lane(), Lane::Unreliable);
}

#[test]
fn malformed_or_unknown_pattern_volleys_are_not_relayed() {
    let mut app = app();
    let (_owner, _) = player(&mut app, PlayerId(1));
    let (_observer, mut receiver) = player(&mut app, PlayerId(2));
    let valid = CProjectileShot {
        origin: Position { x: 1.0, y: 2.0, z: 3.0 },
        face_yaw: 0.5,
        face_pitch: -0.2,
        pattern: 0,
    };
    let invalid = [
        CProjectileShot {
            origin: Position {
                x: f32::NAN,
                ..valid.origin
            },
            ..valid
        },
        CProjectileShot {
            face_yaw: f32::INFINITY,
            ..valid
        },
        CProjectileShot {
            face_pitch: f32::NAN,
            ..valid
        },
        CProjectileShot { pattern: 255, ..valid },
    ];
    for shot in invalid {
        handle_projectile_shot_message(
            PlayerId(1),
            shot,
            app.world().resource::<PlayerMap>(),
            &app.world().resource::<ServerGameplayConfig>().weapons.projectiles,
        );
        assert!(receiver.try_recv().is_err(), "{shot:?} was relayed");
    }
    handle_projectile_shot_message(
        PlayerId(1),
        valid,
        app.world().resource::<PlayerMap>(),
        &app.world().resource::<ServerGameplayConfig>().weapons.projectiles,
    );
    assert!(matches!(receiver.try_recv(), Ok(ServerMessage::ProjectileShot(_))));
}
