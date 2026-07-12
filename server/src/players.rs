use bevy::prelude::*;

use super::characters::{generate_player_spawn_position, spawn_face_direction};
use super::combat::kill_player;
use super::network::broadcast_to_all;
use crate::resources::{MapConfig, PlayerMap};
use common::{
    config::GameplayConfig,
    constants::{CHARACTER_FALL_DEATH_Y, CHARACTER_GROUND_SNAP_DISTANCE, PHYSICS_EPSILON, TICK_SECS},
    health::apply_damage,
    map_geometry::MapGeometry,
    physics::{CharacterVerticalVelocity, CollisionWorld},
    protocol::{
        FaceDirection, Health, MapSettings, PlayerId, PlayerMarker, PlayerMoveIntent, Position, SPlayerFallDamage,
        ServerMessage,
    },
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
    map_config: Res<MapConfig>,
    map_geometry: Res<MapGeometry>,
    collision_world: Res<CollisionWorld>,
    player_query: Query<(Entity, &PlayerId, &Position), With<PlayerMarker>>,
) {
    for (entity, id, pos) in player_query.iter() {
        if pos.y >= CHARACTER_FALL_DEATH_Y {
            continue;
        }
        // Skip players already dead this tick (e.g. killed by a projectile
        // before falling out of the world).
        if players.get(id).is_some_and(|info| info.is_dead()) {
            continue;
        }
        if server_gameplay_config.player.invincible {
            // Debug invincibility turns the void fall into a silent teleport
            // home: no `SPlayerDeath` (no banner, no kill feed), no per-life
            // state loss — keys, power-ups, and score all survive. The client
            // reconciliation snaps to the new position on the next snapshot.
            let occupied_positions: Vec<Position> = player_query
                .iter()
                .filter(|(other, _, other_pos)| *other != entity && other_pos.y >= CHARACTER_FALL_DEATH_Y)
                .map(|(_, _, other_pos)| *other_pos)
                .collect();
            let spawn_pos = generate_player_spawn_position(
                &map_config,
                &map_geometry,
                &collision_world,
                &occupied_positions,
                gameplay_config.player.physics(),
            );
            info!(
                "{:?} fell out of the world while invincible; teleporting to {:?}",
                id, spawn_pos
            );
            commands.entity(entity).insert((
                spawn_pos,
                FaceDirection(spawn_face_direction(&spawn_pos)),
                CharacterVerticalVelocity::default(),
            ));
            if let Some(info) = players.get_mut(id) {
                // The aborted fall must not register as an impact.
                info.peak_fall_speed = 0.0;
                info.fall_peak_y = f32::NEG_INFINITY;
            }
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
        let face_dir = spawn_face_direction(&pos);
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

// Velocity-implied distance may exceed the actual drop beyond this before
// the landing is logged as a phantom fall.
const PHANTOM_FALL_TRIPWIRE_SLACK: f32 = 5.0;

// Apply impact damage on landing from a fall. The highest Y of the current
// airborne window is tracked on `PlayerInfo.fall_peak_y`; when the player
// transitions to grounded (current vy clears the small negative threshold),
// damage lerps from 0 at `safe_fall_distance` to `max_health` at
// `lethal_fall_distance`, clamped past lethal. The fall distance requires
// BOTH a real drop AND matching impact energy — min(drop, velocity-implied):
//   * the drop bound kills phantom falls (the support probe can miss at a
//     ledge lip while the collider holds the body, so vy accumulates to
//     terminal with no displacement);
//   * the energy bound keeps anti-gravity landings soft (a low-gravity jump
//     tops out near lethal height but lands at only jump speed).
//
// Runs after `characters_movement_system` so it observes the *post-step*
// `CharacterVerticalVelocity` (i.e. 0 on the impact tick because the floor
// resolved the contact). Skipped entirely under debug invincibility.
pub fn players_fall_damage_system(
    mut commands: Commands,
    mut players: ResMut<PlayerMap>,
    gameplay_config: Res<GameplayConfig>,
    server_gameplay_config: Res<crate::config::ServerGameplayConfig>,
    map_settings: Res<MapSettings>,
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
            let drop = (info.fall_peak_y - pos.y).max(0.0);
            let velocity_implied = velocity_implied_fall_distance(info.peak_fall_speed, map_settings.gravity);
            // Tripwire for the phantom-fall physics hang: on a genuine fall
            // the velocity-implied distance matches the displacement.
            if velocity_implied > drop + PHANTOM_FALL_TRIPWIRE_SLACK {
                warn!(
                    "{:?} phantom fall: velocity implies {velocity_implied:.1}m, actual drop {drop:.1}m",
                    id
                );
            }
            let fall_distance = effective_fall_distance(drop, info.peak_fall_speed, map_settings.gravity);
            info.peak_fall_speed = 0.0;
            info.fall_peak_y = f32::NEG_INFINITY;

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
                    let _ = info.channel.send(ServerToClient::Send(ServerMessage::PlayerFallDamage(
                        SPlayerFallDamage {
                            id: *id,
                            health: *health,
                        },
                    )));
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
            info.fall_peak_y = info.fall_peak_y.max(pos.y);
        }
    }
}

// Normal-gravity distance equivalent to the landing's impact energy, with
// two corrections so JSON values can stay semantic ("level heights"):
//   1. `+gravity * TICK_SECS` to the impact speed —
//      `peak_fall_speed` is captured at end-of-tick *before* the impact
//      tick, so it misses one gravity application.
//   2. `+CHARACTER_GROUND_SNAP_DISTANCE` — the last ~0.5 m of every fall is
//      "snapped" by the character controller (vy → 0, no further gravity),
//      so naive `v²/2g` undercounts by exactly that snap distance.
// `gravity` is the map's normal-gravity magnitude, even for anti-gravity
// falls — the energy bound is what keeps those landings soft.
fn velocity_implied_fall_distance(peak_fall_speed: f32, gravity: f32) -> f32 {
    let impact_speed = peak_fall_speed + gravity * TICK_SECS;
    impact_speed.powi(2) / (2.0 * gravity) + CHARACTER_GROUND_SNAP_DISTANCE
}

// Fall distance that damage is charged for: min(actual drop, energy
// equivalent). See `players_fall_damage_system` header for the why.
fn effective_fall_distance(drop: f32, peak_fall_speed: f32, gravity: f32) -> f32 {
    drop.min(velocity_implied_fall_distance(peak_fall_speed, gravity))
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

    // Matches the shipping map's normal-gravity setting.
    const TEST_GRAVITY: f32 = 25.0;

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

    #[test]
    fn antigravity_jump_lands_below_safe_distance() {
        // Anti-gravity jump: apex ≈ 14.4 m of drop, but the landing speed is
        // only PLAYER_JUMP_SPEED (12 m/s) — an energy equivalent of ~3.8 m,
        // well under the 8 m safe threshold.
        let distance = effective_fall_distance(14.4, 12.0, TEST_GRAVITY);
        assert!(distance < 8.0, "expected soft landing, got {distance}m");
        assert!(distance > 3.0);
    }

    #[test]
    fn phantom_fall_charges_zero_distance() {
        // Terminal velocity fabricated while standing: no drop, no damage.
        assert_eq!(effective_fall_distance(0.0, 50.0, TEST_GRAVITY), 0.0);
    }

    #[test]
    fn genuine_fall_distance_passes_through() {
        // A real 13.2 m fall: impact speed √(2g·12.7) ≈ 25.2 m/s, captured
        // one tick early (−g·dt). Drop and energy agree, so min ≈ drop.
        let impact_speed = (2.0 * TEST_GRAVITY * (13.2 - CHARACTER_GROUND_SNAP_DISTANCE)).sqrt();
        let peak_fall_speed = impact_speed - TEST_GRAVITY * TICK_SECS;
        let distance = effective_fall_distance(13.2, peak_fall_speed, TEST_GRAVITY);
        assert!((distance - 13.2).abs() < 0.1, "expected ≈13.2m, got {distance}m");
    }
}
