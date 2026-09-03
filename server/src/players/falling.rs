use bevy::prelude::*;

use super::{Invincibility, PlayerMap};
use crate::characters::{generate_player_spawn_position, spawn_face_yaw};
use crate::combat::{DeathSource, PendingExplosions, kill_player};
use crate::config::ServerGameplayConfig;
use crate::map::MapConfig;
use crate::network::ServerToClient;
use common::constants::CHARACTER_FALL_DEATH_Y;
use common::{
    config::GameplayConfig,
    health::apply_damage,
    map::MapGeometry,
    physics::{CharacterSupport, CharacterVerticalVelocity, CollisionWorld},
    protocol::{FaceYaw, Health, MapSettings, PlayerId, PlayerMarker, Position, SPlayerFallDamage, ServerMessage},
};

#[derive(Debug, Clone, Copy)]
pub struct PlayerFallState {
    support: CharacterSupport,
    peak_y: f32,
}

impl Default for PlayerFallState {
    fn default() -> Self {
        Self {
            support: CharacterSupport::Airborne,
            peak_y: f32::NEG_INFINITY,
        }
    }
}

impl PlayerFallState {
    #[must_use]
    pub(crate) const fn support(&self) -> CharacterSupport {
        self.support
    }

    pub(crate) fn set_support(&mut self, support: CharacterSupport) {
        self.support = support;
    }

    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    fn update(&mut self, y: f32) -> Option<FallImpact> {
        if self.support == CharacterSupport::Airborne {
            self.peak_y = self.peak_y.max(y);
            return None;
        }

        let impact = (self.peak_y > y).then_some(FallImpact { drop: self.peak_y - y });
        self.peak_y = f32::NEG_INFINITY;
        impact
    }
}

// ============================================================================
// Players Fall Death System
// ============================================================================

// Detect players that have fallen below the death threshold and kill them
// using the same flow as any other death (clear per-life state, arm respawn
// timer, despawn entity). The respawn system brings them back at a fresh
// spawn-zone cell after `respawn_secs`.
pub fn players_fall_death_system(
    mut commands: Commands,
    mut players: ResMut<PlayerMap>,
    mut pending_explosions: ResMut<PendingExplosions>,
    gameplay_config: Res<GameplayConfig>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    invincibility: Res<Invincibility>,
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
        if invincibility.0 {
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
                "{} fell out of the world while invincible; teleporting to {:?}",
                players.describe(id),
                spawn_pos
            );
            commands.entity(entity).insert((
                spawn_pos,
                FaceYaw(spawn_face_yaw(&spawn_pos)),
                CharacterVerticalVelocity::default(),
            ));
            if let Some(info) = players.get_mut(id) {
                info.life.fall_state.reset();
            }
            continue;
        }
        info!("{} fell and died at {:?}", players.describe(id), pos);
        kill_player(
            &mut commands,
            &mut players,
            *id,
            entity,
            *pos,
            server_gameplay_config.player.respawn_secs,
            DeathSource::Fall,
            &server_gameplay_config.feed,
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
const FALL_DAMAGE_EMIT_THRESHOLD: f32 = 1.0;

// Apply impact damage on landing from a fall. The highest Y of the current
// airborne window is tracked by `PlayerFallState`; when the player
// reaches support (ground or ladder), damage lerps from 0 at
// `safe_distance` to `max_health` at
// `lethal_distance`, clamped past lethal. The charged distance is the
// actual drop, scaled by the gravity it fell under relative to the map's
// normal gravity:
//   * a phantom fall (velocity with no displacement — the support probe can
//     miss at a ledge lip while the collider holds the body) drops nothing,
//     so it charges nothing;
//   * a low-gravity fall lands as softly as a proportionally shorter
//     normal fall;
//   * and deliberately NOT the impact speed: the terminal-velocity clamp
//     caps that, which would make every fall survivable once the clamp is
//     low enough — height must stay lethal.
//
// Runs after `characters_movement_system` so it observes the support derived
// by that tick's shared movement step. Under debug invincibility the impact cue
// (`SPlayerFallDamage` → camera wiggle + sound) still fires; only the
// health loss — and therefore the lethal branch — is skipped.
pub fn players_fall_damage_system(
    mut commands: Commands,
    mut players: ResMut<PlayerMap>,
    mut pending_explosions: ResMut<PendingExplosions>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    invincibility: Res<Invincibility>,
    map_settings: Res<MapSettings>,
    mut player_query: Query<(Entity, &PlayerId, &Position, &mut Health), With<PlayerMarker>>,
) {
    let invincible = invincibility.0;
    let fall = server_gameplay_config.combat.damage.player_fall;
    let max_health = server_gameplay_config.combat.health.player.max;
    let respawn_secs = server_gameplay_config.player.respawn_secs;

    for (entity, id, pos, mut health) in player_query.iter_mut() {
        let Some(info) = players.get_mut(id) else { continue };
        if info.is_dead() {
            continue;
        }

        let fall_gravity = map_settings.gravity_for(info.has_low_gravity());
        let Some(impact) = info.life.fall_state.update(pos.y) else {
            continue;
        };

        let fall_distance = effective_fall_distance(impact.drop, fall_gravity, map_settings.movement.gravity);
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
                .send(ServerToClient::Send(ServerMessage::PlayerFallDamage(
                    SPlayerFallDamage {
                        id: *id,
                        health: *health,
                    },
                )));
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
                &server_gameplay_config.feed,
                &mut pending_explosions,
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct FallImpact {
    drop: f32,
}

// Fall distance that damage is charged for: the actual drop, scaled by the
// gravity it fell under relative to the map's normal gravity. See the
// `players_fall_damage_system` header for the why.
fn effective_fall_distance(drop: f32, fall_gravity: f32, normal_gravity: f32) -> f32 {
    drop * (fall_gravity / normal_gravity)
}

// Lerp damage between `safe_distance` (0 dmg) and `lethal_distance`
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
    fn airborne_tracking_records_the_apex() {
        let mut state = PlayerFallState::default();

        assert_eq!(state.update(8.0), None);
        assert_eq!(state.update(7.0), None);

        assert_eq!(state.peak_y, 8.0);
    }

    #[test]
    fn upward_airborne_motion_does_not_finish_the_tracked_fall() {
        let mut state = PlayerFallState {
            peak_y: 8.0,
            ..default()
        };

        let impact = state.update(7.0);

        assert_eq!(impact, None);
        assert_eq!(state.peak_y, 8.0);
    }

    #[test]
    fn ground_support_finishes_the_tracked_fall() {
        let mut state = PlayerFallState {
            support: CharacterSupport::Ground,
            peak_y: 10.0,
        };

        let impact = state.update(4.0);

        assert_eq!(impact, Some(FallImpact { drop: 6.0 }));
        assert_eq!(state.peak_y, f32::NEG_INFINITY);
    }

    #[test]
    fn ladder_support_finishes_the_tracked_fall() {
        let mut state = PlayerFallState {
            support: CharacterSupport::Ladder,
            peak_y: 10.0,
        };

        let impact = state.update(4.0);

        assert_eq!(impact, Some(FallImpact { drop: 6.0 }));
        assert_eq!(state.peak_y, f32::NEG_INFINITY);
    }

    #[test]
    fn support_at_the_apex_height_clears_without_an_impact() {
        let mut state = PlayerFallState {
            support: CharacterSupport::Ground,
            peak_y: 4.0,
        };

        let impact = state.update(4.0);

        assert_eq!(impact, None);
        assert_eq!(state.peak_y, f32::NEG_INFINITY);
    }

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
    fn low_gravity_jump_lands_below_safe_distance() {
        // Low-gravity jump: apex ≈ 14.4 m of drop under g = 5, charged like a
        // 14.4 · 5/25 = 2.88 m normal fall — well under the safe threshold.
        let distance = effective_fall_distance(14.4, 5.0, TEST_GRAVITY);
        assert!((distance - 2.88).abs() < 1e-4, "expected 2.88m, got {distance}m");
    }

    #[test]
    fn phantom_fall_charges_zero_distance() {
        // Velocity fabricated while standing: no drop, no damage.
        assert_eq!(effective_fall_distance(0.0, TEST_GRAVITY, TEST_GRAVITY), 0.0);
    }

    #[test]
    fn genuine_fall_distance_passes_through() {
        // A normal-gravity fall charges its full height — independent of the
        // terminal-velocity clamp, so a long fall stays lethal.
        assert_eq!(effective_fall_distance(13.2, TEST_GRAVITY, TEST_GRAVITY), 13.2);
    }
}
