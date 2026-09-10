use std::collections::HashMap;

use bevy::{ecs::system::SystemParam, prelude::*};
use common::{
    config::{GameplayConfig, MapMovementConfig},
    constants::EXPLOSION_BLAST_CORE_FRACTION,
    health::apply_damage,
    physics::{CharacterVerticalVelocity, CollisionWorld, KnockbackVelocity, character_hitbox_center},
    protocol::{
        ActorId, ActorMarker, BarrierKindId, Health, MapSettings, PlateState, PlayerId, PlayerMarker, Position,
        SPlayerBlast, ServerMessage,
    },
};

use super::{DeathSource, PendingExplosion, PendingExplosions, award_actor_kill, kill_actor, kill_player};
use crate::{
    actors::ActorMap,
    config::{BlastConfig, ServerGameplayConfig},
    network::ServerToClient,
    players::{Invincibility, PlayerMap},
    quests::{QuestBoard, QuestCatalog},
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
        &'static mut KnockbackVelocity,
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
    map_settings: Res<'w, MapSettings>,
    server_gameplay_config: Res<'w, ServerGameplayConfig>,
    quest_board: ResMut<'w, QuestBoard>,
    quest_catalog: Res<'w, QuestCatalog>,
    invincibility: Res<'w, Invincibility>,
    collision_world: Res<'w, CollisionWorld>,
    plates: Res<'w, PlateState>,
    player_query: PlayerBlastQuery<'w, 's>,
    actor_query: ActorBlastQuery<'w, 's>,
}

#[derive(Clone)]
enum BlastSource {
    Player(PlayerId),
    // The source actor is already gone from `ActorMap` when its blast
    // resolves, so the kind rides along for the log label.
    Actor { id: ActorId, kind: String },
    Missile(PlayerId),
}

impl From<&BlastSource> for DeathSource {
    fn from(source: &BlastSource) -> Self {
        match source {
            BlastSource::Player(id) => Self::PlayerBlast(*id),
            BlastSource::Actor { kind, .. } => Self::ActorBlast { kind: kind.clone() },
            BlastSource::Missile(id) => Self::Missile(*id),
        }
    }
}

#[derive(Clone)]
struct BlastSpec {
    source: BlastSource,
    center: Vec3,
    excluded_actor: Option<Entity>,
    damage: BlastConfig,
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

pub(super) struct AccumulatedImpulse {
    entity: Entity,
    pub(super) velocity: Vec3,
    pub(super) blast_delta: Vec3,
    pub(super) strength: f32,
}

pub fn explosions_system(mut context: ExplosionContext) {
    let mut player_impulses = HashMap::<PlayerId, AccumulatedImpulse>::new();
    let mut actor_impulses = HashMap::<ActorId, AccumulatedImpulse>::new();
    let respawn_secs = context.server_gameplay_config.player.respawn_secs;

    while let Some(pending) = context.pending.0.pop_front() {
        let spec = blast_spec(pending, &context.gameplay_config, &context.server_gameplay_config);
        let outcome = apply_blast(
            &spec,
            &context.gameplay_config,
            &context.map_settings.movement,
            context.invincibility.0,
            &context.collision_world,
            &context.plates.open_barrier_kinds,
            &context.players,
            &context.actors,
            &mut context.player_query,
            &mut context.actor_query,
            &mut player_impulses,
            &mut actor_impulses,
        );

        for death in outcome.dead_players {
            info!(
                "{} died in {}",
                context.players.describe(&death.id),
                source_description(&spec.source, &context.players)
            );
            player_impulses.remove(&death.id);
            kill_player(
                &mut context.commands,
                &mut context.players,
                death.id,
                death.entity,
                death.pos,
                respawn_secs,
                DeathSource::from(&spec.source),
                &context.server_gameplay_config,
                &mut context.pending,
            );
        }

        for death in outcome.dead_actors {
            let victim = context.actors.describe(&death.id);
            actor_impulses.remove(&death.id);
            let killer = spec.killer.filter(|k| context.players.get(k).is_some());
            if let Some(killer_id) = killer
                && let Some(kind) = context.actors.get(&death.id).map(|info| info.spawn_kind.clone())
            {
                award_actor_kill(
                    &mut context.players,
                    &mut context.quest_board,
                    &context.quest_catalog,
                    killer_id,
                    &kind,
                    &context.server_gameplay_config,
                );
            }
            if kill_actor(
                &mut context.commands,
                &mut context.actors,
                &context.players,
                &mut context.pending,
                &context.server_gameplay_config.feed,
                death.id,
                death.entity,
                death.pos,
                killer,
            ) {
                info!(
                    "{} died in {}",
                    victim,
                    source_description(&spec.source, &context.players)
                );
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
            center: character_hitbox_center(pos, gameplay.player.physics()),
            excluded_actor: None,
            damage: server.combat.damage.player_blast,
            killer: None,
        },
        PendingExplosion::Actor {
            source_id,
            source_entity,
            spawn_kind,
            pos,
        } => BlastSpec {
            source: BlastSource::Actor {
                id: source_id,
                kind: spawn_kind.clone(),
            },
            center: character_hitbox_center(pos, gameplay.expect_actor(&spawn_kind).physics()),
            excluded_actor: Some(source_entity),
            damage: server.combat.damage.expect_actor(&spawn_kind).death_blast,
            killer: None,
        },
        PendingExplosion::Missile { shooter, pos } => BlastSpec {
            source: BlastSource::Missile(shooter),
            // The detonation point itself — a missile has no character body.
            center: Vec3::from(pos),
            excluded_actor: None,
            damage: server.combat.damage.missile_blast,
            killer: Some(shooter),
        },
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "blast resolution reads both victim query sets and impulse maps"
)]
fn apply_blast(
    spec: &BlastSpec,
    gameplay: &GameplayConfig,
    movement: &MapMovementConfig,
    invincible: bool,
    collision_world: &CollisionWorld,
    open_barriers: &[BarrierKindId],
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
        let victim_center = character_hitbox_center(*pos, gameplay.player.physics());
        let Some(falloff) = visible_blast_falloff(
            spec.center,
            victim_center,
            spec.damage.radius,
            collision_world,
            open_barriers,
        ) else {
            continue;
        };
        if !invincible {
            apply_damage(&mut health, spec.damage.max_damage * falloff);
            if health.0 <= 0.0 {
                outcome.dead_players.push(DeadPlayer {
                    id: *id,
                    entity,
                    pos: *pos,
                });
                continue;
            }
        }

        vertical_velocity.0 += movement.knockback.up_speed * falloff;
        accumulate_impulse(
            player_impulses,
            *id,
            entity,
            Some(&*knockback),
            planar_shove(spec.center, victim_center, falloff, movement.knockback.max_speed),
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
        let actor_physics = gameplay.expect_actor(&info.spawn_kind).physics();
        let victim_center = character_hitbox_center(*pos, actor_physics);
        let Some(falloff) = visible_blast_falloff(
            spec.center,
            victim_center,
            spec.damage.radius,
            collision_world,
            open_barriers,
        ) else {
            continue;
        };
        apply_damage(&mut health, spec.damage.max_damage * falloff);
        if health.0 <= 0.0 {
            outcome.dead_actors.push(DeadActor {
                id: *id,
                entity,
                pos: *pos,
            });
            continue;
        }

        if info.anchor.is_some() {
            continue;
        }
        vertical_velocity.0 += movement.knockback.up_speed * falloff;
        accumulate_impulse(
            actor_impulses,
            *id,
            entity,
            knockback,
            planar_shove(spec.center, victim_center, falloff, movement.knockback.max_speed),
            falloff,
        );
    }

    outcome
}

pub(super) fn accumulate_impulse<Id: std::hash::Hash + Eq + Copy>(
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
    let max_speed = context.map_settings.movement.knockback.max_speed * 1.5;
    for (id, impulse) in impulses {
        if context.players.get(&id).is_some_and(|info| info.is_dead()) {
            continue;
        }
        let Ok((_, _, _, health, vertical_velocity, mut knockback)) = context.player_query.get_mut(impulse.entity)
        else {
            continue;
        };
        let velocity = impulse.velocity.clamp_length_max(max_speed);
        knockback.0 = velocity;
        let direction = impulse.blast_delta.normalize_or_zero();
        if let Some(info) = context.players.get(&id) {
            let _ = info
                .connection
                .channel
                .send(ServerToClient::Send(ServerMessage::PlayerBlast(SPlayerBlast {
                    id,
                    health: *health,
                    vertical_velocity: vertical_velocity.0,
                    velocity_x: velocity.x,
                    velocity_z: velocity.z,
                    hit_dir_x: direction.x,
                    hit_dir_z: direction.z,
                    strength: impulse.strength,
                })));
        }
    }
}

fn apply_actor_impulses(context: &mut ExplosionContext, impulses: HashMap<ActorId, AccumulatedImpulse>) {
    let max_speed = context.map_settings.movement.knockback.max_speed * 1.5;
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

fn visible_blast_falloff(
    center: Vec3,
    target: Vec3,
    radius: f32,
    collision_world: &CollisionWorld,
    open_barriers: &[BarrierKindId],
) -> Option<f32> {
    let distance_squared = center.distance_squared(target);
    if distance_squared >= radius * radius {
        return None;
    }
    if !collision_world.attack_path_clear(center, target, open_barriers) {
        return None;
    }
    Some(blast_falloff_at_distance(distance_squared.sqrt(), radius))
}

fn planar_shove(center: Vec3, target: Vec3, falloff: f32, max_speed: f32) -> Vec3 {
    Vec3::new(target.x - center.x, 0.0, target.z - center.z).normalize_or_zero() * max_speed * falloff
}

pub(super) fn blast_falloff_at_distance(distance: f32, radius: f32) -> f32 {
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

fn source_description(source: &BlastSource, players: &PlayerMap) -> String {
    match source {
        BlastSource::Player(id) => format!("{}'s death explosion", players.describe(id)),
        BlastSource::Actor { id, kind } => format!("{kind}#{}'s explosion", id.0),
        BlastSource::Missile(id) => format!("{}'s missile explosion", players.describe(id)),
    }
}
