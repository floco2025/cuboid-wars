use bevy::prelude::*;

use crate::resources::WallConfig;
use common::{
    collision::check_ghost_wall_collision,
    collision::check_player_wall_collision,
    constants::{RUN_SPEED, UPDATE_BROADCAST_INTERVAL},
    protocol::{Position, Velocity},
};

// ============================================================================
// Components
// ============================================================================

// Marker component for the local player (yourself)
#[derive(Component)]
pub struct LocalPlayer;

// Track bump flash effect state for local player
#[derive(Component, Default)]
pub struct BumpFlashState {
    pub was_colliding: bool,
    pub flash_timer: f32,
}

// Server's authoritative snapshot for this entity
#[derive(Component)]
pub struct ServerReconciliation {
    pub client_pos: Position,
    pub server_pos: Position,
    pub server_vel: Velocity,
    pub timer: f32,
    pub rtt: f32,
}

// ============================================================================
// Client-side Movement System
// ============================================================================

type MovementQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Position,
        &'static Velocity,
        Option<&'static mut BumpFlashState>,
        Option<&'static mut ServerReconciliation>,
        Has<LocalPlayer>,
    ),
>;

// Client-side movement system with wall collision detection for smooth prediction
pub fn client_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    wall_config: Option<Res<WallConfig>>,
    mut query: MovementQuery,
    mut bump_flash_ui: Query<(&mut BackgroundColor, &mut Visibility), With<super::ui::BumpFlashUI>>,
) {
    let delta = time.delta_secs();

    let walls = wall_config.as_deref();
    let entity_positions: Vec<(Entity, Position)> =
        query.iter().map(|(entity, pos, _, _, _, _)| (entity, *pos)).collect();

    for (entity, mut client_pos, client_vel, mut flash_state, mut server_snapshot, is_local) in &mut query {
        if let Some(state) = flash_state.as_mut() {
            decay_flash_timer(state, delta, is_local, &mut bump_flash_ui);
        }

        let abs_velocity = calculate_absolute_velocity(client_vel);
        let is_standing_still = abs_velocity < f32::EPSILON;

        let target_pos = if let Some(recon) = server_snapshot.as_mut() {
            const IDLE_CORRECTION_TIME: f32 = 5.0; // Standing still: slow, smooth correction
            const RUN_CORRECTION_TIME: f32 = 3.5; // Running: faster, more responsive correction

            let speed_ratio = (abs_velocity / RUN_SPEED).clamp(0.0, 1.0); // Ignore speed power-ups
            let correction_time_interval = IDLE_CORRECTION_TIME.lerp(RUN_CORRECTION_TIME, speed_ratio);
            let correction_factor = (UPDATE_BROADCAST_INTERVAL / correction_time_interval).clamp(0.0, 1.0);

            recon.timer += delta * correction_factor;
            if recon.timer >= UPDATE_BROADCAST_INTERVAL {
                commands.entity(entity).remove::<ServerReconciliation>();
            }

            let server_pos_x = recon.server_pos.x + recon.server_vel.x * recon.rtt / 2.0;
            let server_pos_z = recon.server_pos.z + recon.server_vel.z * recon.rtt / 2.0;

            let total_dx = server_pos_x - recon.client_pos.x;
            let total_dz = server_pos_z - recon.client_pos.z;

            let dx = total_dx * delta * correction_factor / UPDATE_BROADCAST_INTERVAL;
            let dz = total_dz * delta * correction_factor / UPDATE_BROADCAST_INTERVAL;

            Position {
                x: client_vel.x.mul_add(delta, client_pos.x) + dx,
                y: client_pos.y,
                z: client_vel.z.mul_add(delta, client_pos.z) + dz,
            }
        } else {
            Position {
                x: client_vel.x.mul_add(delta, client_pos.x),
                y: client_pos.y,
                z: client_vel.z.mul_add(delta, client_pos.z),
            }
        };

        // Skip collision checks if player is standing still
        if is_standing_still {
            continue;
        }

        let hits_wall = player_hits_wall(walls, &target_pos);
        let hits_player = player_hits_other_player(entity, &target_pos, &entity_positions);

        if !hits_wall && !hits_player {
            *client_pos = target_pos;
            if let Some(state) = flash_state.as_mut() {
                state.was_colliding = false;
            }
        } else if hits_wall {
            // Slide along the wall instead of stopping
            let slide_pos = common::collision::calculate_wall_slide(
                &walls.expect("walls should exist if hit_wall is true").walls,
                &client_pos,
                &target_pos,
                client_vel.x,
                client_vel.z,
                delta,
            );
            *client_pos = slide_pos;

            if is_local && let Some(state) = flash_state.as_mut() {
                trigger_collision_feedback(&mut commands, &asset_server, &mut bump_flash_ui, state, true);
            }
        } else if hits_player {
            // Stop for player collisions
            if is_local && let Some(state) = flash_state.as_mut() {
                trigger_collision_feedback(&mut commands, &asset_server, &mut bump_flash_ui, state, false);
            }
        }
    }
}

const BUMP_FLASH_DURATION: f32 = 0.08;

fn calculate_absolute_velocity(velocity: &Velocity) -> f32 {
    (velocity.x * velocity.x + velocity.z * velocity.z).sqrt()
}

fn player_hits_wall(walls: Option<&WallConfig>, new_pos: &Position) -> bool {
    let Some(config) = walls else { return false };
    config
        .walls
        .iter()
        .any(|wall| check_player_wall_collision(new_pos, wall))
}

fn player_hits_other_player(entity: Entity, new_pos: &Position, positions: &[(Entity, Position)]) -> bool {
    positions.iter().any(|(other_entity, other_pos)| {
        *other_entity != entity && common::collision::check_player_player_collision(new_pos, other_pos)
    })
}

fn decay_flash_timer(
    state: &mut Mut<BumpFlashState>,
    delta: f32,
    is_local: bool,
    bump_flash_ui: &mut Query<(&mut BackgroundColor, &mut Visibility), With<super::ui::BumpFlashUI>>,
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
    bump_flash_ui: &mut Query<(&mut BackgroundColor, &mut Visibility), With<super::ui::BumpFlashUI>>,
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
// Ghost Movement System
// ============================================================================

// Client-side ghost movement system - applies velocity with server reconciliation
// Enforces client-side wall collision to prevent visual clipping
pub fn ghost_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    wall_config: Option<Res<WallConfig>>,
    mut ghost_query: Query<
        (Entity, &mut Position, &mut Velocity, Option<&mut ServerReconciliation>),
        With<crate::spawning::GhostMarker>,
    >,
) {
    let delta = time.delta_secs();
    let walls = wall_config.as_deref();

    for (entity, mut pos, mut vel, server_snapshot) in &mut ghost_query {
        let (target_pos, target_vel) = if let Some(mut recon) = server_snapshot {
            // Ghosts: smoothly interpolate from client to server position
            const CORRECTION_TIME: f32 = 3.0; // Fast smooth correction
            let correction_factor = (UPDATE_BROADCAST_INTERVAL / CORRECTION_TIME).clamp(0.0, 1.0);

            recon.timer += delta * correction_factor;
            if recon.timer >= UPDATE_BROADCAST_INTERVAL {
                commands.entity(entity).remove::<ServerReconciliation>();
            }

            // Predict where the server thinks the ghost should be now
            let time_since_update = delta;
            let server_pos_x = recon.server_pos.x + recon.server_vel.x * time_since_update;
            let server_pos_z = recon.server_pos.z + recon.server_vel.z * time_since_update;

            // Smoothly lerp from current client position to predicted server position
            let lerp_amount = (delta * correction_factor / UPDATE_BROADCAST_INTERVAL).clamp(0.0, 1.0);

            (
                Position {
                    x: pos.x + (server_pos_x - pos.x) * lerp_amount,
                    y: pos.y,
                    z: pos.z + (server_pos_z - pos.z) * lerp_amount,
                },
                recon.server_vel,
            )
        } else {
            // No reconciliation - just apply velocity normally
            (
                Position {
                    x: vel.x.mul_add(delta, pos.x),
                    y: pos.y,
                    z: vel.z.mul_add(delta, pos.z),
                },
                *vel,
            )
        };

        let hits_wall = ghost_hits_wall(walls, &target_pos);

        if !hits_wall {
            *pos = target_pos;
            *vel = target_vel;
        } else {
            // Stop the ghost when it hits a wall to prevent jittering
            // The server will send updated velocity when it changes direction
            vel.x = 0.0;
            vel.z = 0.0;
        }
    }
}

fn ghost_hits_wall(walls: Option<&WallConfig>, new_pos: &Position) -> bool {
    let Some(config) = walls else { return false };
    config
        .walls
        .iter()
        .any(|wall| check_ghost_wall_collision(new_pos, wall))
}
