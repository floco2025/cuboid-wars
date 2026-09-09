use bevy::prelude::*;

use super::PendingExplosions;
use crate::{
    actors::ActorMap,
    config::{FeedConfig, ServerGameplayConfig},
    network::{DeathCause, FeedAudience, FeedEvent, broadcast_to_all, emit_feed},
    players::PlayerMap,
    quests::{QuestBoard, QuestCatalog, QuestEvent, record_event},
};
use common::{
    health::apply_damage,
    protocol::{ActorId, Health, PlayerDeathEffect, PlayerId, Position, SActorDeath, SPlayerDeath, ServerMessage},
};

// What killed a player, by id. `kill_player` derives both the kill credit
// (`SPlayerDeath.killer`) and the feed's `DeathCause` from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeathSource {
    Shot(PlayerId),
    Missile(PlayerId),
    Beam { kind: String },
    PlayerBlast(PlayerId),
    ActorBlast { kind: String },
    // A lethal landing.
    Fall,
    // Fell out of the world.
    Void,
    // Caught inside a carrier's geometry.
    Crushed,
    Admin,
}

// Credit goes to a shooter other than the victim who is still connected;
// beams, death blasts, falls, crushes, and admin kills credit nobody.
#[must_use]
pub fn kill_credit(source: &DeathSource, victim: PlayerId, players: &PlayerMap) -> Option<PlayerId> {
    match source {
        DeathSource::Shot(by) | DeathSource::Missile(by) => (*by != victim && players.get(by).is_some()).then_some(*by),
        DeathSource::Beam { .. }
        | DeathSource::PlayerBlast(_)
        | DeathSource::ActorBlast { .. }
        | DeathSource::Fall
        | DeathSource::Void
        | DeathSource::Crushed
        | DeathSource::Admin => None,
    }
}

fn death_cause(source: &DeathSource, victim: PlayerId, players: &PlayerMap) -> DeathCause {
    match source {
        DeathSource::Shot(by) if *by == victim => DeathCause::SelfShot,
        DeathSource::Shot(by) => DeathCause::Shot {
            by: players.display_name(by),
        },
        DeathSource::Missile(by) if *by == victim => DeathCause::SelfMissile,
        DeathSource::Missile(by) => DeathCause::Missile {
            by: players.display_name(by),
        },
        DeathSource::Beam { kind } => DeathCause::Beam { kind: kind.clone() },
        DeathSource::PlayerBlast(by) => DeathCause::PlayerBlast {
            by: players.display_name(by),
        },
        DeathSource::ActorBlast { kind } => DeathCause::ActorBlast { kind: kind.clone() },
        DeathSource::Fall | DeathSource::Void => DeathCause::Fall,
        DeathSource::Crushed => DeathCause::Crushed,
        DeathSource::Admin => DeathCause::Admin,
    }
}

// Run the death sequence for one player: replace its life with the dead
// lifecycle, charge the victim `scoring.player_death` and credit the killer
// `scoring.player_kill` (so every death path scores once and the cue carries
// the post-death scores), queue the death explosion (not for a void fall),
// despawn the entity, broadcast `SPlayerDeath` so clients run death-side
// effects on the impact tick instead of waiting a snapshot, and announce the
// feed line.
// `PlayerMap` captures actor-reset eligibility and arms the shared timer in group mode.
// Called from every code path that takes a player to zero health
// (projectile hits, beams, explosions, falls, `/kill`).
#[expect(
    clippy::too_many_arguments,
    reason = "the one-stop death sequence threads all death state"
)]
pub fn kill_player(
    commands: &mut Commands,
    players: &mut PlayerMap,
    id: PlayerId,
    entity: Entity,
    pos: Position,
    respawn_secs: f32,
    source: DeathSource,
    server_gameplay_config: &ServerGameplayConfig,
    pending_explosions: &mut PendingExplosions,
) {
    let killer = kill_credit(&source, id, players);
    if !players.begin_respawn(id, respawn_secs) {
        return;
    }
    let scoring = &server_gameplay_config.scoring;
    if let Some(victim) = players.get_mut(&id) {
        victim.session.score += scoring.player_death;
    }
    if let Some(killer_info) = killer.and_then(|killer| players.get_mut(&killer)) {
        killer_info.session.score += scoring.player_kill;
    }
    // Every death but a void fall detonates — `explosions_system` drains
    // the queue this tick and applies the blast. That deep a blast would
    // reach nothing, and the cue tells clients to show nothing.
    let explodes = !matches!(source, DeathSource::Void);
    if explodes {
        pending_explosions.push_player(id, pos);
    }
    commands.entity(entity).despawn();
    // Snapshot the post-death scores so the cue carries the early-apply
    // values (HUD bumps on impact tick rather than next snapshot).
    let victim_score = players.get(&id).map_or(0, |info| info.session.score);
    let killer_score = killer.and_then(|kid| players.get(&kid)).map(|info| info.session.score);
    broadcast_to_all(
        players,
        ServerMessage::PlayerDeath(SPlayerDeath {
            id,
            pos,
            killer,
            victim_score,
            killer_score,
            effect: if explodes {
                PlayerDeathEffect::Explosion
            } else {
                PlayerDeathEffect::VoidFall
            },
        }),
    );
    emit_feed(
        players,
        &server_gameplay_config.feed,
        FeedAudience::Everyone,
        FeedEvent::PlayerDied {
            name: players.display_name(&id),
            cause: death_cause(&source, id, players),
        },
    );
}

#[expect(clippy::too_many_arguments, reason = "the one-stop actor death sequence")]
pub fn kill_actor(
    commands: &mut Commands,
    actors: &mut ActorMap,
    players: &PlayerMap,
    pending_explosions: &mut PendingExplosions,
    feed: &FeedConfig,
    id: ActorId,
    entity: Entity,
    pos: Position,
    killer: Option<PlayerId>,
) -> bool {
    let Some(info) = actors.remove(&id) else {
        return false;
    };
    if let Some(killer_id) = killer {
        emit_feed(
            players,
            feed,
            FeedAudience::Everyone,
            FeedEvent::ActorDestroyed {
                name: players.display_name(&killer_id),
                kind: info.spawn_kind.clone(),
            },
        );
    }
    let killer_score = killer
        .and_then(|killer_id| players.get(&killer_id))
        .map(|player| player.session.score);
    broadcast_to_all(
        players,
        ServerMessage::ActorDeath(SActorDeath {
            id,
            pos,
            killer,
            killer_score,
        }),
    );
    pending_explosions.push_actor(id, entity, info.spawn_kind, pos);
    commands.entity(entity).despawn();
    true
}

// Award the shooter's actor-kill credit: the per-kind score bonus plus any
// actor-kills quest progress. No-op when the shooter has disconnected.
// Shared by projectile lethal hits and missile blasts so the two paths
// can't drift.
pub fn award_actor_kill(
    players: &mut PlayerMap,
    quest_board: &mut QuestBoard,
    quest_catalog: &QuestCatalog,
    shooter_id: PlayerId,
    kind: &str,
    server_gameplay_config: &ServerGameplayConfig,
) {
    let Some(shooter) = players.get_mut(&shooter_id) else {
        return;
    };
    shooter.session.score += server_gameplay_config
        .scoring
        .actor_kill
        .get(kind)
        .copied()
        .expect("actor kind missing from scoring.actor_kill");
    record_event(
        players,
        quest_board,
        quest_catalog,
        &server_gameplay_config.feed,
        QuestEvent::ActorKilled {
            player: shooter_id,
            kind,
        },
    );
}

// Apply one projectile hit to a player. Returns `true` when this hit drops
// the target's health to zero (and the target wasn't already dead) — the
// caller is responsible for running `kill_player`, which does the scoring.
pub fn apply_player_projectile_hit(
    players: &PlayerMap,
    target_id: PlayerId,
    target_health: &mut Health,
    server_gameplay_config: &ServerGameplayConfig,
    invincible: bool,
) -> bool {
    // The projectile system shouldn't find a dead player (entity is gone),
    // but guard so a stray hit on a queued-for-despawn entity can't redeath.
    if players.get(&target_id).is_some_and(|info| info.is_dead()) {
        return false;
    }

    // Debug invincibility: cosmetic `SPlayerHit` still fires (camera shake
    // for the victim, hit sound for the shooter), but health and score are
    // untouched and the hit cannot be lethal.
    if invincible {
        return false;
    }

    apply_damage(target_health, server_gameplay_config.combat.damage.projectile);
    target_health.0 <= 0.0
}

// Apply one tick of laser-beam contact to a player. `damage` is the per-tick
// amount (`beam_dps * dt`). Returns `true` when this tick drops the target to
// zero — the caller runs `kill_player`, which does the scoring.
pub fn apply_player_beam_damage(
    players: &PlayerMap,
    target_id: PlayerId,
    target_health: &mut Health,
    damage: f32,
    invincible: bool,
) -> bool {
    if players.get(&target_id).is_some_and(|info| info.is_dead()) {
        return false;
    }
    if invincible {
        return false;
    }
    apply_damage(target_health, damage);
    target_health.0 <= 0.0
}

// Apply one projectile hit to an actor. Returns `true` when this hit drops
// the target's health to zero — the caller awards the per-kind kill bonus
// (`scoring.actor_kill`).
pub fn apply_actor_projectile_hit(
    players: &mut PlayerMap,
    shooter_id: &PlayerId,
    kind: &str,
    target_health: &mut Health,
    server_gameplay_config: &ServerGameplayConfig,
) -> bool {
    // A dying actor's entity stays queryable until `actors_removal_system`
    // runs later in the tick; without this guard every further same-tick hit
    // would read the clamped 0 health as "lethal" and duplicate kill credit.
    if target_health.0 <= 0.0 {
        return false;
    }

    apply_damage(target_health, server_gameplay_config.combat.damage.projectile);

    if let Some(shooter_info) = players.get_mut(shooter_id) {
        shooter_info.session.score += server_gameplay_config
            .scoring
            .actor_hit
            .get(kind)
            .copied()
            .expect("actor kind missing from scoring.actor_hit");
    }

    target_health.0 <= 0.0
}
