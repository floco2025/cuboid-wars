use bevy::prelude::*;

use super::{Invincibility, PlayerMap, place_player_body, player_spawn_destination, spawn_zone_destination};
use crate::{
    combat::{DeathSource, PendingExplosions, apply_damage, kill_player},
    config::{FallDamageConfig, ServerGameplayConfig},
    map::MapConfig,
    portals::PortalAssignments,
};
use common::{
    config::GameplayConfig,
    constants::CHARACTER_FALL_DEATH_Y,
    map::Carriers,
    physics::CollisionWorld,
    protocol::{
        Health, MapLayout, MapSettings, PlayerId, PlayerMarker, Position, SPlayerFallDamage, ServerMessage, ServerTick,
    },
};

// Crushing and void falls reported by the owner; an invincible void fall is rescued instead.
pub fn players_fatal_outcomes_system(
    mut commands: Commands,
    tick: Res<ServerTick>,
    portal_assignments: Res<PortalAssignments>,
    mut players: ResMut<PlayerMap>,
    mut pending_explosions: ResMut<PendingExplosions>,
    gameplay_config: Res<GameplayConfig>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    invincibility: Res<Invincibility>,
    map_config: Res<MapConfig>,
    map_layout: Res<MapLayout>,
    carriers: Res<Carriers>,
    collision_world: Res<CollisionWorld>,
    player_query: Query<(Entity, &PlayerId, &Position, &Health), With<PlayerMarker>>,
) {
    // Destinations already claimed this tick: the relocations are queued, so
    // the query still shows those players in the void.
    let mut rescued: Vec<Position> = Vec::new();
    for (entity, id, pos, health) in player_query.iter() {
        let Some(info) = players.get_mut(id).filter(|info| !info.is_dead()) else {
            continue;
        };
        let crushed = info.life.outcomes.crushed.take();
        let fell_out_of_world = std::mem::take(&mut info.life.outcomes.fell_out_of_world);
        let saved_checkpoint = info.session.checkpoint;
        if let Some(pos) = crushed.filter(|_| !invincibility.0) {
            info!("{} was crushed by moving geometry at {:?}", players.describe(id), pos);
            kill_player(
                &mut commands,
                &mut players,
                *id,
                entity,
                pos,
                server_gameplay_config.player.respawn_secs,
                DeathSource::Crushed,
                &server_gameplay_config,
                &mut pending_explosions,
            );
            continue;
        }
        if !fell_out_of_world {
            continue;
        }
        if invincibility.0 {
            // Void rescue preserves equipment and score.
            let occupied_positions: Vec<Position> = player_query
                .iter()
                .filter(|(other, _, other_pos, _)| *other != entity && other_pos.y >= CHARACTER_FALL_DEATH_Y)
                .map(|(_, _, other_pos, _)| *other_pos)
                .chain(rescued.iter().copied())
                .collect();
            let physics = gameplay_config.player.physics();
            // The saved checkpoint, like a respawn; a spawn zone when it is blocked.
            let spawn = player_spawn_destination(
                &map_config,
                &map_layout.checkpoints,
                &carriers,
                &collision_world,
                &occupied_positions,
                physics,
                saved_checkpoint,
            )
            .unwrap_or_else(|| {
                spawn_zone_destination(
                    &map_config,
                    &map_layout.checkpoints,
                    &carriers,
                    &collision_world,
                    &occupied_positions,
                    physics,
                )
            });
            rescued.push(spawn.pos);
            info!(
                "{} fell out of the world while invincible; teleporting to {:?}",
                players.describe(id),
                spawn.pos
            );
            if let Some(info) = players.get_mut(id) {
                info.advance_body();
            }
            place_player_body(
                &mut commands,
                &mut players,
                *id,
                entity,
                &spawn,
                *health,
                tick.0,
                portal_assignments.get(id),
            );
            continue;
        }
        info!("{} fell out of the world", players.describe(id));
        kill_player(
            &mut commands,
            &mut players,
            *id,
            entity,
            *pos,
            server_gameplay_config.player.respawn_secs,
            DeathSource::Void,
            &server_gameplay_config,
            &mut pending_explosions,
        );
    }
}

// ============================================================================
// Players Fall Damage System
// ============================================================================

// Below this damage, skip the impact effect entirely. The lerp produces
// near-zero damage just past `safe_distance` due to float / tick
// noise; without this gate the client would get a wiggle for every tiny
// step off a curb.
// Keep this cutoff in sync with tools/map_editor/jump_reach.py.
const FALL_DAMAGE_EMIT_THRESHOLD: f32 = 1.0;

pub fn players_fall_damage_system(
    mut commands: Commands,
    mut players: ResMut<PlayerMap>,
    mut pending_explosions: ResMut<PendingExplosions>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    invincibility: Res<Invincibility>,
    map_settings: Res<MapSettings>,
    fall: Res<FallDamageConfig>,
    mut player_query: Query<(Entity, &PlayerId, &mut Health), With<PlayerMarker>>,
) {
    let invincible = invincibility.0;
    let max_health = server_gameplay_config.combat.health.player.max;
    let respawn_secs = server_gameplay_config.player.respawn_secs;

    for (entity, id, mut health) in player_query.iter_mut() {
        let Some(info) = players.get_mut(id) else { continue };
        if info.is_dead() {
            continue;
        }

        let landings = std::mem::take(&mut info.life.outcomes.landings);
        for impact in landings {
            if players.get(id).is_none_or(|info| info.is_dead()) {
                break;
            }
            let pos = &impact.pos;
            let fall_distance = fall_distance_for_speed(impact.impact_speed, map_settings.movement.gravity);
            if fall_distance <= fall.safe_distance {
                continue;
            }

            let damage = fall_damage_for_distance(fall_distance, fall.safe_distance, fall.lethal_distance, max_health);
            // Skip the entire emission path for negligible damage —
            // the safe-threshold lerp produces near-zero damage just
            // past `safe_distance` from floating-point slack and
            // discrete-tick noise. No HUD update or camera wiggle for
            // a fall the player barely registers.
            if damage < FALL_DAMAGE_EMIT_THRESHOLD {
                continue;
            }
            if !invincible {
                apply_damage(&mut health, damage);
            }
            // Unicast `SPlayerFallDamage` to the victim so the HUD health bar
            // and vertical camera wiggle land on the impact frame
            // instead of waiting for the next snapshot. The fatal-fall
            // case additionally surfaces `SPlayerDeath` via
            // `kill_player` below.
            if let Some(info) = players.get(id) {
                let _ = info
                    .connection
                    .channel
                    .send(ServerMessage::PlayerFallDamage(SPlayerFallDamage {
                        id: *id,
                        generation: info.session.generation,
                        health: *health,
                    }));
            }
            if health.0 <= 0.0 {
                info!(
                    "{} died from fall (distance {:.1}m)",
                    players.describe(id),
                    fall_distance
                );
                kill_player(
                    &mut commands,
                    &mut players,
                    *id,
                    entity,
                    *pos,
                    respawn_secs,
                    DeathSource::Fall,
                    &server_gameplay_config,
                    &mut pending_explosions,
                );
            }
        }
    }
}

// Express impact energy as a normal-gravity drop so map distance thresholds retain their meaning.
fn fall_distance_for_speed(impact_speed: f32, normal_gravity: f32) -> f32 {
    impact_speed * impact_speed / (2.0 * normal_gravity)
}

// Lerp damage between `safe_distance` (0 dmg) and `lethal_distance`
// (full health), clamping the falloff beyond the lethal endpoint.
// Keep this curve in sync with tools/map_editor/jump_reach.py::FallSettings.damage_fraction.
fn fall_damage_for_distance(distance: f32, safe: f32, lethal: f32, max_health: f32) -> f32 {
    f32::inverse_lerp(safe, lethal, distance).clamp(0.0, 1.0) * max_health
}

#[cfg(test)]
#[path = "tests/falling.rs"]
mod tests;
