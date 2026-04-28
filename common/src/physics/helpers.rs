use bevy::prelude::*;

use crate::{
    constants::{LEVEL_HEIGHT, PHYSICS_EPSILON, WALL_HEIGHT},
    protocol::{Floor, Position, Wall},
};

// Result of a sweep collision test: surface normal and time of impact.
#[derive(Debug, Clone, Copy)]
pub struct Collision {
    pub normal: Vec3,
    pub t: f32,
}

// Axis-aligned cuboid represented by center position and half extents.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cuboid {
    pub center: Vec3,
    pub half_extents: Vec3,
}

#[must_use]
pub fn wall_cuboid(wall: &Wall, radius: f32) -> Cuboid {
    let dx = (wall.x2 - wall.x1).abs();
    let dz = (wall.z2 - wall.z1).abs();
    let wall_half_thickness = wall.width / 2.0;
    let is_horizontal = dx > dz;

    Cuboid {
        center: Vec3::new(
            f32::midpoint(wall.x1, wall.x2),
            f32::from(wall.level).mul_add(LEVEL_HEIGHT, WALL_HEIGHT / 2.0),
            f32::midpoint(wall.z1, wall.z2),
        ),
        half_extents: Vec3::new(
            if is_horizontal { dx / 2.0 } else { wall_half_thickness } + radius,
            WALL_HEIGHT / 2.0 + radius,
            if is_horizontal { wall_half_thickness } else { dz / 2.0 } + radius,
        ),
    }
}

#[must_use]
pub fn floor_cuboid(floor: &Floor, radius: f32) -> Cuboid {
    let (min_x, max_x, min_z, max_z) = floor.bounds_xz();

    Cuboid {
        center: Vec3::new(
            f32::midpoint(min_x, max_x),
            floor.y - floor.thickness / 2.0,
            f32::midpoint(min_z, max_z),
        ),
        half_extents: Vec3::new(
            (max_x - min_x) / 2.0 + radius,
            floor.thickness / 2.0 + radius,
            (max_z - min_z) / 2.0 + radius,
        ),
    }
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

// Boolean segment vs axis-aligned cuboid intersection. This treats a segment
// starting inside the cuboid as intersecting, which is useful for spawn/clearance
// tests where "already inside" should still block.
#[must_use]
pub fn segment_intersects_cuboid(start: &Position, end: &Position, cuboid: Cuboid) -> bool {
    let local = Vec3::from(*start) - cuboid.center;
    let ray_dir = Vec3::from(*end) - Vec3::from(*start);

    let mut t_min = 0.0_f32;
    let mut t_max = 1.0_f32;

    if let Some((new_min, new_max)) = sweep_slab_interval(local.x, ray_dir.x, cuboid.half_extents.x, t_min, t_max) {
        t_min = new_min;
        t_max = new_max;
    } else {
        return false;
    }

    if let Some((new_min, new_max)) = sweep_slab_interval(local.y, ray_dir.y, cuboid.half_extents.y, t_min, t_max) {
        t_min = new_min;
        t_max = new_max;
    } else {
        return false;
    }

    if let Some((new_min, new_max)) = sweep_slab_interval(local.z, ray_dir.z, cuboid.half_extents.z, t_min, t_max) {
        t_min = new_min;
        t_max = new_max;
    } else {
        return false;
    }

    t_min <= t_max && t_max >= 0.0 && t_min <= 1.0
}

// Swept point vs axis-aligned cuboid; returns collision info if within [0,1].
// A point starting inside the cuboid returns `None` because there is no entering
// face normal to report. Use `segment_intersects_cuboid` for boolean overlap.
#[must_use]
pub fn sweep_point_vs_cuboid(proj_pos: &Position, ray_dir: Vec3, cuboid: Cuboid) -> Option<Collision> {
    let local = Vec3::from(*proj_pos) - cuboid.center;

    let mut t_enter = 0.0_f32;
    let mut t_exit = 1.0_f32;
    let mut hit_normal = Vec3::ZERO;

    if ray_dir.x.abs() < PHYSICS_EPSILON {
        if local.x.abs() > cuboid.half_extents.x {
            return None;
        }
    } else {
        let tx1 = (-cuboid.half_extents.x - local.x) / ray_dir.x;
        let tx2 = (cuboid.half_extents.x - local.x) / ray_dir.x;
        let (tx_min, tx_max) = if tx1 < tx2 { (tx1, tx2) } else { (tx2, tx1) };
        if tx_min > t_enter {
            t_enter = tx_min;
            hit_normal = Vec3::new(if ray_dir.x > 0.0 { -1.0 } else { 1.0 }, 0.0, 0.0);
        }
        t_exit = t_exit.min(tx_max);
        if t_enter > t_exit {
            return None;
        }
    }

    if ray_dir.y.abs() < PHYSICS_EPSILON {
        if local.y.abs() > cuboid.half_extents.y {
            return None;
        }
    } else {
        let ty1 = (-cuboid.half_extents.y - local.y) / ray_dir.y;
        let ty2 = (cuboid.half_extents.y - local.y) / ray_dir.y;
        let (ty_min, ty_max) = if ty1 < ty2 { (ty1, ty2) } else { (ty2, ty1) };
        if ty_min > t_enter {
            t_enter = ty_min;
            hit_normal = Vec3::new(0.0, if ray_dir.y > 0.0 { -1.0 } else { 1.0 }, 0.0);
        }
        t_exit = t_exit.min(ty_max);
        if t_enter > t_exit {
            return None;
        }
    }

    if ray_dir.z.abs() < PHYSICS_EPSILON {
        if local.z.abs() > cuboid.half_extents.z {
            return None;
        }
    } else {
        let tz1 = (-cuboid.half_extents.z - local.z) / ray_dir.z;
        let tz2 = (cuboid.half_extents.z - local.z) / ray_dir.z;
        let (tz_min, tz_max) = if tz1 < tz2 { (tz1, tz2) } else { (tz2, tz1) };
        if tz_min > t_enter {
            t_enter = tz_min;
            hit_normal = Vec3::new(0.0, 0.0, if ray_dir.z > 0.0 { -1.0 } else { 1.0 });
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{FLOOR_THICKNESS, WALL_THICKNESS};

    fn unit_cuboid() -> Cuboid {
        Cuboid {
            center: Vec3::ZERO,
            half_extents: Vec3::splat(1.0),
        }
    }

    #[test]
    fn segment_intersects_cuboid_when_crossing() {
        let cuboid = unit_cuboid();
        let start = Position {
            x: -2.0,
            y: 0.0,
            z: 0.0,
        };
        let end = Position { x: 2.0, y: 0.0, z: 0.0 };

        assert!(segment_intersects_cuboid(&start, &end, cuboid));
    }

    #[test]
    fn segment_intersects_cuboid_when_starting_inside() {
        let cuboid = unit_cuboid();
        let start = Position::default();
        let end = Position { x: 2.0, y: 0.0, z: 0.0 };

        assert!(segment_intersects_cuboid(&start, &end, cuboid));
    }

    #[test]
    fn segment_misses_cuboid() {
        let cuboid = unit_cuboid();
        let start = Position {
            x: -2.0,
            y: 2.0,
            z: 0.0,
        };
        let end = Position { x: 2.0, y: 2.0, z: 0.0 };

        assert!(!segment_intersects_cuboid(&start, &end, cuboid));
    }

    #[test]
    fn sweep_point_vs_cuboid_returns_none_when_starting_inside() {
        let cuboid = unit_cuboid();
        let start = Position::default();

        assert!(sweep_point_vs_cuboid(&start, Vec3::X, cuboid).is_none());
    }

    #[test]
    fn sweep_point_vs_cuboid_reports_entering_normal() {
        let cuboid = unit_cuboid();
        let start = Position {
            x: -2.0,
            y: 0.0,
            z: 0.0,
        };
        let collision =
            sweep_point_vs_cuboid(&start, Vec3::new(4.0, 0.0, 0.0), cuboid).expect("segment should enter cuboid");

        assert_eq!(collision.normal, Vec3::NEG_X);
        assert!((collision.t - 0.25).abs() < PHYSICS_EPSILON);
    }

    #[test]
    fn wall_cuboid_uses_wall_level_height() {
        let wall = Wall {
            x1: -2.0,
            z1: 1.0,
            x2: 2.0,
            z2: 1.0,
            width: WALL_THICKNESS,
            level: 1,
        };
        let cuboid = wall_cuboid(&wall, 0.1);

        assert_eq!(cuboid.center, Vec3::new(0.0, LEVEL_HEIGHT + WALL_HEIGHT / 2.0, 1.0));
        assert_eq!(
            cuboid.half_extents,
            Vec3::new(2.1, WALL_HEIGHT / 2.0 + 0.1, WALL_THICKNESS / 2.0 + 0.1)
        );
    }

    #[test]
    fn floor_cuboid_uses_floor_surface_and_thickness() {
        let floor = Floor {
            x1: -2.0,
            z1: -4.0,
            x2: 2.0,
            z2: 4.0,
            y: LEVEL_HEIGHT,
            thickness: FLOOR_THICKNESS,
            level: 1,
        };
        let cuboid = floor_cuboid(&floor, 0.1);

        assert_eq!(cuboid.center, Vec3::new(0.0, LEVEL_HEIGHT - FLOOR_THICKNESS / 2.0, 0.0));
        assert_eq!(cuboid.half_extents, Vec3::new(2.1, FLOOR_THICKNESS / 2.0 + 0.1, 4.1));
    }
}
