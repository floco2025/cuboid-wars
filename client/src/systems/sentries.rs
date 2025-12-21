use bevy::prelude::*;

use super::network::ServerReconciliation;
use common::{
    collision::{slide_sentry_along_obstacles, sweep_sentry_vs_ramp_footprint, sweep_sentry_vs_wall},
    constants::{SENTRY_HEIGHT, UPDATE_BROADCAST_INTERVAL},
    markers::SentryMarker,
    protocol::{FaceDirection, MapLayout, Position, Velocity},
};

// ============================================================================
// Sentries Movement System
// ============================================================================

pub fn sentries_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    map_layout: Option<Res<MapLayout>>,
    mut sentry_query: Query<
        (Entity, &mut Position, &mut Velocity, Option<&mut ServerReconciliation>),
        With<SentryMarker>,
    >,
) {
    let delta = time.delta_secs();

    for (entity, mut client_pos, client_vel, recon_option) in &mut sentry_query {
        let target_pos = if let Some(mut recon) = recon_option {
            let correction_time: f32 = recon.rtt * 5.0; // Benchmark: RTT = 100ms equals 0.5s correction time
            let correction_factor = (UPDATE_BROADCAST_INTERVAL / correction_time).clamp(0.0, 1.0);

            recon.timer += delta * correction_factor;
            if recon.timer >= UPDATE_BROADCAST_INTERVAL {
                commands.entity(entity).remove::<ServerReconciliation>();
            }

            let server_pos_x = recon.server_pos.x + recon.server_vel.x * recon.rtt / 2.0;
            let server_pos_z = recon.server_pos.z + recon.server_vel.z * recon.rtt / 2.0;

            let total_dx = server_pos_x - recon.client_pos.x;
            let total_dz = server_pos_z - recon.client_pos.z;

            // If the sentry got totally out of sync, we jump to the server position
            if total_dx.abs() >= 3.0 || total_dz.abs() >= 3.0 {
                warn!("sentry out of sync, jumping to server position");
                *client_pos = recon.server_pos;
                commands.entity(entity).remove::<ServerReconciliation>();
                continue;
            }

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

        let final_pos = apply_sentry_wall_sliding(map_layout.as_deref(), &client_pos, &target_pos, &client_vel, delta);
        *client_pos = final_pos;
    }
}

fn apply_sentry_wall_sliding(
    map_layout: Option<&MapLayout>,
    current_pos: &Position,
    target_pos: &Position,
    velocity: &Velocity,
    delta: f32,
) -> Position {
    let Some(map_layout) = map_layout else {
        return *target_pos;
    };

    let mut collides = false;

    for wall in &map_layout.lower_walls {
        if sweep_sentry_vs_wall(current_pos, target_pos, wall) {
            collides = true;
            break;
        }
    }

    if !collides {
        for ramp in &map_layout.ramps {
            if sweep_sentry_vs_ramp_footprint(current_pos, target_pos, ramp) {
                collides = true;
                break;
            }
        }
    }

    if collides {
        // Apply the same slide logic as server: walls + ramp footprints
        slide_sentry_along_obstacles(
            &map_layout.lower_walls,
            &map_layout.ramps,
            current_pos,
            velocity.x,
            velocity.z,
            delta,
        )
    } else {
        *target_pos
    }
}

// ============================================================================
// Sentries Sync System
// ============================================================================

// Update sentry Transform from Position and FaceDirection components for rendering
pub fn sentries_transform_sync_system(
    mut sentry_query: Query<(&Position, &FaceDirection, &mut Transform), With<SentryMarker>>,
) {
    for (pos, face_dir, mut transform) in &mut sentry_query {
        transform.translation.x = pos.x;
        transform.translation.y = SENTRY_HEIGHT / 2.0; // Sentry center at correct height
        transform.translation.z = pos.z;
        transform.rotation = Quat::from_rotation_y(face_dir.0);
    }
}
