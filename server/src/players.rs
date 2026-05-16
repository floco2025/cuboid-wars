use bevy::prelude::*;

use super::characters::generate_player_spawn_position;
use super::combat::kill_player;
use super::network::broadcast_to_all;
use crate::resources::{MapConfig, PlayerMap};
use common::{
    config::GameplayConfig,
    constants::{
        CHARACTER_FALL_DEATH_Y, CHARACTER_GRAVITY, CHARACTER_GROUND_SNAP_DISTANCE, PHYSICS_EPSILON, TICK_PERIOD_SECS,
    },
    health::apply_damage,
    map_geometry::MapGeometry,
    physics::{CharacterVerticalVelocity, CollisionWorld},
    protocol::{FaceDirection, Health, PlayerId, PlayerMarker, PlayerMoveIntent, Position, SFallDamage, ServerMessage},
};

use crate::net::ServerToClient;

// ============================================================================
// Players Status Timers System
// ============================================================================

// System to count down player power-up and stun timers
pub fn players_status_timers_system(time: Res<Time>, mut players: ResMut<PlayerMap>) {
    let delta = time.delta_secs();

    let mut status_messages = Vec::new();

    for (player_id, player_info) in players.iter_mut() {
        let old_status = player_info.status(*player_id);

        player_info.tick_timers(delta);

        let new_status = player_info.status(*player_id);

        if old_status != new_status {
            status_messages.push(new_status);
        }
    }

    // Send status updates to all clients
    for msg in status_messages {
        broadcast_to_all(&players, ServerMessage::PlayerStatus(msg));
    }
}

// ============================================================================
// Players Fall Death System
// ============================================================================

// Detect players that have fallen below the death threshold and kill them
// using the same flow as any other death (clear per-life state, arm respawn
// timer, despawn entity). The respawn system brings them back at a fresh
// spawn-zone cell after `respawn_delay_secs`.
pub fn players_fall_death_system(
    mut commands: Commands,
    mut players: ResMut<PlayerMap>,
    gameplay_config: Res<GameplayConfig>,
    server_gameplay_config: Res<crate::config::ServerGameplayConfig>,
    player_query: Query<(Entity, &PlayerId, &Position), With<PlayerMarker>>,
) {
    // Debug invincibility shorts the whole system — a player can keep
    // falling indefinitely. That's the intended trade-off; the only
    // alternative would be a teleport, which is beyond "no damage".
    if server_gameplay_config.player.invincible {
        return;
    }
    for (entity, id, pos) in player_query.iter() {
        if pos.y >= CHARACTER_FALL_DEATH_Y {
            continue;
        }
        // Skip players already dead this tick (e.g. killed by a projectile
        // before falling out of the world).
        if players.get(id).is_some_and(|info| info.is_dead()) {
            continue;
        }
        info!("{:?} fell and died at {:?}", id, pos);
        kill_player(
            &mut commands,
            &mut players,
            *id,
            entity,
            *pos,
            gameplay_config.player.respawn_delay_secs,
            None,
        );
    }
}

// ============================================================================
// Players Respawn System
// ============================================================================

// Tick each dead player's respawn timer. When it elapses, spawn a fresh entity
// at a new spawn-zone cell with full health. Per-life state (power-ups, keys,
// stun) was already cleared at death; score is preserved.
//
// The new entity replaces the (already despawned) old one; the next `SSnapshot`
// will carry the player at their new position and the client's snapshot diff
// resurrects their visual.
pub fn players_respawn_system(
    mut commands: Commands,
    time: Res<Time>,
    mut players: ResMut<PlayerMap>,
    map_config: Res<MapConfig>,
    map_geometry: Res<MapGeometry>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    player_query: Query<&Position, With<PlayerMarker>>,
) {
    let delta = time.delta_secs();

    let mut occupied_positions: Vec<Position> = player_query.iter().copied().collect();
    let mut to_respawn: Vec<PlayerId> = Vec::new();

    for (id, info) in players.iter_mut() {
        let Some(timer) = info.death_timer.as_mut() else {
            continue;
        };
        *timer -= delta;
        if *timer <= 0.0 {
            to_respawn.push(*id);
        }
    }

    for id in to_respawn {
        let pos = generate_player_spawn_position(
            &map_config,
            &map_geometry,
            &collision_world,
            &occupied_positions,
            gameplay_config.player.physics(),
        );
        let face_dir = (-pos.x).atan2(-pos.z);
        let move_intent = PlayerMoveIntent::Idle;
        let entity = commands
            .spawn((
                PlayerMarker,
                id,
                pos,
                move_intent,
                FaceDirection(face_dir),
                CharacterVerticalVelocity::default(),
                Health(gameplay_config.player.health().max),
            ))
            .id();

        if let Some(info) = players.get_mut(&id) {
            info.entity = entity;
            info.death_timer = None;
        }

        occupied_positions.push(pos);
        info!("{:?} respawned at {:?}", id, pos);
    }
}

// ============================================================================
// Players Fall Damage System
// ============================================================================

// Below this damage, skip the impact effect entirely. The lerp produces
// near-zero damage just past `safe_fall_distance` due to float / tick
// noise; without this gate the client would get a wiggle for every tiny
// step off a curb.
const FALL_DAMAGE_EMIT_THRESHOLD: f32 = 1.0;

// Apply impact damage on landing from a fall. The peak |vy| during the
// uninterrupted fall is tracked on `PlayerInfo.peak_fall_speed`; when the
// player transitions to grounded (current vy clears the small negative
// threshold), the equivalent fall distance is `peak² / (2·gravity)` and
// damage lerps from 0 at `safe_fall_distance` to `max_health` at
// `lethal_fall_distance`, clamped past lethal.
//
// Runs after `characters_movement_system` so it observes the *post-step*
// `CharacterVerticalVelocity` (i.e. 0 on the impact tick because the floor
// resolved the contact). Skipped entirely under debug invincibility.
pub fn players_fall_damage_system(
    mut commands: Commands,
    mut players: ResMut<PlayerMap>,
    gameplay_config: Res<GameplayConfig>,
    server_gameplay_config: Res<crate::config::ServerGameplayConfig>,
    mut player_query: Query<
        (Entity, &PlayerId, &Position, &CharacterVerticalVelocity, &mut Health),
        With<PlayerMarker>,
    >,
) {
    if server_gameplay_config.player.invincible {
        return;
    }
    let fall = server_gameplay_config.player.fall_damage;
    let max_health = gameplay_config.player.health().max;
    let respawn_delay_secs = gameplay_config.player.respawn_delay_secs;

    for (entity, id, pos, motion, mut health) in player_query.iter_mut() {
        let Some(info) = players.get_mut(id) else { continue };
        if info.is_dead() {
            continue;
        }

        let current_vy = motion.0;
        let is_grounded = current_vy > -PHYSICS_EPSILON;

        if is_grounded && info.peak_fall_speed > 0.0 {
            // Reconstruct effective fall distance from the captured peak
            // speed, with two corrections so JSON values can stay semantic
            // ("level heights"):
            //   1. `+CHARACTER_GRAVITY * TICK_PERIOD_SECS` to the impact
            //      speed — `peak_fall_speed` is captured at end-of-tick
            //      *before* the impact tick, so it misses one gravity
            //      application that the physics applies in the impact tick
            //      itself before the floor zeroes vy.
            //   2. `+CHARACTER_GROUND_SNAP_DISTANCE` to the distance — the
            //      last ~0.5 m of every fall is "snapped" by the character
            //      controller (vy → 0, no further gravity), so naive
            //      `v²/2g` undercounts by exactly that snap distance.
            let impact_speed = info.peak_fall_speed + CHARACTER_GRAVITY * TICK_PERIOD_SECS;
            let fall_distance = impact_speed.powi(2) / (2.0 * CHARACTER_GRAVITY) + CHARACTER_GROUND_SNAP_DISTANCE;
            info.peak_fall_speed = 0.0;

            if fall_distance > fall.safe_fall_distance {
                let damage = fall_damage_for_distance(
                    fall_distance,
                    fall.safe_fall_distance,
                    fall.lethal_fall_distance,
                    max_health,
                );
                // Skip the entire emission path for negligible damage —
                // the safe-threshold lerp produces near-zero damage just
                // past `safe_fall_distance` from floating-point slack and
                // discrete-tick noise. No HUD update or camera wiggle for
                // a fall the player barely registers.
                if damage < FALL_DAMAGE_EMIT_THRESHOLD {
                    continue;
                }
                apply_damage(&mut health, damage);
                // Unicast `SFallDamage` to the victim so the HUD health bar
                // and vertical camera wiggle land on the impact frame
                // instead of waiting for the next snapshot. The fatal-fall
                // case additionally surfaces `SPlayerDeath` via
                // `kill_player` below.
                if let Some(info) = players.get(id) {
                    let _ = info
                        .channel
                        .send(ServerToClient::Send(ServerMessage::FallDamage(SFallDamage {
                            id: *id,
                            health: *health,
                        })));
                }
                if health.0 <= 0.0 {
                    info!("{:?} died from fall (distance {:.1}m)", id, fall_distance);
                    kill_player(&mut commands, &mut players, *id, entity, *pos, respawn_delay_secs, None);
                }
            }
        } else if !is_grounded {
            // In the air (rising or falling). Track only downward speed.
            let downward_speed = (-current_vy).max(0.0);
            if downward_speed > info.peak_fall_speed {
                info.peak_fall_speed = downward_speed;
            }
        }
    }
}

// Lerp damage between `safe_fall_distance` (0 dmg) and `lethal_fall_distance`
// (full health), clamping the falloff beyond the lethal endpoint.
fn fall_damage_for_distance(distance: f32, safe: f32, lethal: f32, max_health: f32) -> f32 {
    let t = ((distance - safe) / (lethal - safe)).clamp(0.0, 1.0);
    t * max_health
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fall_damage_zero_at_safe_distance() {
        assert_eq!(fall_damage_for_distance(4.0, 4.0, 12.0, 100.0), 0.0);
        assert_eq!(fall_damage_for_distance(3.0, 4.0, 12.0, 100.0), 0.0);
    }

    #[test]
    fn fall_damage_lethal_at_lethal_distance() {
        assert_eq!(fall_damage_for_distance(12.0, 4.0, 12.0, 100.0), 100.0);
    }

    #[test]
    fn fall_damage_lerps_midpoint() {
        // (8 - 4) / (12 - 4) = 0.5 → 50 dmg
        assert_eq!(fall_damage_for_distance(8.0, 4.0, 12.0, 100.0), 50.0);
    }

    #[test]
    fn fall_damage_saturates_past_lethal() {
        assert_eq!(fall_damage_for_distance(100.0, 4.0, 12.0, 100.0), 100.0);
    }
}
