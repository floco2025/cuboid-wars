use bevy::prelude::*;

use super::components::BumpFlashState;
use crate::{markers::*, resources::PlayerMap, systems::network::ServerReconciliation};
use common::{
    collision::{slide_player_along_obstacles, sweep_player_vs_ramp_edges, sweep_player_vs_wall},
    constants::{ALWAYS_PHASING, ROOF_HEIGHT, SPEED_RUN, UPDATE_BROADCAST_INTERVAL},
    map::{close_to_roof, has_roof, height_on_ramp},
    players::{PlannedMove, overlaps_other_player},
    protocol::{MapLayout, PlayerId, Position, Velocity},
};

// ============================================================================
// Helper Functions
// ============================================================================

const BUMP_FLASH_DURATION: f32 = 0.08;

fn decay_flash_timer(
    state: &mut Mut<BumpFlashState>,
    delta: f32,
    is_local: bool,
    bump_flash_ui: &mut Query<(&mut BackgroundColor, &mut Visibility), With<BumpFlashUIMarker>>,
) {
    if state.flash_timer <= 0.0 {
        return;
    }

    state.flash_timer -= delta;
    if state.flash_timer <= 0.0
        && is_local
        && let Some((mut bg_color, mut visibility)) = bump_flash_ui.iter_mut().next()
    {
        *visibility = Visibility::Hidden;
        bg_color.0 = Color::srgba(1.0, 1.0, 1.0, 0.0);
    }
}

fn trigger_collision_feedback(
    commands: &mut Commands,
    asset_server: &AssetServer,
    bump_flash_ui: &mut Query<(&mut BackgroundColor, &mut Visibility), With<BumpFlashUIMarker>>,
    state: &mut Mut<BumpFlashState>,
    collided_with_wall: bool,
) {
    if !state.was_colliding {
        if let Some((mut bg_color, mut visibility)) = bump_flash_ui.iter_mut().next() {
            *visibility = Visibility::Visible;
            bg_color.0 = Color::srgba(1.0, 1.0, 1.0, 0.2);
        }

        let sound_path = if collided_with_wall {
            "sounds/player_bumps_wall.ogg"
        } else {
            "sounds/player_bumps_player.ogg"
        };

        commands.spawn((
            AudioPlayer::new(asset_server.load(sound_path)),
            PlaybackSettings::DESPAWN,
        ));

        state.flash_timer = BUMP_FLASH_DURATION;
    }

    state.was_colliding = true;
}

// ============================================================================
// Players Movement System
// ============================================================================

type MovementQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PlayerId,
        &'static mut Position,
        &'static Velocity,
        Option<&'static mut BumpFlashState>,
        Option<&'static mut ServerReconciliation>,
        Has<LocalPlayerMarker>,
    ),
>;

pub fn players_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    map_layout: Option<Res<MapLayout>>,
    players: Res<PlayerMap>,
    mut query: MovementQuery,
    mut bump_flash_ui: Query<(&mut BackgroundColor, &mut Visibility), With<BumpFlashUIMarker>>,
) {
    let delta = time.delta_secs();

    // Pass 1: For each player, calculate intended position, then apply wall collision logic
    let mut planned_moves: Vec<PlannedMove> = Vec::new();

    for (entity, player_id, mut client_pos, client_vel, mut flash_state, mut recon_option, is_local) in &mut query {
        if let Some(state) = flash_state.as_mut() {
            decay_flash_timer(state, delta, is_local, &mut bump_flash_ui);
        }

        let abs_velocity = client_vel.x.hypot(client_vel.z);
        let is_standing_still = abs_velocity < f32::EPSILON;

        // Calculate intended position from velocity (with server reconciliation if needed)
        let mut target_pos = if let Some(recon) = recon_option.as_mut() {
            const IDLE_CORRECTION_TIME: f32 = 10.0; // Standing still: slow, smooth correction
            let run_correction_time: f32 = recon.rtt * 5.0; // Benchmark: RTT = 100ms equals 0.5s correction time

            let speed_ratio = (abs_velocity / SPEED_RUN).clamp(0.0, 1.0); // Ignore speed power-ups
            let correction_time_interval = IDLE_CORRECTION_TIME.lerp(run_correction_time, speed_ratio);
            let correction_factor = (UPDATE_BROADCAST_INTERVAL / correction_time_interval).clamp(0.0, 1.0);

            recon.timer += delta * correction_factor;
            if recon.timer >= UPDATE_BROADCAST_INTERVAL {
                commands.entity(entity).remove::<ServerReconciliation>();
            }

            let server_pos_x = recon.server_pos.x + recon.server_vel.x * recon.rtt / 2.0;
            let server_pos_z = recon.server_pos.z + recon.server_vel.z * recon.rtt / 2.0;

            let total_dx = server_pos_x - recon.client_pos.x;
            let total_dz = server_pos_z - recon.client_pos.z;

            // If the player got totally out of sync, we jump to the server position
            let out_of_sync_distance = if is_standing_still { 3.0 } else { 5.0 };
            if total_dx.abs() >= out_of_sync_distance || total_dz.abs() >= out_of_sync_distance {
                warn!("player out of sync, jumping to server position");
                *client_pos = recon.server_pos;
                commands.entity(entity).remove::<ServerReconciliation>();
                continue;
            }

            let dx = total_dx * delta * correction_factor / UPDATE_BROADCAST_INTERVAL;
            let dz = total_dz * delta * correction_factor / UPDATE_BROADCAST_INTERVAL;

            let new_x = client_vel.x.mul_add(delta, client_pos.x) + dx;
            let new_z = client_vel.z.mul_add(delta, client_pos.z) + dz;

            Position {
                x: new_x,
                y: client_pos.y, // Keep current Y for collision detection
                z: new_z,
            }
        } else {
            let new_x = client_vel.x.mul_add(delta, client_pos.x);
            let new_z = client_vel.z.mul_add(delta, client_pos.z);
            Position {
                x: new_x,
                y: client_pos.y, // Keep current Y for collision detection
                z: new_z,
            }
        };

        // Skip collision checks if player is standing still
        if is_standing_still {
            planned_moves.push(PlannedMove {
                entity,
                start: *client_pos,
                target: target_pos,
                collides: false,
            });
            continue;
        }

        // Check collision and calculate target (with sliding if collision)
        let mut collides = false;

        if let Some(map_layout) = map_layout.as_ref() {
            let walls_to_check = if close_to_roof(client_pos.y) {
                &map_layout.roof_walls
            } else {
                let has_phasing = ALWAYS_PHASING || players.0.get(player_id).is_some_and(|info| info.phasing_power_up);
                if has_phasing {
                    &map_layout.boundary_walls
                } else {
                    &map_layout.lower_walls
                }
            };

            for wall in walls_to_check {
                if sweep_player_vs_wall(&client_pos, &target_pos, wall) {
                    collides = true;
                    break;
                }
            }

            if !collides {
                for ramp in &map_layout.ramps {
                    if sweep_player_vs_ramp_edges(&client_pos, &target_pos, ramp) {
                        collides = true;
                        break;
                    }
                }
            }

            if collides {
                target_pos = slide_player_along_obstacles(
                    walls_to_check,
                    &map_layout.ramps,
                    &client_pos,
                    client_vel.x,
                    client_vel.z,
                    delta,
                );
            }

            let target_height_on_ramp = height_on_ramp(&map_layout.ramps, target_pos.x, target_pos.z);
            let target_has_roof = has_roof(&map_layout.roofs, target_pos.x, target_pos.z);

            if target_height_on_ramp > 0.0 {
                target_pos.y = target_height_on_ramp;
            } else if target_has_roof && close_to_roof(client_pos.y) {
                target_pos.y = ROOF_HEIGHT;
            } else {
                target_pos.y = 0.0;
            }
        }

        planned_moves.push(PlannedMove {
            entity,
            start: *client_pos,
            target: target_pos,
            collides,
        });
    }

    // Pass 2: Check player-player collisions and apply final positions
    for planned_move in &planned_moves {
        let Ok((_, _, mut client_pos, _, mut flash_state, _, is_local)) = query.get_mut(planned_move.entity) else {
            continue;
        };

        let hits_player = overlaps_other_player(planned_move, &planned_moves);

        // Apply final position and feedback
        if hits_player {
            // Stop for player collisions
            if is_local && let Some(state) = flash_state.as_mut() {
                trigger_collision_feedback(&mut commands, &asset_server, &mut bump_flash_ui, state, false);
            }
        } else {
            *client_pos = planned_move.target;

            if let Some(state) = flash_state.as_mut() {
                if planned_move.collides {
                    if is_local {
                        trigger_collision_feedback(&mut commands, &asset_server, &mut bump_flash_ui, state, true);
                    }
                } else {
                    state.was_colliding = false;
                }
            }
        }
    }
}
