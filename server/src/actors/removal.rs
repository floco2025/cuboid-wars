use bevy::prelude::*;

use crate::{
    combat::{ActorDeathQuery, apply_actor_explosion_damage, kill_player},
    config::ServerGameplayConfig,
    network::broadcast_to_all,
    resources::{ActorMap, PlayerMap},
};
use common::{
    config::GameplayConfig,
    constants::CHARACTER_FALL_DEATH_Y,
    protocol::{ActorId, ActorMarker, Health, PlayerId, PlayerMarker, Position, SActorDeath, ServerMessage},
};

// Despawn actors that have either fallen below the death threshold or had
// their health reduced to zero. Health-zero death also applies blast damage
// to nearby characters and broadcasts the explosion VFX. Falls are silent —
// they were teleports before, so the asymmetry is preserved.
//
// Actor entities are despawned outright; the `actor_respawn_system` will
// pick the missing slots up next tick and create replacements.
pub fn actor_removal_system(
    mut commands: Commands,
    mut actors: ResMut<ActorMap>,
    mut players: ResMut<PlayerMap>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    gameplay_config: Res<GameplayConfig>,
    mut player_query: Query<(Entity, &PlayerId, &Position, &mut Health), (With<PlayerMarker>, Without<ActorMarker>)>,
    mut query: ActorDeathQuery,
) {
    let mut deaths: Vec<ActorDeath> = Vec::new();
    for (entity, id, pos, _, _, health) in query.iter() {
        let Some(info) = actors.get(id) else {
            continue;
        };
        // Clone `spawn_kind` only on a confirmed death — the common live-actor
        // path runs every tick and must not allocate.
        if pos.y < CHARACTER_FALL_DEATH_Y {
            deaths.push(ActorDeath {
                entity,
                id: *id,
                pos: *pos,
                spawn_kind: info.spawn_kind.clone(),
                killer: None,
                cause: DeathCause::Fall,
            });
        } else if health.0 <= 0.0 {
            deaths.push(ActorDeath {
                entity,
                id: *id,
                pos: *pos,
                spawn_kind: info.spawn_kind.clone(),
                killer: info.last_damager,
                cause: DeathCause::Killed,
            });
        }
    }

    if deaths.is_empty() {
        return;
    }

    let respawn_delay_secs = gameplay_config.player.respawn_delay_secs;
    for death in deaths {
        if matches!(death.cause, DeathCause::Killed) {
            // Per-kind config: actor kinds are cross-validated against the map
            // at startup, so `validated_actor` cannot panic here.
            let kind_server_config = server_gameplay_config.validated_actor(&death.spawn_kind);
            let dead_players = apply_actor_explosion_damage(
                death.pos,
                death.entity,
                &death.spawn_kind,
                &kind_server_config.combat.explosion,
                &gameplay_config,
                server_gameplay_config.player.invincible,
                &players,
                &mut player_query,
                &mut query,
            );
            let killer_score = death.killer.and_then(|kid| players.get(&kid)).map(|info| info.score);
            broadcast_to_all(
                &players,
                ServerMessage::ActorDeath(SActorDeath {
                    id: death.id,
                    pos: death.pos,
                    killer: death.killer,
                    killer_score,
                }),
            );
            for (player_id, entity) in dead_players {
                info!("{:?} died in {:?}'s explosion", player_id, death.id);
                // Explosion deaths award no kill credit: the victim takes the
                // death penalty as an environmental death, and the cue reports
                // no killer (a solo death in the feed). Apply the penalty before
                // `kill_player` snapshots the victim's score for `SPlayerDeath`.
                if let Some(victim) = players.get_mut(&player_id) {
                    victim.score += server_gameplay_config.scoring.player_death;
                }
                kill_player(&mut commands, &mut players, player_id, entity, respawn_delay_secs, None);
            }
        } else {
            info!("{:?} fell and despawned at {:?}", death.id, death.pos);
        }
        // Death is unconditional despawn. For respawning kinds, the throttle
        // in `actor_respawn_system` ticks down only while the slot is short
        // of its quota, so dropping a live actor here is what causes the
        // next replacement to be scheduled. For non-respawning kinds, the
        // respawn system skips the zone entirely — no replacement happens.
        commands.entity(death.entity).despawn();
        actors.remove(&death.id);
    }
}

#[derive(Copy, Clone)]
enum DeathCause {
    Fall,
    Killed,
}

struct ActorDeath {
    entity: Entity,
    id: ActorId,
    pos: Position,
    spawn_kind: String,
    killer: Option<PlayerId>,
    cause: DeathCause,
}
