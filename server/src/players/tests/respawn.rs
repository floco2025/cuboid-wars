use bevy::{ecs::world::CommandQueue, prelude::*};

use crate::{
    actors::{ActorMap, ActorSpawner, PendingActorSpawns},
    combat::{PendingExplosions, kill_actor},
    config::{ActorRespawnScope, PlayerRespawnMode, ServerGameplayConfig},
    map::MapConfig,
    missiles::MissileMap,
    players::{
        PlayerInfo, PlayerMap, enter_group_respawn,
        fixtures::{add_player, advance, disconnect, kill, respawn_app},
    },
};
use common::{
    constants::TICK_DURATION,
    protocol::{
        ActorId, Health, Missile, MissileMovementState, PlayerDeathEffect, PlayerId, Position, QuestId, ServerMessage,
        ServerTick,
    },
};

fn materialize_actors(app: &mut App) {
    let tick = app
        .world()
        .resource::<PendingActorSpawns>()
        .0
        .iter()
        .map(|spawn| spawn.due_tick)
        .min()
        .expect("pending actors missing");
    app.world_mut().resource_mut::<ServerTick>().0 = tick - 1;
    advance(app, TICK_DURATION.as_secs_f32());
}

fn destroy_actor(app: &mut App, id: ActorId) {
    let feed = app.world().resource::<ServerGameplayConfig>().feed.clone();
    app.world_mut().resource_scope(|world, mut actors: Mut<ActorMap>| {
        let entity = actors.get(&id).expect("actor missing").entity;
        let pos = *world.get::<Position>(entity).expect("actor position missing");
        world.resource_scope(|world, mut explosions: Mut<PendingExplosions>| {
            let mut queue = CommandQueue::default();
            assert!(kill_actor(
                &mut Commands::new(&mut queue, world),
                &mut actors,
                world.resource::<PlayerMap>(),
                &mut explosions,
                &feed,
                id,
                entity,
                pos,
                None
            ));
            queue.apply(world);
        });
    });
}

#[test]
fn logout_restores_actors_on_an_empty_server_after_the_remaining_delay_and_beam_in() {
    for died_first in [false, true] {
        let mut app = respawn_app(PlayerRespawnMode::Individual, ActorRespawnScope::All);
        materialize_actors(&mut app);
        let player = PlayerId(1);
        let (entity, _rx) = add_player(&mut app, player);

        if died_first {
            kill(&mut app, player);
            advance(&mut app, 1.0);
        }
        disconnect(&mut app, player);
        assert!(app.world().get_entity(entity).is_err());
        assert!(!app.world().resource::<PlayerMap>().has_active_players());
        let actor = *app
            .world()
            .resource::<ActorMap>()
            .iter()
            .next()
            .expect("actor missing")
            .0;
        advance(&mut app, 0.5);
        destroy_actor(&mut app, actor);
        let blasts = app.world().resource::<PendingExplosions>().0.len();
        assert_eq!(blasts, if died_first { 2 } else { 1 });
        advance(&mut app, if died_first { 0.5 } else { 1.5 });
        assert_eq!(app.world().resource::<ActorMap>().values().count(), 0);
        let pending = &app.world().resource::<PendingActorSpawns>().0;
        assert_eq!(pending.len(), 3);
        assert!(pending.iter().all(|spawn| spawn.due_tick - spawn.reserved_tick == 90));
        materialize_actors(&mut app);
        assert_eq!(app.world().resource::<ActorMap>().values().count(), 3);
        advance(&mut app, 2.0);
        assert!(app.world().resource::<PendingActorSpawns>().0.is_empty());
        assert_eq!(app.world().resource::<ActorSpawner>().next_id, 6);
        assert_eq!(app.world().resource::<PendingExplosions>().0.len(), blasts);
        assert!(!app.world().resource::<PlayerMap>().has_active_players());
    }
}

#[test]
fn an_actor_killed_during_the_player_countdown_returns_after_beam_in_without_clearing_shots() {
    let mut app = respawn_app(PlayerRespawnMode::Individual, ActorRespawnScope::All);
    materialize_actors(&mut app);
    let victim = PlayerId(1);
    let (_, _rx) = add_player(&mut app, victim);
    let actor_id = *app
        .world()
        .resource::<ActorMap>()
        .iter()
        .next()
        .expect("actor missing")
        .0;

    let mut missiles = app.world_mut().resource_mut::<MissileMap>();
    let missile_id = missiles.allocate();
    missiles.insert(
        missile_id,
        Missile {
            shooter: victim,
            seq: 0,
            movement: MissileMovementState::from_velocity(Position::default(), Vec3::X),
        },
        0,
    );

    kill(&mut app, victim);
    advance(&mut app, 1.0);
    destroy_actor(&mut app, actor_id);
    advance(&mut app, 0.5);
    assert_eq!(app.world().resource::<ActorMap>().values().count(), 2);
    assert!(app.world().resource::<PendingActorSpawns>().0.is_empty());
    advance(&mut app, 0.5);

    assert!(
        !app.world()
            .resource::<PlayerMap>()
            .get(&victim)
            .expect("player missing")
            .is_dead()
    );
    assert_eq!(app.world().resource::<ActorMap>().values().count(), 0);
    let pending = &app.world().resource::<PendingActorSpawns>().0;
    assert_eq!(pending.len(), 3);
    assert!(
        pending
            .iter()
            .all(|spawn| spawn.actor_id != actor_id && spawn.due_tick - spawn.reserved_tick == 90)
    );
    advance(&mut app, 0.0);
    assert_eq!(app.world().resource::<ActorMap>().values().count(), 0);
    materialize_actors(&mut app);
    let actors = app.world().resource::<ActorMap>();
    assert_eq!(actors.values().count(), 3);
    let max_health = app
        .world()
        .resource::<ServerGameplayConfig>()
        .combat
        .health
        .expect_actor("turret")
        .max;
    for actor in actors.values() {
        assert_eq!(app.world().get::<Health>(actor.entity), Some(&Health(max_health)));
        assert_eq!(
            *app.world()
                .get::<Position>(actor.entity)
                .expect("actor position missing"),
            actor.anchor.expect("turret anchor missing").pos
        );
    }

    assert!(app.world().resource::<MissileMap>().get(&missile_id).is_some());
    assert_eq!(app.world().resource::<PendingExplosions>().0.len(), 2);
}

#[test]
fn actor_reset_scopes_preserve_or_replace_survivors_and_pending_spawns() {
    for scope in [ActorRespawnScope::Dead, ActorRespawnScope::All] {
        let mut app = respawn_app(PlayerRespawnMode::Individual, scope);
        for zone in &mut app.world_mut().resource_mut::<MapConfig>().actor_spawn_zones {
            zone.respawn_secs = Some(180.0);
        }
        let pending_id = {
            let mut pending = app.world_mut().resource_mut::<PendingActorSpawns>();
            pending.0[2].due_tick += 300;
            pending.0[2].actor_id
        };
        materialize_actors(&mut app);
        let (_, _rx) = add_player(&mut app, PlayerId(1));
        let mut actors: Vec<_> = app
            .world()
            .resource::<ActorMap>()
            .iter()
            .map(|(id, info)| (*id, info.entity))
            .collect();
        actors.sort_by_key(|(id, _)| id.0);
        let (survivor, survivor_entity) = actors[1];
        let displaced = Position {
            x: 50.0,
            y: 0.0,
            z: 0.0,
        };
        app.world_mut()
            .entity_mut(survivor_entity)
            .insert((Health(1.0), displaced));
        destroy_actor(&mut app, actors[0].0);
        kill(&mut app, PlayerId(1));
        advance(&mut app, 2.0);
        let pending = &app.world().resource::<PendingActorSpawns>().0;
        if scope == ActorRespawnScope::Dead {
            assert_eq!(app.world().resource::<ActorMap>().values().count(), 1);
            assert_eq!(app.world().get::<Health>(survivor_entity), Some(&Health(1.0)));
            assert_eq!(app.world().get::<Position>(survivor_entity), Some(&displaced));
            assert_eq!(pending.len(), 2);
            assert!(pending.iter().any(|spawn| spawn.actor_id == pending_id));
        } else {
            assert!(app.world().resource::<ActorMap>().get(&survivor).is_none());
            assert!(app.world().get_entity(survivor_entity).is_err());
            assert_eq!(pending.len(), 3);
            assert!(pending.iter().all(|spawn| spawn.actor_id != pending_id));
        }
        assert_eq!(
            app.world().resource::<ActorSpawner>().next_id,
            if scope == ActorRespawnScope::Dead { 4 } else { 6 }
        );
        assert_eq!(app.world().resource::<PendingExplosions>().0.len(), 2);
    }
}

#[test]
fn a_group_death_resets_teammates_once_and_respawns_everyone_together() {
    let mut app = respawn_app(PlayerRespawnMode::Group, ActorRespawnScope::All);
    materialize_actors(&mut app);
    let (first, rx) = add_player(&mut app, PlayerId(1));
    let (second, _rx) = add_player(&mut app, PlayerId(2));
    kill(&mut app, PlayerId(1));
    advance(&mut app, 1.0);
    assert!(app.world().resource::<PlayerMap>().values().all(PlayerInfo::is_dead));
    assert!(app.world().get_entity(first).is_err());
    assert!(app.world().get_entity(second).is_err());
    assert_eq!(app.world().resource::<PendingExplosions>().0.len(), 1);
    let player_death = app.world().resource::<ServerGameplayConfig>().scoring.player_death;
    let mut effects = Vec::new();
    let mut feed_count = 0;
    while let Ok(message) = rx.try_recv() {
        match message {
            ServerMessage::PlayerDeath(death) => {
                assert_eq!(death.killer, None);
                // Only the player who died pays for it; a teammate pulled into the group respawn does not.
                let charged = if death.id == PlayerId(1) { player_death } else { 0 };
                assert_eq!(death.victim_score, 42 + charged);
                effects.push((death.id, death.effect));
            }
            ServerMessage::Feed(_) => feed_count += 1,
            _ => {}
        }
    }
    assert!(matches!(
        effects.as_slice(),
        [
            (PlayerId(1), PlayerDeathEffect::Explosion { .. }),
            (PlayerId(2), PlayerDeathEffect::GroupRespawn)
        ]
    ));
    assert_eq!(feed_count, 1);
    assert!(
        !app.world_mut()
            .resource_mut::<PlayerMap>()
            .begin_respawn(PlayerId(2), 20.0)
    );
    advance(&mut app, 1.0);
    let mut relocated: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok())
        .filter_map(|message| match message {
            ServerMessage::PlayerRelocated(relocation) => Some(relocation),
            _ => None,
        })
        .collect();
    relocated.sort_by_key(|relocation| relocation.id.0);
    assert_eq!(
        relocated
            .iter()
            .map(|relocation| (relocation.id, relocation.player.generation.0))
            .collect::<Vec<_>>(),
        [(PlayerId(1), 1), (PlayerId(2), 1)]
    );
    let players = app.world().resource::<PlayerMap>();
    let max_health = app.world().resource::<ServerGameplayConfig>().combat.health.player.max;
    assert!(!players.group_respawn_active());
    for relocation in &relocated {
        let info = players.get(&relocation.id).expect("relocated player missing");
        assert_eq!(relocation.player.movement.pos, info.life.movement.pos);
        assert_eq!(relocation.player.movement.face_yaw, info.life.movement.face_yaw);
        assert_eq!(relocation.player.health, Health(max_health));
        assert_eq!(
            app.world()
                .get::<Position>(info.entity().expect("respawned entity missing")),
            Some(&relocation.player.movement.pos)
        );
    }
    for (id, info) in players.iter() {
        assert!(!info.is_dead());
        let charged = if *id == PlayerId(1) { player_death } else { 0 };
        assert_eq!(info.session.score, 42 + charged);
        assert_eq!(
            info.session.quest_states[&QuestId("progress".into())].own_progress(),
            Some(3)
        );
        assert_eq!(info.life.missiles, 0);
        assert_eq!(
            app.world()
                .get::<Health>(info.entity().expect("respawned entity missing")),
            Some(&Health(max_health))
        );
    }
    assert_eq!(app.world().resource::<PendingActorSpawns>().0.len(), 3);
    assert_eq!(app.world().resource::<ActorSpawner>().next_id, 6);
    advance(&mut app, 0.5);
    assert_eq!(app.world().resource::<ActorSpawner>().next_id, 6);
}

#[test]
fn group_joiners_share_the_remaining_countdown_after_the_triggering_player_disconnects() {
    let mut app = respawn_app(PlayerRespawnMode::Group, ActorRespawnScope::All);
    let (_, _rx) = add_player(&mut app, PlayerId(1));
    let (_, _rx) = add_player(&mut app, PlayerId(2));
    kill(&mut app, PlayerId(1));
    advance(&mut app, 1.0);
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .disconnect(&PlayerId(1), 2.0);
    let (entity, _rx) = add_player(&mut app, PlayerId(3));
    app.world_mut().resource_scope(|world, mut players: Mut<PlayerMap>| {
        let mut queue = CommandQueue::default();
        let pos = *world.get::<Position>(entity).expect("joining player position missing");
        assert!(enter_group_respawn(
            &mut Commands::new(&mut queue, world),
            &mut players,
            PlayerId(3),
            pos
        ));
        queue.apply(world);
    });
    assert!(
        app.world()
            .resource::<PlayerMap>()
            .get(&PlayerId(3))
            .expect("joiner missing")
            .is_dead()
    );
    advance(&mut app, 1.0);
    assert_eq!(app.world().resource::<PlayerMap>().values().count(), 2);
    assert!(app.world().resource::<PlayerMap>().values().all(|info| !info.is_dead()));
    assert_eq!(app.world().resource::<PendingActorSpawns>().0.len(), 3);
}

#[test]
fn reset_refills_wait_for_space_even_for_movable_actors_without_automatic_respawning() {
    for kind in ["turret", "bruiser"] {
        let mut app = respawn_app(PlayerRespawnMode::Individual, ActorRespawnScope::Dead);
        app.world_mut().resource_mut::<PendingActorSpawns>().0.clear();
        let mut map = app.world_mut().resource_mut::<MapConfig>();
        map.actor_spawn_zones.truncate(1);
        map.actor_spawn_zones[0].kind = kind.into();
        map.grids[0].levels[0].cells.rows[0][3].has_ramp = true;
        let (_, _rx) = add_player(&mut app, PlayerId(1));
        kill(&mut app, PlayerId(1));
        advance(&mut app, 2.0);
        assert!(app.world().resource::<PendingActorSpawns>().0.is_empty());
        assert!(app.world().resource::<ActorSpawner>().blocked.contains(&0));
        advance(&mut app, 1.0);
        assert!(app.world().resource::<PendingActorSpawns>().0.is_empty());
        app.world_mut().resource_mut::<MapConfig>().grids[0].levels[0]
            .cells
            .rows[0][3]
            .has_ramp = false;
        advance(&mut app, 0.0);
        assert_eq!(app.world().resource::<PendingActorSpawns>().0.len(), 1);
        assert!(!app.world().resource::<ActorSpawner>().blocked.contains(&0));
    }
}
