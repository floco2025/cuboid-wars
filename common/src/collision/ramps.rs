use super::helpers::sweep_slab_interval;
use crate::{
    constants::{GHOST_SIZE, PLAYER_DEPTH, PLAYER_WIDTH, PROJECTILE_RADIUS, RAMP_EDGE_WIDTH},
    protocol::{Position, Ramp},
};

// Swept AABB vs ramp edges (side guards) without wall helpers.
#[must_use]
pub fn sweep_aabb_vs_ramp_edges(
    start_pos: &Position,
    end_pos: &Position,
    ramp: &Ramp,
    half_x: f32,
    half_z: f32,
) -> bool {
    let min_x = ramp.x1.min(ramp.x2);
    let max_x = ramp.x1.max(ramp.x2);
    let min_z = ramp.z1.min(ramp.z2);
    let max_z = ramp.z1.max(ramp.z2);

    let dx = (ramp.x2 - ramp.x1).abs();
    let dz = (ramp.z2 - ramp.z1).abs();
    let block_sides_along_z = dx >= dz;

    let sweep_edge = |center_x: f32, center_z: f32, half_x_edge: f32, half_z_edge: f32| -> bool {
        let dir_x = end_pos.x - start_pos.x;
        let dir_z = end_pos.z - start_pos.z;

        let local_x = start_pos.x - center_x;
        let local_z = start_pos.z - center_z;

        let mut t_min = 0.0_f32;
        let mut t_max = 1.0_f32;

        if let Some((new_min, new_max)) = sweep_slab_interval(local_x, dir_x, half_x + half_x_edge, t_min, t_max) {
            t_min = new_min;
            t_max = new_max;
        } else {
            return false;
        }

        if let Some((new_min, new_max)) = sweep_slab_interval(local_z, dir_z, half_z + half_z_edge, t_min, t_max) {
            t_min = new_min;
            t_max = new_max;
        } else {
            return false;
        }

        t_min <= t_max && t_max >= 0.0 && t_min <= 1.0
    };

    let edge_half = RAMP_EDGE_WIDTH / 2.0;

    if block_sides_along_z {
        let center_x = (min_x + max_x) / 2.0;
        let half_x_edge = (max_x - min_x) / 2.0;
        sweep_edge(center_x, min_z, half_x_edge, edge_half) || sweep_edge(center_x, max_z, half_x_edge, edge_half)
    } else {
        let center_z = (min_z + max_z) / 2.0;
        let half_z_edge = (max_z - min_z) / 2.0;
        sweep_edge(min_x, center_z, edge_half, half_z_edge) || sweep_edge(max_x, center_z, edge_half, half_z_edge)
    }
}

#[must_use]
pub fn sweep_player_vs_ramp_edges(start_pos: &Position, end_pos: &Position, ramp: &Ramp) -> bool {
    sweep_aabb_vs_ramp_edges(start_pos, end_pos, ramp, PLAYER_WIDTH / 2.0, PLAYER_DEPTH / 2.0)
}

#[must_use]
pub fn sweep_ghost_vs_ramp_edges(start_pos: &Position, end_pos: &Position, ramp: &Ramp) -> bool {
    let ghost_half_size = GHOST_SIZE / 2.0;
    sweep_aabb_vs_ramp_edges(start_pos, end_pos, ramp, ghost_half_size, ghost_half_size)
}

// Sampled ramp hit check used by projectiles.
#[must_use]
pub fn projectile_hits_ramp(
    proj_pos: &Position,
    projectile_velocity: &bevy_math::Vec3,
    delta: f32,
    ramp: &Ramp,
) -> bool {
    use crate::ramps::calculate_height_at_position;

    let num_samples = 5;
    for i in 0..=num_samples {
        let t = i as f32 / num_samples as f32;
        let sample_x = proj_pos.x + projectile_velocity.x * delta * t;
        let sample_y = proj_pos.y + projectile_velocity.y * delta * t;
        let sample_z = proj_pos.z + projectile_velocity.z * delta * t;

        let min_x = ramp.x1.min(ramp.x2);
        let max_x = ramp.x1.max(ramp.x2);
        let min_z = ramp.z1.min(ramp.z2);
        let max_z = ramp.z1.max(ramp.z2);

        if sample_x >= min_x && sample_x <= max_x && sample_z >= min_z && sample_z <= max_z {
            let ramp_height = calculate_height_at_position(&[*ramp], sample_x, sample_z);

            if (sample_y - ramp_height).abs() < PROJECTILE_RADIUS * 2.0 {
                return true;
            }
        }
    }

    false
}
