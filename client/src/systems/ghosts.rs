use bevy::prelude::*;

use super::network::ServerReconciliation;
use crate::resources::WallConfig;
use common::{
    collision::check_ghost_wall_collision,
    constants::{GHOST_SIZE, UPDATE_BROADCAST_INTERVAL},
    protocol::{GhostId, Position, Velocity},
};

// ============================================================================
// Ghost Movement System
// ============================================================================

#[allow(clippy::similar_names)]
pub fn ghost_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    wall_config: Option<Res<WallConfig>>,
    mut ghost_query: Query<(Entity, &mut Position, &mut Velocity, Option<&mut ServerReconciliation>), With<GhostId>>,
) {
    let delta = time.delta_secs();

    for (entity, mut client_pos, client_vel, recon_option) in &mut ghost_query {
        let target_pos = if let Some(mut recon) = recon_option {
            const CORRECTION_TIME: f32 = 1.0;
            let correction_factor = (UPDATE_BROADCAST_INTERVAL / CORRECTION_TIME).clamp(0.0, 1.0);

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

        let walls = wall_config.as_deref();
        let hits_wall = ghost_hits_wall(walls, &target_pos);
        if !hits_wall {
            *client_pos = target_pos;
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

// ============================================================================
// Ghost Sync System
// ============================================================================

// Update ghost Transform from Position component for rendering
pub fn sync_position_to_transform_system(mut ghost_query: Query<(&Position, &mut Transform), With<GhostId>>) {
    for (pos, mut transform) in &mut ghost_query {
        transform.translation.x = pos.x;
        transform.translation.y = GHOST_SIZE / 2.0; // Ghost center at correct height
        transform.translation.z = pos.z;
    }
}
