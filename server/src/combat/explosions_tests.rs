use std::collections::HashMap;

use bevy::prelude::*;
use tokio::sync::mpsc::unbounded_channel;

use super::{PendingExplosions, explosions::*};
use crate::{
    actors::{ActorCrushed, ActorInfo, ActorMap, actors_removal_system, navigation::NavGraphs},
    characters::characters_health_regeneration_system,
    config::ServerGameplayConfig,
    map::MapConfig,
    network::ServerToClient,
    players::{Invincibility, PlayerInfo, PlayerMap},
    quests::{QuestBoard, QuestCatalog},
    test_geometry::geometry,
};
use common::{
    map::Carriers,
    physics::{
        AirborneMomentum, CharacterVerticalVelocity, CollisionWorld, KnockbackVelocity, blast_falloff_at_distance,
    },
    protocol::{
        ActorAnchor, ActorId, ActorMarker, Barrier, BarrierKindId, BarrierKindTable, BridgeKindId, CarrierId, Health,
        LightBridge, MapLayout, PlateState, PlayerId, PlayerMarker, Position, SPlayerDeath, ServerMessage,
    },
};

fn test_app() -> App {
    let server = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
    let gameplay = server.gameplay_config();
    let map_settings = server
        .maps
        .get(&server.default_map)
        .expect("default map settings missing")
        .settings
        .clone();
    let collision_world = CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default());
    let quest_catalog = QuestCatalog::from_config(&server);
    let quest_board = QuestBoard::from_catalog(&quest_catalog);
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(gameplay)
        .insert_resource(map_settings)
        .insert_resource(server)
        .insert_resource(quest_catalog)
        .insert_resource(collision_world)
        .init_resource::<PlateState>()
        .insert_resource(Carriers::default())
        .insert_resource(NavGraphs::new(&MapConfig::for_grid(Vec::new(), geometry(1, 1))))
        .insert_resource(PlayerMap::default())
        .insert_resource(ActorMap::default())
        .insert_resource(Invincibility(false))
        .insert_resource(PendingExplosions::default())
        .insert_resource(quest_board);
    app
}

fn spawn_actor(app: &mut App, id: ActorId, x: f32, health: f32) -> Entity {
    let pos = Position { x, y: 0.0, z: 0.0 };
    let entity = app
        .world_mut()
        .spawn((
            ActorMarker,
            id,
            pos,
            Health(health),
            CharacterVerticalVelocity::default(),
            ActorCrushed::default(),
        ))
        .id();
    app.world_mut()
        .resource_mut::<ActorMap>()
        .insert(id, ActorInfo::new(entity, 0, "zapper".to_owned(), CarrierId::WORLD));
    entity
}

#[test]
fn blast_falloff_is_full_in_core_and_quadratic_to_radius() {
    assert_eq!(blast_falloff_at_distance(0.0, 10.0), 1.0);
    assert_eq!(blast_falloff_at_distance(2.5, 10.0), 1.0);
    assert_eq!(blast_falloff_at_distance(6.25, 10.0), 0.25);
    assert_eq!(blast_falloff_at_distance(10.0, 10.0), 0.0);
    assert_eq!(blast_falloff_at_distance(11.0, 10.0), 0.0);
}

#[test]
fn accumulated_impulses_sum_existing_and_new_velocity() {
    let id = PlayerId(1);
    let entity = Entity::PLACEHOLDER;
    let existing = KnockbackVelocity(Vec3::X * 2.0);
    let mut impulses = HashMap::new();

    accumulate_impulse(&mut impulses, id, entity, Some(&existing), Vec3::X * 3.0);
    accumulate_impulse(&mut impulses, id, entity, Some(&existing), Vec3::Z * 4.0);

    let impulse = impulses.get(&id).expect("player impulse");
    assert_eq!(impulse.velocity, Vec3::new(5.0, 0.0, 4.0));
}

#[test]
fn actor_explosions_resolve_chain_to_fixed_point_before_regeneration() {
    let mut app = test_app();
    app.add_systems(
        Update,
        (
            actors_removal_system,
            explosions_system,
            characters_health_regeneration_system,
        )
            .chain(),
    );

    let first = spawn_actor(&mut app, ActorId(1), 0.0, 0.0);
    let second = spawn_actor(&mut app, ActorId(2), 5.0, 1.0);
    let third = spawn_actor(&mut app, ActorId(3), 10.0, 1.0);

    app.update();

    let actors = app.world().resource::<ActorMap>();
    assert!(actors.get(&ActorId(1)).is_none());
    assert!(actors.get(&ActorId(2)).is_none());
    assert!(actors.get(&ActorId(3)).is_none());
    assert!(app.world().resource::<PendingExplosions>().0.is_empty());
    assert!(app.world().get_entity(first).is_err());
    assert!(app.world().get_entity(second).is_err());
    assert!(app.world().get_entity(third).is_err());
}

#[test]
fn crushed_actor_death_broadcasts_once_and_detonates_without_kill_credit() {
    let mut app = test_app();
    app.add_systems(Update, (actors_removal_system, explosions_system).chain());
    let observer_id = PlayerId(1);
    let (_, mut receiver) = spawn_logged_in_player(&mut app, observer_id, 100.0, 100.0);
    let crushed_id = ActorId(1);
    let crushed = spawn_actor(&mut app, crushed_id, 0.0, 100.0);
    let nearby = spawn_actor(&mut app, ActorId(2), 5.0, 1.0);
    app.world_mut().entity_mut(crushed).insert(ActorCrushed(true));
    app.world_mut()
        .resource_mut::<ActorMap>()
        .get_mut(&crushed_id)
        .expect("crushed actor missing")
        .last_damager = Some(observer_id);

    app.update();

    for id in [crushed_id, ActorId(2)] {
        let ServerToClient::Send(ServerMessage::ActorDeath(death)) =
            receiver.try_recv().expect("actor death cue missing")
        else {
            panic!("unexpected message instead of actor death cue");
        };
        assert_eq!(death.id, id);
        assert_eq!(death.killer, None);
        assert_eq!(death.killer_score, None);
    }
    assert!(app.world().get_entity(crushed).is_err());
    assert!(app.world().get_entity(nearby).is_err());
    assert!(app.world().resource::<ActorMap>().get(&crushed_id).is_none());
    assert!(app.world().resource::<ActorMap>().get(&ActorId(2)).is_none());
    assert!(app.world().resource::<PendingExplosions>().0.is_empty());
    assert_eq!(
        app.world()
            .resource::<PlayerMap>()
            .get(&observer_id)
            .expect("observer missing")
            .session
            .score,
        0
    );

    app.update();

    assert!(receiver.try_recv().is_err());
    assert!(app.world().resource::<PendingExplosions>().0.is_empty());
}

#[test]
fn surviving_actor_receives_blast_knockback() {
    let mut app = test_app();
    app.add_systems(Update, explosions_system);
    let actor = spawn_actor(&mut app, ActorId(1), 1.0, 1_000.0);
    app.world_mut()
        .resource_mut::<PendingExplosions>()
        .push_player(PlayerId(9), Position::default());

    app.update();

    let actor = app.world().entity(actor);
    assert!(
        actor
            .get::<Health>()
            .is_some_and(|health| (0.0..1_000.0).contains(&health.0))
    );
    assert!(
        actor
            .get::<KnockbackVelocity>()
            .is_some_and(|knockback| knockback.0.x > 0.0)
    );
    assert!(
        actor
            .get::<CharacterVerticalVelocity>()
            .is_some_and(|velocity| velocity.0 > 0.0)
    );
}

#[test]
fn missile_destroys_turret_with_normal_death_cue_and_kill_credit() {
    let mut app = test_app();
    app.add_systems(Update, explosions_system);
    let shooter = PlayerId(1);
    let (_, mut receiver) = spawn_logged_in_player(&mut app, shooter, 100.0, 500.0);
    let id = ActorId(1);
    let entity = spawn_actor(&mut app, id, 1.0, 50.0);
    let pos = *app.world().get::<Position>(entity).expect("turret position missing");
    {
        let mut actors = app.world_mut().resource_mut::<ActorMap>();
        let info = actors.get_mut(&id).expect("turret missing");
        info.spawn_kind = "turret".into();
        info.anchor = Some(ActorAnchor {
            carrier: CarrierId::WORLD,
            pos,
        });
    }
    queue_missile_blast(&mut app, shooter, pos);
    app.update();
    assert!(app.world().get_entity(entity).is_err());
    assert!(app.world().resource::<ActorMap>().get(&id).is_none());
    let mut deaths = Vec::new();
    while let Ok(ServerToClient::Send(message)) = receiver.try_recv() {
        if let ServerMessage::ActorDeath(death) = message {
            deaths.push(death);
        }
    }
    assert_eq!(deaths.len(), 1);
    assert_eq!(deaths[0].id, id);
    assert_eq!(deaths[0].killer, Some(shooter));
    assert_eq!(deaths[0].killer_score, Some(150));
}

#[test]
fn turret_takes_blast_damage_without_knockback() {
    let mut app = test_app();
    app.add_systems(Update, explosions_system);
    let id = ActorId(1);
    let entity = spawn_actor(&mut app, id, 1.0, 1000.0);
    let pos = *app.world().get::<Position>(entity).expect("actor position missing");
    let mut actors = app.world_mut().resource_mut::<ActorMap>();
    let info = actors.get_mut(&id).expect("turret missing");
    info.spawn_kind = "turret".into();
    info.anchor = Some(ActorAnchor {
        carrier: CarrierId::WORLD,
        pos,
    });
    app.world_mut()
        .resource_mut::<PendingExplosions>()
        .push_player(PlayerId(9), Position::default());
    app.update();
    assert!(
        app.world()
            .get::<Health>(entity)
            .is_some_and(|health| (0.0..1000.0).contains(&health.0))
    );
    assert!(app.world().get::<KnockbackVelocity>(entity).is_none());
    assert_eq!(
        app.world()
            .get::<CharacterVerticalVelocity>(entity)
            .expect("actor velocity missing")
            .0,
        0.0
    );
}

#[test]
fn simultaneous_blasts_send_one_combined_player_result() {
    let mut app = test_app();
    app.add_systems(Update, explosions_system);
    let id = PlayerId(1);
    let entity = app
        .world_mut()
        .spawn((
            PlayerMarker,
            id,
            Position::default(),
            Health(1_000.0),
            CharacterVerticalVelocity(-7.0),
            AirborneMomentum::default(),
            KnockbackVelocity(Vec3::Z * 3.0),
        ))
        .id();
    let (sender, mut receiver) = unbounded_channel();
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .insert(id, PlayerInfo::new(entity, sender));
    {
        let mut pending = app.world_mut().resource_mut::<PendingExplosions>();
        pending.push_actor(
            ActorId(1),
            Entity::from_bits(10),
            "zapper".to_owned(),
            Position { x: -1.0, ..default() },
        );
        pending.push_actor(
            ActorId(2),
            Entity::from_bits(11),
            "zapper".to_owned(),
            Position { x: 1.0, ..default() },
        );
    }

    app.update();

    let ServerToClient::Send(ServerMessage::PlayerKnockback(message)) =
        receiver.try_recv().expect("combined blast result")
    else {
        panic!("expected player knockback message");
    };
    let health = *app.world().entity(entity).get::<Health>().expect("player health");
    assert_eq!(message.id, id);
    assert_eq!(message.health, health);
    assert!(message.health.0 < 1_000.0);
    assert!(message.impulse[0].abs() < 0.001);
    assert!(message.impulse[2].abs() < 0.001);
    assert!(message.impulse[1] > 0.0);
    assert_eq!(
        app.world()
            .get::<CharacterVerticalVelocity>(entity)
            .expect("velocity missing")
            .0,
        -7.0
    );
    assert_eq!(
        app.world()
            .get::<KnockbackVelocity>(entity)
            .expect("knockback missing")
            .0,
        Vec3::Z * 3.0
    );
    assert!(receiver.try_recv().is_err());
}

fn spawn_logged_in_player(
    app: &mut App,
    id: PlayerId,
    x: f32,
    health: f32,
) -> (Entity, tokio::sync::mpsc::UnboundedReceiver<ServerToClient>) {
    let entity = app
        .world_mut()
        .spawn((
            PlayerMarker,
            id,
            Position { x, ..default() },
            Health(health),
            CharacterVerticalVelocity::default(),
            AirborneMomentum::default(),
            KnockbackVelocity::default(),
        ))
        .id();
    let (sender, receiver) = unbounded_channel();
    let mut info = PlayerInfo::new(entity, sender);
    info.connection.logged_in = true;
    app.world_mut().resource_mut::<PlayerMap>().insert(id, info);
    (entity, receiver)
}

fn next_player_death(receiver: &mut tokio::sync::mpsc::UnboundedReceiver<ServerToClient>) -> SPlayerDeath {
    loop {
        match receiver.try_recv().expect("expected a PlayerDeath broadcast") {
            ServerToClient::Send(ServerMessage::PlayerDeath(msg)) => return msg,
            _ => continue,
        }
    }
}

fn next_feed_line(receiver: &mut tokio::sync::mpsc::UnboundedReceiver<ServerToClient>) -> String {
    loop {
        match receiver.try_recv().expect("expected a Feed broadcast") {
            ServerToClient::Send(ServerMessage::Feed(msg)) => {
                return msg.spans.into_iter().map(|span| span.text).collect();
            }
            _ => continue,
        }
    }
}

#[test]
fn missile_blast_awards_shooter_player_kill_credit() {
    let mut app = test_app();
    app.add_systems(Update, explosions_system);
    let shooter_id = PlayerId(1);
    let victim_id = PlayerId(2);
    // Shooter far outside the blast so only the victim dies.
    let (_, mut shooter_rx) = spawn_logged_in_player(&mut app, shooter_id, 100.0, 100.0);
    spawn_logged_in_player(&mut app, victim_id, 0.0, 1.0);
    queue_missile_blast(&mut app, shooter_id, Position::default());

    app.update();

    let scoring = app.world().resource::<ServerGameplayConfig>().scoring.clone();
    let players = app.world().resource::<PlayerMap>();
    let shooter_score = players.get(&shooter_id).expect("shooter still present").session.score;
    assert_eq!(shooter_score, scoring.player_kill);
    assert_eq!(
        players.get(&victim_id).expect("victim still present").session.score,
        scoring.player_death
    );

    let death = next_player_death(&mut shooter_rx);
    assert_eq!(death.id, victim_id);
    assert_eq!(death.killer, Some(shooter_id));
    assert_eq!(death.killer_score, Some(scoring.player_kill));
    assert_eq!(next_feed_line(&mut shooter_rx), "Player 1 blew up Player 2");
}

#[test]
fn missile_self_blast_awards_no_credit() {
    let mut app = test_app();
    app.add_systems(Update, explosions_system);
    let shooter_id = PlayerId(1);
    let (_, mut shooter_rx) = spawn_logged_in_player(&mut app, shooter_id, 0.0, 1.0);
    queue_missile_blast(&mut app, shooter_id, Position::default());

    app.update();

    let scoring = app.world().resource::<ServerGameplayConfig>().scoring.clone();
    let players = app.world().resource::<PlayerMap>();
    assert_eq!(
        players.get(&shooter_id).expect("shooter still present").session.score,
        scoring.player_death,
        "self-kill takes the death penalty but earns no kill bonus"
    );

    let death = next_player_death(&mut shooter_rx);
    assert_eq!(death.id, shooter_id);
    assert_eq!(death.killer, None);
    assert_eq!(next_feed_line(&mut shooter_rx), "Player 1 blew themselves up");
}

#[test]
fn missile_blast_kills_actor_with_shooter_credit() {
    let mut app = test_app();
    app.add_systems(Update, explosions_system);
    let shooter_id = PlayerId(1);
    let (_, mut shooter_rx) = spawn_logged_in_player(&mut app, shooter_id, 100.0, 100.0);
    spawn_actor(&mut app, ActorId(1), 0.0, 1.0);
    queue_missile_blast(&mut app, shooter_id, Position::default());

    app.update();

    let reward = app.world().resource::<ServerGameplayConfig>().scoring.actor_kill["zapper"];
    assert_eq!(
        app.world()
            .resource::<PlayerMap>()
            .get(&shooter_id)
            .expect("shooter still present")
            .session
            .score,
        reward
    );

    let death = loop {
        match shooter_rx.try_recv().expect("expected an ActorDeath broadcast") {
            ServerToClient::Send(ServerMessage::ActorDeath(msg)) => break msg,
            _ => continue,
        }
    };
    assert_eq!(death.id, ActorId(1));
    assert_eq!(death.killer, Some(shooter_id));
    assert_eq!(death.killer_score, Some(reward));
}
fn field_world(bridge: bool) -> CollisionWorld {
    let layout = if bridge {
        MapLayout {
            light_bridges: vec![LightBridge {
                x1: -4.0,
                z1: -4.0,
                x2: 4.0,
                z2: 4.0,
                y: 3.0,
                thickness: 0.1,
                level: 1,
                kind: BridgeKindId(0),
                carrier: CarrierId::WORLD,
            }],
            ..default()
        }
    } else {
        MapLayout {
            barriers: vec![Barrier {
                x1: 1.0,
                z1: -4.0,
                x2: 1.0,
                z2: 4.0,
                y: 0.0,
                height: 4.0,
                width: 0.1,
                level: 0,
                levels: 1,
                kind: BarrierKindId(0),
                carrier: CarrierId::WORLD,
            }],
            ..default()
        }
    };
    let kinds = BarrierKindTable::from_ids(vec!["shield".into()]).expect("barrier catalog rejected");
    CollisionWorld::from_map_layout(&layout, &kinds)
}

fn power_field(app: &mut App, bridge: bool, active: bool) {
    let mut plates = app.world_mut().resource_mut::<PlateState>();
    plates.open_barrier_kinds = if active { vec![] } else { vec![BarrierKindId(0)] };
    let powered = if bridge && active {
        vec![BridgeKindId(0)]
    } else {
        vec![]
    };
    plates.powered_bridge_kinds = powered.clone();
    app.world_mut()
        .resource_mut::<CollisionWorld>()
        .set_powered_bridges(&powered);
}

#[test]
fn fields_shield_players_and_actors_from_missile_damage_and_knockback() {
    for bridge in [false, true] {
        for active in [false, true] {
            let mut app = test_app();
            app.insert_resource(field_world(bridge))
                .add_systems(Update, explosions_system);
            let (player, mut receiver) = spawn_logged_in_player(&mut app, PlayerId(1), 2.0, 10000.0);
            app.world_mut()
                .resource_mut::<PlayerMap>()
                .get_mut(&PlayerId(1))
                .expect("player missing")
                .add_key(BarrierKindId(0));
            let actor = spawn_actor(&mut app, ActorId(1), 2.0, 10000.0);
            power_field(&mut app, bridge, active);
            queue_missile_blast(
                &mut app,
                PlayerId(2),
                Position {
                    x: 0.0,
                    y: if bridge { 5.0 } else { 1.0 },
                    z: 0.0,
                },
            );
            app.update();
            let impulse = std::iter::from_fn(|| receiver.try_recv().ok()).find_map(|message| match message {
                ServerToClient::Send(ServerMessage::PlayerKnockback(message)) => Some(message.impulse),
                _ => None,
            });
            assert_eq!(impulse.is_some(), !active);
            if let Some(impulse) = impulse {
                assert!(impulse[1] > 0.0);
            }
            for entity in [player, actor] {
                let health = app.world().get::<Health>(entity).expect("victim health missing").0;
                let vertical = app
                    .world()
                    .get::<CharacterVerticalVelocity>(entity)
                    .expect("victim velocity missing")
                    .0;
                assert_eq!(health < 10000.0, !active, "bridge={bridge}, active={active}");
                assert_eq!(vertical > 0.0, entity == actor && !active);
                assert_eq!(
                    app.world()
                        .get::<KnockbackVelocity>(entity)
                        .is_some_and(|knockback| knockback.0 != Vec3::ZERO),
                    entity == actor && !active
                );
            }
        }
    }
}

#[test]
fn activating_cover_stops_an_existing_beam_burst_and_reopening_restores_damage() {
    use crate::{actors::BeamState, combat::actors_beam_damage_system};
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    for bridge in [false, true] {
        let mut app = test_app();
        app.insert_resource(field_world(bridge))
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(1.0 / 30.0)))
            .add_systems(Update, actors_beam_damage_system);
        let (player, _) = spawn_logged_in_player(&mut app, PlayerId(1), 2.0, 1000.0);
        let actor = spawn_actor(&mut app, ActorId(1), 0.0, 1000.0);
        if bridge {
            app.world_mut()
                .get_mut::<Position>(actor)
                .expect("actor position missing")
                .y = 4.0;
        }
        app.world_mut()
            .resource_mut::<ActorMap>()
            .get_mut(&ActorId(1))
            .expect("actor missing")
            .beam = BeamState::Firing {
            target: PlayerId(1),
            started_tick: 0,
            remaining_secs: 2.0,
        };
        app.update();
        let mut previous = app.world().get::<Health>(player).expect("player health missing").0;
        for active in [false, true, true, false] {
            power_field(&mut app, bridge, active);
            app.update();
            let current = app.world().get::<Health>(player).expect("player health missing").0;
            assert_eq!(current < previous, !active, "bridge={bridge}, active={active}");
            previous = current;
        }
    }
}

fn queue_missile_blast(app: &mut App, shooter: PlayerId, pos: Position) {
    use common::{
        config::GameplayConfig,
        physics::{blast_hit, character_hitbox_center},
        protocol::{HitTarget, MissileBlastHit},
    };
    let world = app.world();
    let gameplay = world.resource::<GameplayConfig>();
    let players = world.resource::<PlayerMap>();
    let actors = world.resource::<ActorMap>();
    let candidates = players
        .iter()
        .filter_map(|(id, info)| {
            let pos = *world.get::<Position>(info.entity()?)?;
            Some((
                HitTarget::Player {
                    id: *id,
                    generation: info.session.generation,
                },
                pos,
                gameplay.player.physics(),
            ))
        })
        .chain(actors.iter().filter_map(|(id, info)| {
            Some((
                HitTarget::Actor(*id),
                *world.get::<Position>(info.entity)?,
                gameplay.expect_actor(&info.spawn_kind).physics(),
            ))
        }));
    let center = Vec3::from(pos);
    let radius = world
        .resource::<ServerGameplayConfig>()
        .combat
        .damage
        .missile_blast
        .radius;
    let hits = candidates
        .filter_map(|(target, pos, physics)| {
            let victim = character_hitbox_center(pos, physics);
            let (falloff, direction) = blast_hit(
                center,
                victim,
                radius,
                world.resource::<CollisionWorld>(),
                &world.resource::<PlateState>().open_barrier_kinds,
            )?;
            Some(MissileBlastHit {
                target,
                falloff,
                direction: [direction.x, direction.z],
            })
        })
        .collect();
    app.world_mut()
        .resource_mut::<PendingExplosions>()
        .push_missile(shooter, pos, hits);
}

#[test]
fn reported_missile_hits_ignore_server_distance_but_not_victim_generation_or_duplicate_hits() {
    use common::protocol::{HitTarget, MissileBlastHit, PlayerGeneration};
    let mut app = test_app();
    app.add_systems(Update, explosions_system);
    let (entity, mut receiver) = spawn_logged_in_player(&mut app, PlayerId(1), 1000.0, 10000.0);
    let generation = app
        .world()
        .resource::<PlayerMap>()
        .get(&PlayerId(1))
        .expect("victim missing")
        .session
        .generation;
    let hit = |generation| MissileBlastHit {
        target: HitTarget::Player {
            id: PlayerId(1),
            generation,
        },
        falloff: 0.5,
        direction: [0.0, -1.0],
    };
    app.world_mut().resource_mut::<PendingExplosions>().push_missile(
        PlayerId(2),
        Position::default(),
        vec![hit(PlayerGeneration(generation.0.wrapping_sub(1)))],
    );
    app.update();
    assert_eq!(app.world().get::<Health>(entity).expect("health missing").0, 10000.0);
    assert!(receiver.try_recv().is_err(), "retired body received an impulse");
    app.world_mut().resource_mut::<PendingExplosions>().push_missile(
        PlayerId(2),
        Position::default(),
        vec![hit(generation), hit(generation)],
    );
    app.update();
    let damage = app
        .world()
        .resource::<ServerGameplayConfig>()
        .combat
        .damage
        .missile_blast
        .max_damage
        * 0.5;
    assert_eq!(
        app.world().get::<Health>(entity).expect("health missing").0,
        10000.0 - damage
    );
    let ServerToClient::Send(ServerMessage::PlayerKnockback(message)) = receiver.try_recv().expect("impulse missing")
    else {
        panic!("unexpected reply");
    };
    assert_eq!(message.impulse[0], 0.0);
    assert!(message.impulse[1] > 0.0);
    assert!(
        message.impulse[2] < 0.0,
        "impulse was recomputed from the delayed server position"
    );
    assert!(receiver.try_recv().is_err());
}
