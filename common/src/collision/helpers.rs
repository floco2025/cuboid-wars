use bevy::prelude::*;

use crate::{
    constants::PHYSICS_EPSILON,
    protocol::{Position, Ramp, Wall},
};

/// Result of a sweep collision test: surface normal and time of impact.
#[derive(Debug, Clone, Copy)]
pub struct Collision {
    pub normal: Vec3,
    pub t: f32,
}

// Check if two 1D ranges overlap.
#[must_use]
pub fn ranges_overlap_1d(a_min: f32, a_max: f32, b_min: f32, b_max: f32) -> bool {
    a_max >= b_min && a_min <= b_max
}

// Compute the intersection interval of a ray with a slab (used in ray-AABB tests)
#[must_use]
pub fn sweep_slab_interval(
    local_coord: f32,
    ray_dir: f32,
    half_extent: f32,
    t_min: f32,
    t_max: f32,
) -> Option<(f32, f32)> {
    if ray_dir.abs() > PHYSICS_EPSILON {
        let t1 = (-half_extent - local_coord) / ray_dir;
        let t2 = (half_extent - local_coord) / ray_dir;
        let new_min = t_min.max(t1.min(t2));
        let new_max = t_max.min(t1.max(t2));
        if new_min <= new_max {
            Some((new_min, new_max))
        } else {
            None
        }
    } else if local_coord.abs() > half_extent {
        None
    } else {
        Some((t_min, t_max))
    }
}

// Generic swept AABB vs AABB (same height) in the XZ plane; caller supplies combined half extents and height.
#[must_use]
pub fn sweep_aabb_vs_aabb(
    start1: &Position,
    end1: &Position,
    start2: &Position,
    end2: &Position,
    combined_half_x: f32,
    combined_half_z: f32,
    height: f32,
) -> bool {
    let y_diff_start = (start1.y - start2.y).abs();
    let y_diff_end = (end1.y - end2.y).abs();
    if y_diff_start >= height && y_diff_end >= height {
        return false;
    }

    let rel_start_x = start1.x - start2.x;
    let rel_start_z = start1.z - start2.z;
    let rel_dir_x = (end1.x - start1.x) - (end2.x - start2.x);
    let rel_dir_z = (end1.z - start1.z) - (end2.z - start2.z);

    let mut t_min = 0.0_f32;
    let mut t_max = 1.0_f32;

    if let Some((new_min, new_max)) = sweep_slab_interval(rel_start_x, rel_dir_x, combined_half_x, t_min, t_max) {
        t_min = new_min;
        t_max = new_max;
    } else {
        return false;
    }

    if let Some((new_min, new_max)) = sweep_slab_interval(rel_start_z, rel_dir_z, combined_half_z, t_min, t_max) {
        t_min = new_min;
        t_max = new_max;
    } else {
        return false;
    }

    t_min <= t_max && t_max >= 0.0 && t_min <= 1.0
}

// Swept AABB vs ramp side edges; caller supplies entity half-extents and edge half-width.
#[must_use]
pub fn sweep_ramp_edges(
    start_pos: &Position,
    end_pos: &Position,
    ramp: &Ramp,
    half_x: f32,
    half_z: f32,
    edge_half: f32,
) -> bool {
    let (min_x, max_x, min_z, max_z) = ramp.bounds_xz();

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

    if block_sides_along_z {
        let center_x = f32::midpoint(min_x, max_x);
        let half_x_edge = (max_x - min_x) / 2.0;
        sweep_edge(center_x, min_z, half_x_edge, edge_half) || sweep_edge(center_x, max_z, half_x_edge, edge_half)
    } else {
        let center_z = f32::midpoint(min_z, max_z);
        let half_z_edge = (max_z - min_z) / 2.0;
        sweep_edge(min_x, center_z, edge_half, half_z_edge) || sweep_edge(max_x, center_z, edge_half, half_z_edge)
    }
}

// Swept AABB vs the high-side cap of a ramp (blocks entering through the tall face).
pub fn sweep_ramp_high_cap(
    start_pos: &Position,
    end_pos: &Position,
    ramp: &Ramp,
    half_x: f32,
    half_z: f32,
    cap_half: f32,
) -> bool {
    let (min_x, max_x, min_z, max_z) = ramp.bounds_xz();

    let dx = (ramp.x2 - ramp.x1).abs();
    let dz = (ramp.z2 - ramp.z1).abs();
    let along_x = dx >= dz;
    let high_along_positive = ramp.y2 >= ramp.y1;

    let (center_x, center_z, half_x_cap, half_z_cap) = if along_x {
        let high_x = if high_along_positive { ramp.x2 } else { ramp.x1 };
        (high_x, f32::midpoint(min_z, max_z), cap_half, (max_z - min_z) / 2.0)
    } else {
        let high_z = if high_along_positive { ramp.z2 } else { ramp.z1 };
        (f32::midpoint(min_x, max_x), high_z, (max_x - min_x) / 2.0, cap_half)
    };

    let dir_x = end_pos.x - start_pos.x;
    let dir_z = end_pos.z - start_pos.z;

    let local_x = start_pos.x - center_x;
    let local_z = start_pos.z - center_z;

    // If we already start inside the cap volume, allow movement to escape it
    if local_x.abs() <= half_x + half_x_cap && local_z.abs() <= half_z + half_z_cap {
        warn!("escaping from inside ramp high-side cap; this should not normally happen");
        return false;
    }

    let mut t_min = 0.0_f32;
    let mut t_max = 1.0_f32;

    if let Some((new_min, new_max)) = sweep_slab_interval(local_x, dir_x, half_x + half_x_cap, t_min, t_max) {
        t_min = new_min;
        t_max = new_max;
    } else {
        return false;
    }

    if let Some((new_min, new_max)) = sweep_slab_interval(local_z, dir_z, half_z + half_z_cap, t_min, t_max) {
        t_min = new_min;
        t_max = new_max;
    } else {
        return false;
    }

    t_min <= t_max && t_max >= 0.0 && t_min <= 1.0
}

/// Swept point vs axis-aligned cuboid; returns collision info if within [0,1].
#[must_use]
pub fn sweep_point_vs_cuboid(
    proj_pos: &Position,
    ray_dir_x: f32,
    ray_dir_y: f32,
    ray_dir_z: f32,
    center_x: f32,
    center_y: f32,
    center_z: f32,
    half_x: f32,
    half_y: f32,
    half_z: f32,
) -> Option<Collision> {
    let local_x = proj_pos.x - center_x;
    let local_y = proj_pos.y - center_y;
    let local_z = proj_pos.z - center_z;

    let mut t_enter = 0.0_f32;
    let mut t_exit = 1.0_f32;
    let mut hit_normal = Vec3::ZERO;

    if ray_dir_x.abs() < PHYSICS_EPSILON {
        if local_x.abs() > half_x {
            return None;
        }
    } else {
        let tx1 = (-half_x - local_x) / ray_dir_x;
        let tx2 = (half_x - local_x) / ray_dir_x;
        let (tx_min, tx_max) = if tx1 < tx2 { (tx1, tx2) } else { (tx2, tx1) };
        if tx_min > t_enter {
            t_enter = tx_min;
            hit_normal = Vec3::new(if ray_dir_x > 0.0 { -1.0 } else { 1.0 }, 0.0, 0.0);
        }
        t_exit = t_exit.min(tx_max);
        if t_enter > t_exit {
            return None;
        }
    }

    if ray_dir_y.abs() < PHYSICS_EPSILON {
        if local_y.abs() > half_y {
            return None;
        }
    } else {
        let ty1 = (-half_y - local_y) / ray_dir_y;
        let ty2 = (half_y - local_y) / ray_dir_y;
        let (ty_min, ty_max) = if ty1 < ty2 { (ty1, ty2) } else { (ty2, ty1) };
        if ty_min > t_enter {
            t_enter = ty_min;
            hit_normal = Vec3::new(0.0, if ray_dir_y > 0.0 { -1.0 } else { 1.0 }, 0.0);
        }
        t_exit = t_exit.min(ty_max);
        if t_enter > t_exit {
            return None;
        }
    }

    if ray_dir_z.abs() < PHYSICS_EPSILON {
        if local_z.abs() > half_z {
            return None;
        }
    } else {
        let tz1 = (-half_z - local_z) / ray_dir_z;
        let tz2 = (half_z - local_z) / ray_dir_z;
        let (tz_min, tz_max) = if tz1 < tz2 { (tz1, tz2) } else { (tz2, tz1) };
        if tz_min > t_enter {
            t_enter = tz_min;
            hit_normal = Vec3::new(0.0, 0.0, if ray_dir_z > 0.0 { -1.0 } else { 1.0 });
        }
        t_exit = t_exit.min(tz_max);
        if t_enter > t_exit {
            return None;
        }
    }

    if t_exit < 0.0 || t_enter > 1.0 {
        return None;
    }

    if hit_normal == Vec3::ZERO {
        return None;
    }

    Some(Collision {
        normal: hit_normal,
        t: t_enter.clamp(0.0, 1.0),
    })
}

// Axis-aligned wall overlap against an AABB with given half-extents.
#[must_use]
pub fn overlap_aabb_vs_wall(entity_pos: &Position, wall: &Wall, half_x: f32, half_z: f32) -> bool {
    let dx = (wall.x2 - wall.x1).abs();
    let dz = (wall.z2 - wall.z1).abs();
    let wall_half_width = wall.width / 2.0;

    let (wall_min_x, wall_max_x, wall_min_z, wall_max_z) = if dx > dz {
        (
            wall.x1.min(wall.x2),
            wall.x1.max(wall.x2),
            wall.z1.min(wall.z2) - wall_half_width,
            wall.z1.max(wall.z2) + wall_half_width,
        )
    } else {
        (
            wall.x1.min(wall.x2) - wall_half_width,
            wall.x1.max(wall.x2) + wall_half_width,
            wall.z1.min(wall.z2),
            wall.z1.max(wall.z2),
        )
    };

    let entity_min_x = entity_pos.x - half_x;
    let entity_max_x = entity_pos.x + half_x;
    let entity_min_z = entity_pos.z - half_z;
    let entity_max_z = entity_pos.z + half_z;

    ranges_overlap_1d(entity_min_x, entity_max_x, wall_min_x, wall_max_x)
        && ranges_overlap_1d(entity_min_z, entity_max_z, wall_min_z, wall_max_z)
}

// Swept AABB vs wall (axis-aligned) to prevent tunneling.
#[must_use]
pub fn sweep_aabb_vs_wall(start_pos: &Position, end_pos: &Position, wall: &Wall, half_x: f32, half_z: f32) -> bool {
    let wall_center_x = f32::midpoint(wall.x1, wall.x2);
    let wall_center_z = f32::midpoint(wall.z1, wall.z2);

    let dx = (wall.x2 - wall.x1).abs();
    let dz = (wall.z2 - wall.z1).abs();
    let wall_half_width = wall.width / 2.0;

    let (wall_half_x, wall_half_z) = if dx > dz {
        (dx / 2.0, wall_half_width)
    } else {
        (wall_half_width, dz / 2.0)
    };

    let ray_dir_x = end_pos.x - start_pos.x;
    let ray_dir_z = end_pos.z - start_pos.z;

    let combined_half_x = half_x + wall_half_x;
    let combined_half_z = half_z + wall_half_z;

    let local_x = start_pos.x - wall_center_x;
    let local_z = start_pos.z - wall_center_z;

    let mut t_min = 0.0_f32;
    let mut t_max = 1.0_f32;

    if let Some((min_x, max_x)) = sweep_slab_interval(local_x, ray_dir_x, combined_half_x, t_min, t_max) {
        t_min = min_x;
        t_max = max_x;
    } else {
        return false;
    }

    if let Some((min_z, max_z)) = sweep_slab_interval(local_z, ray_dir_z, combined_half_z, t_min, t_max) {
        t_min = min_z;
        t_max = max_z;
    } else {
        return false;
    }

    t_min <= t_max && t_max >= 0.0 && t_min <= 1.0
}

// Shared axis-aligned slide between two candidate positions; collision functions decide validity.
pub fn slide_along_axes(
    current_pos: &Position,
    velocity_x: f32,
    velocity_z: f32,
    delta: f32,
    make_pos_x: impl Fn(f32) -> Position,
    make_pos_z: impl Fn(f32) -> Position,
    collides_x: impl Fn(&Position) -> bool,
    collides_z: impl Fn(&Position) -> bool,
) -> Position {
    // Try full diagonal movement first
    let diagonal_pos = Position {
        x: velocity_x.mul_add(delta, current_pos.x),
        y: current_pos.y,
        z: velocity_z.mul_add(delta, current_pos.z),
    };
    
    // Check if diagonal path collides
    let diagonal_collides = collides_x(&diagonal_pos) || collides_z(&diagonal_pos);
    
    if !diagonal_collides {
        return diagonal_pos;
    }
    
    // Diagonal blocked, try axis-aligned sliding
    let x_only_pos = make_pos_x(delta);
    let z_only_pos = make_pos_z(delta);

    let x_collides = collides_x(&x_only_pos);
    let z_collides = collides_z(&z_only_pos);

    if !x_collides {
        x_only_pos
    } else if !z_collides {
        z_only_pos
    } else {
        *current_pos
    }
}

// Helper is intentionally small; higher-level sliding lives with players/sentries.
