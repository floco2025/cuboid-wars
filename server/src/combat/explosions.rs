use std::collections::HashMap;

use bevy::{ecs::system::SystemParam, prelude::*};
use common::{
    config::GameplayConfig,
    constants::{EXPLOSION_BLAST_CORE_FRACTION, EXPLOSION_KNOCKBACK_MAX_SPEED, EXPLOSION_KNOCKBACK_UP_SPEED},
    health::apply_damage,
    physics::{CharacterVerticalVelocity, CollisionWorld, KnockbackVelocity, character_center},
    protocol::{ActorId, ActorMarker, Health, PlayerId, PlayerMarker, Position, SPlayerBlast, ServerMessage},
};

use super::{PendingExplosion, PendingExplosions, kill_actor, kill_player};
use crate::{
    actors::ActorMap,
    config::{ExplosionDamageConfig, ServerGameplayConfig},
    network::ServerToClient,
    players::{Invincibility, PlayerMap, QuestEvent, record_quest_event},
};

type PlayerBlastQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PlayerId,
        &'static Position,
        &'static mut Health,
        &'static mut CharacterVerticalVelocity,
        Option<&'static KnockbackVelocity>,
    ),
    (With<PlayerMarker>, Without<ActorMarker>),
>;

type ActorBlastQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ActorId,
        &'static Position,
        &'static mut Health,
        &'static mut CharacterVerticalVelocity,
        Option<&'static KnockbackVelocity>,
    ),
    (With<ActorMarker>, Without<PlayerMarker>),
>;

#[derive(SystemParam)]
pub struct ExplosionContext<'w, 's> {
    commands: Commands<'w, 's>,
    players: ResMut<'w, PlayerMap>,
    actors: ResMut<'w, ActorMap>,
    pending: ResMut<'w, PendingExplosions>,
    gameplay_config: Res<'w, GameplayConfig>,
    server_gameplay_config: Res<'w, ServerGameplayConfig>,
    invincibility: Res<'w, Invincibility>,
    collision_world: Res<'w, CollisionWorld>,
    player_query: PlayerBlastQuery<'w, 's>,
    actor_query: ActorBlastQuery<'w, 's>,
}

#[derive(Clone, Copy)]
enum BlastSource {
    Player(PlayerId),
    Actor(ActorId),
    Missile(PlayerId),
}

#[derive(Clone, Copy)]
struct BlastSpec {
    source: BlastSource,
    center: Vec3,
    excluded_actor: Option<Entity>,
    damage: ExplosionDamageConfig,
    // Player credited for blast kills. Death blasts credit no one; a missile
    // blast credits its shooter.
    killer: Option<PlayerId>,
}

struct DeadPlayer {
    id: PlayerId,
    entity: Entity,
    pos: Position,
}

struct DeadActor {
    id: ActorId,
    entity: Entity,
    pos: Position,
}

#[derive(Default)]
struct BlastOutcome {
    dead_players: Vec<DeadPlayer>,
    dead_actors: Vec<DeadActor>,
}

struct AccumulatedImpulse {
    entity: Entity,
    velocity: Vec3,
    blast_delta: Vec3,
    strength: f32,
}

pub fn explosions_system(mut context: ExplosionContext) {
    let mut player_impulses = HashMap::<PlayerId, AccumulatedImpulse>::new();
    let mut actor_impulses = HashMap::<ActorId, AccumulatedImpulse>::new();
    let respawn_delay_secs = context.gameplay_config.player.respawn_delay_secs;

    while let Some(pending) = context.pending.0.pop_front() {
        let spec = blast_spec(pending, &context.gameplay_config, &context.server_gameplay_config);
        let outcome = apply_blast(
            spec,
            &context.gameplay_config,
            &context.server_gameplay_config,
            context.invincibility.0,
            &context.collision_world,
            &context.players,
            &context.actors,
            &mut context.player_query,
            &mut context.actor_query,
            &mut player_impulses,
            &mut actor_impulses,
        );

        for death in outcome.dead_players {
            info!("{:?} died in {}", death.id, source_description(spec.source));
            player_impulses.remove(&death.id);
            if let Some(victim) = context.players.get_mut(&death.id) {
                victim.score += context.server_gameplay_config.scoring.player_death;
            }
            // No credit for self-kills or departed shooters. Award the kill
            // bonus before `kill_player` so the `SPlayerDeath` cue carries
            // the post-kill killer score.
            let killer = spec
                .killer
                .filter(|k| *k != death.id && context.players.get(k).is_some());
            if let Some(killer_id) = killer
                && let Some(shooter) = context.players.get_mut(&killer_id)
            {
                shooter.score += context.server_gameplay_config.scoring.player_kill;
            }
            kill_player(
                &mut context.commands,
                &mut context.players,
                death.id,
                death.entity,
                death.pos,
                respawn_delay_secs,
                killer,
                &mut context.pending,
            );
        }

        for death in outcome.dead_actors {
            actor_impulses.remove(&death.id);
            let killer = spec.killer.filter(|k| context.players.get(k).is_some());
            if let Some(killer_id) = killer
                && let Some(kind) = context.actors.get(&death.id).map(|info| info.spawn_kind.clone())
                && let Some(shooter) = context.players.get_mut(&killer_id)
            {
                shooter.score += context
                    .server_gameplay_config
                    .validated_actor(&kind)
                    .combat
                    .score_reward_on_kill;
                let quest_messages = record_quest_event(
                    shooter,
                    &context.server_gameplay_config.quests,
                    QuestEvent::ActorKilled { kind: &kind },
                );
                for msg in quest_messages {
                    let _ = shooter.channel.send(ServerToClient::Send(msg));
                }
            }
            if kill_actor(
                &mut context.commands,
                &mut context.actors,
                &context.players,
                &mut context.pending,
                death.id,
                death.entity,
                death.pos,
                killer,
            ) {
                info!("{:?} died in {}", death.id, source_description(spec.source));
            }
        }
    }

    apply_player_impulses(&mut context, player_impulses);
    apply_actor_impulses(&mut context, actor_impulses);
}

fn blast_spec(pending: PendingExplosion, gameplay: &GameplayConfig, server: &ServerGameplayConfig) -> BlastSpec {
    match pending {
        PendingExplosion::Player { source_id, pos } => BlastSpec {
            source: BlastSource::Player(source_id),
            center: character_center(pos, gameplay.player.physics()),
            excluded_actor: None,
            damage: server.player.explosion,
            killer: None,
        },
        PendingExplosion::Actor {
            source_id,
            source_entity,
            spawn_kind,
            pos,
        } => BlastSpec {
            source: BlastSource::Actor(source_id),
            center: character_center(pos, gameplay.validated_actor(&spawn_kind).physics()),
            excluded_actor: Some(source_entity),
            damage: server.validated_actor(&spawn_kind).combat.explosion,
            killer: None,
        },
        PendingExplosion::Missile { shooter, pos } => BlastSpec {
            source: BlastSource::Missile(shooter),
            // The detonation point itself — a missile has no character body.
            center: Vec3::from(pos),
            excluded_actor: None,
            damage: ExplosionDamageConfig {
                radius: gameplay.missiles.blast_radius,
                max_damage: server.missiles.max_damage,
            },
            killer: Some(shooter),
        },
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "blast resolution reads both victim query sets and impulse maps"
)]
fn apply_blast(
    spec: BlastSpec,
    gameplay: &GameplayConfig,
    server: &ServerGameplayConfig,
    invincible: bool,
    collision_world: &CollisionWorld,
    players: &PlayerMap,
    actors: &ActorMap,
    player_query: &mut PlayerBlastQuery,
    actor_query: &mut ActorBlastQuery,
    player_impulses: &mut HashMap<PlayerId, AccumulatedImpulse>,
    actor_impulses: &mut HashMap<ActorId, AccumulatedImpulse>,
) -> BlastOutcome {
    let mut outcome = BlastOutcome::default();

    for (entity, id, pos, mut health, mut vertical_velocity, knockback) in player_query.iter_mut() {
        if players.get(id).is_some_and(|info| info.is_dead()) {
            continue;
        }
        let victim_center = character_center(*pos, gameplay.player.physics());
        let Some(falloff) = visible_blast_falloff(spec.center, victim_center, spec.damage.radius, collision_world)
        else {
            continue;
        };
        if !invincible {
            apply_damage(
                &mut health,
                spec.damage.max_damage * falloff * (1.0 - server.player.armor),
            );
            if health.0 <= 0.0 {
                outcome.dead_players.push(DeadPlayer {
                    id: *id,
                    entity,
                    pos: *pos,
                });
                continue;
            }
        }

        vertical_velocity.0 += EXPLOSION_KNOCKBACK_UP_SPEED * falloff;
        accumulate_impulse(
            player_impulses,
            *id,
            entity,
            knockback,
            planar_shove(spec.center, victim_center, falloff),
            falloff,
        );
    }

    for (entity, id, pos, mut health, mut vertical_velocity, knockback) in actor_query.iter_mut() {
        if Some(entity) == spec.excluded_actor || health.0 <= 0.0 {
            continue;
        }
        let Some(info) = actors.get(id) else {
            continue;
        };
        let actor_physics = gameplay.validated_actor(&info.spawn_kind).physics();
        let victim_center = character_center(*pos, actor_physics);
        let Some(falloff) = visible_blast_falloff(spec.center, victim_center, spec.damage.radius, collision_world)
        else {
            continue;
        };
        let armor = server.validated_actor(&info.spawn_kind).combat.armor;
        apply_damage(&mut health, spec.damage.max_damage * falloff * (1.0 - armor));
        if health.0 <= 0.0 {
            outcome.dead_actors.push(DeadActor {
                id: *id,
                entity,
                pos: *pos,
            });
            continue;
        }

        vertical_velocity.0 += EXPLOSION_KNOCKBACK_UP_SPEED * falloff;
        accumulate_impulse(
            actor_impulses,
            *id,
            entity,
            knockback,
            planar_shove(spec.center, victim_center, falloff),
            falloff,
        );
    }

    outcome
}

fn accumulate_impulse<Id: std::hash::Hash + Eq + Copy>(
    impulses: &mut HashMap<Id, AccumulatedImpulse>,
    id: Id,
    entity: Entity,
    current: Option<&KnockbackVelocity>,
    shove: Vec3,
    strength: f32,
) {
    let accumulated = impulses.entry(id).or_insert_with(|| AccumulatedImpulse {
        entity,
        velocity: current.map_or(Vec3::ZERO, |knockback| knockback.0),
        blast_delta: Vec3::ZERO,
        strength: 0.0,
    });
    accumulated.velocity += shove;
    accumulated.blast_delta += shove;
    accumulated.strength = (accumulated.strength + strength).min(1.0);
}

fn apply_player_impulses(context: &mut ExplosionContext, impulses: HashMap<PlayerId, AccumulatedImpulse>) {
    let max_speed = EXPLOSION_KNOCKBACK_MAX_SPEED * 1.5;
    for (id, impulse) in impulses {
        if context.players.get(&id).is_some_and(|info| info.is_dead()) {
            continue;
        }
        let Ok((_, _, _, health, vertical_velocity, _)) = context.player_query.get_mut(impulse.entity) else {
            continue;
        };
        let velocity = impulse.velocity.clamp_length_max(max_speed);
        context
            .commands
            .entity(impulse.entity)
            .insert(KnockbackVelocity(velocity));
        let direction = impulse.blast_delta.normalize_or_zero();
        if let Some(info) = context.players.get(&id) {
            let _ = info
                .channel
                .send(ServerToClient::Send(ServerMessage::PlayerBlast(SPlayerBlast {
                    id,
                    health: *health,
                    vertical_velocity: vertical_velocity.0,
                    velocity_x: velocity.x,
                    velocity_z: velocity.z,
                    direction_x: direction.x,
                    direction_z: direction.z,
                    strength: impulse.strength,
                })));
        }
    }
}

fn apply_actor_impulses(context: &mut ExplosionContext, impulses: HashMap<ActorId, AccumulatedImpulse>) {
    let max_speed = EXPLOSION_KNOCKBACK_MAX_SPEED * 1.5;
    for (id, impulse) in impulses {
        if context.actors.get(&id).is_none() {
            continue;
        }
        context
            .commands
            .entity(impulse.entity)
            .insert(KnockbackVelocity(impulse.velocity.clamp_length_max(max_speed)));
    }
}

fn visible_blast_falloff(center: Vec3, target: Vec3, radius: f32, collision_world: &CollisionWorld) -> Option<f32> {
    let distance_squared = center.distance_squared(target);
    if distance_squared >= radius * radius {
        return None;
    }
    if !collision_world.line_of_sight_clear(center, target) {
        return None;
    }
    Some(blast_falloff_at_distance(distance_squared.sqrt(), radius))
}

fn planar_shove(center: Vec3, target: Vec3, falloff: f32) -> Vec3 {
    Vec3::new(target.x - center.x, 0.0, target.z - center.z).normalize_or_zero()
        * EXPLOSION_KNOCKBACK_MAX_SPEED
        * falloff
}

fn blast_falloff_at_distance(distance: f32, radius: f32) -> f32 {
    if distance >= radius {
        return 0.0;
    }
    let core = radius * EXPLOSION_BLAST_CORE_FRACTION;
    if distance <= core {
        return 1.0;
    }
    let progress = (distance - core) / (radius - core);
    (1.0 - progress).powi(2)
}

fn source_description(source: BlastSource) -> String {
    match source {
        BlastSource::Player(id) => format!("{:?}'s death explosion", id),
        BlastSource::Actor(id) => format!("{:?}'s explosion", id),
        BlastSource::Missile(id) => format!("{:?}'s missile explosion", id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        actors::actor_removal_system,
        actors::{ActorGoal, ActorInfo},
        characters::characters_health_regeneration_system,
        players::PlayerInfo,
    };
    use common::protocol::{ActorMoveIntent, BarrierKindTable, MapLayout, SPlayerDeath};
    use tokio::sync::mpsc::unbounded_channel;

    fn test_app() -> App {
        let gameplay = GameplayConfig::load_default().expect("default gameplay config should load");
        let server = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
        let collision_world = CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default());
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(gameplay)
            .insert_resource(server)
            .insert_resource(collision_world)
            .insert_resource(PlayerMap::default())
            .insert_resource(ActorMap::default())
            .insert_resource(Invincibility(false))
            .insert_resource(PendingExplosions::default());
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
            ))
            .id();
        app.world_mut().resource_mut::<ActorMap>().insert(
            id,
            ActorInfo::new(
                entity,
                0,
                "zapper".to_owned(),
                ActorGoal::Patrol {
                    intent: ActorMoveIntent::Idle,
                    direction_timer: 0.0,
                    ledge_escape_timer: 0.0,
                },
            ),
        );
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

        accumulate_impulse(&mut impulses, id, entity, Some(&existing), Vec3::X * 3.0, 0.4);
        accumulate_impulse(&mut impulses, id, entity, Some(&existing), Vec3::Z * 4.0, 0.5);

        let impulse = impulses.get(&id).expect("player impulse");
        assert_eq!(impulse.velocity, Vec3::new(5.0, 0.0, 4.0));
        assert_eq!(impulse.blast_delta, Vec3::new(3.0, 0.0, 4.0));
        assert!((impulse.strength - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn actor_explosions_resolve_chain_to_fixed_point_before_regeneration() {
        let mut app = test_app();
        app.add_systems(
            Update,
            (
                actor_removal_system,
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
                CharacterVerticalVelocity::default(),
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

        let ServerToClient::Send(ServerMessage::PlayerBlast(message)) =
            receiver.try_recv().expect("combined blast result")
        else {
            panic!("expected player blast message");
        };
        let health = *app.world().entity(entity).get::<Health>().expect("player health");
        assert_eq!(message.id, id);
        assert_eq!(message.health, health);
        assert!(message.health.0 < 1_000.0);
        assert!(message.velocity_x.abs() < 0.001);
        assert!(message.velocity_z.abs() < 0.001);
        assert!(message.strength > 0.0);
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
            ))
            .id();
        let (sender, receiver) = unbounded_channel();
        let mut info = PlayerInfo::new(entity, sender);
        info.logged_in = true;
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

    #[test]
    fn missile_blast_awards_shooter_player_kill_credit() {
        let mut app = test_app();
        app.add_systems(Update, explosions_system);
        let shooter_id = PlayerId(1);
        let victim_id = PlayerId(2);
        // Shooter far outside the blast so only the victim dies.
        let (_, mut shooter_rx) = spawn_logged_in_player(&mut app, shooter_id, 100.0, 100.0);
        spawn_logged_in_player(&mut app, victim_id, 0.0, 1.0);
        app.world_mut()
            .resource_mut::<PendingExplosions>()
            .push_missile(shooter_id, Position::default());

        app.update();

        let scoring = app.world().resource::<ServerGameplayConfig>().scoring.clone();
        let players = app.world().resource::<PlayerMap>();
        let shooter_score = players.get(&shooter_id).expect("shooter still present").score;
        assert_eq!(shooter_score, scoring.player_kill);
        assert_eq!(
            players.get(&victim_id).expect("victim still present").score,
            scoring.player_death
        );

        let death = next_player_death(&mut shooter_rx);
        assert_eq!(death.id, victim_id);
        assert_eq!(death.killer, Some(shooter_id));
        assert_eq!(death.killer_score, Some(scoring.player_kill));
    }

    #[test]
    fn missile_self_blast_awards_no_credit() {
        let mut app = test_app();
        app.add_systems(Update, explosions_system);
        let shooter_id = PlayerId(1);
        let (_, mut shooter_rx) = spawn_logged_in_player(&mut app, shooter_id, 0.0, 1.0);
        app.world_mut()
            .resource_mut::<PendingExplosions>()
            .push_missile(shooter_id, Position::default());

        app.update();

        let scoring = app.world().resource::<ServerGameplayConfig>().scoring.clone();
        let players = app.world().resource::<PlayerMap>();
        assert_eq!(
            players.get(&shooter_id).expect("shooter still present").score,
            scoring.player_death,
            "self-kill takes the death penalty but earns no kill bonus"
        );

        let death = next_player_death(&mut shooter_rx);
        assert_eq!(death.id, shooter_id);
        assert_eq!(death.killer, None);
    }

    #[test]
    fn missile_blast_kills_actor_with_shooter_credit() {
        let mut app = test_app();
        app.add_systems(Update, explosions_system);
        let shooter_id = PlayerId(1);
        let (_, mut shooter_rx) = spawn_logged_in_player(&mut app, shooter_id, 100.0, 100.0);
        spawn_actor(&mut app, ActorId(1), 0.0, 1.0);
        app.world_mut()
            .resource_mut::<PendingExplosions>()
            .push_missile(shooter_id, Position::default());

        app.update();

        let reward = app
            .world()
            .resource::<ServerGameplayConfig>()
            .validated_actor("zapper")
            .combat
            .score_reward_on_kill;
        assert_eq!(
            app.world()
                .resource::<PlayerMap>()
                .get(&shooter_id)
                .expect("shooter still present")
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
}
