use bevy_ecs::prelude::*;
use bevy_math::Vec3;
use bevy_time::{Timer, TimerMode};

use super::helpers::{Collision, sweep_point_vs_cuboid, sweep_slab_interval};
use crate::{
    constants::*,
    protocol::{Position, Ramp, Roof, Wall},
};

// Direction of a projectile hit (normalized XZ vector).
#[derive(Debug, Clone, Copy)]
pub struct HitDirection {
    pub x: f32,
    pub z: f32,
}

// Component attached to projectile entities to track velocity, lifetime, and bounce behavior
#[derive(Component)]
pub struct Projectile {
    pub velocity: Vec3,
    pub lifetime: Timer,
}

impl Projectile {
    #[must_use]
    pub fn new(face_dir: f32, face_pitch: f32) -> Self {
        let pitch_sin = face_pitch.sin();
        let pitch_cos = face_pitch.cos();
        let velocity = Vec3::new(
            face_dir.sin() * pitch_cos * PROJECTILE_SPEED,
            pitch_sin * PROJECTILE_SPEED,
            face_dir.cos() * pitch_cos * PROJECTILE_SPEED,
        );

        Self {
            velocity,
            lifetime: Timer::from_seconds(PROJECTILE_LIFETIME, TimerMode::Once),
        }
    }

    #[must_use]
    pub fn handle_ramp_bounce(&mut self, projectile_pos: &Position, delta: f32, ramp: &Ramp) -> Option<Position> {
        let collision = sweep_projectile_vs_ramp(projectile_pos, self, delta, ramp)?;
        Some(self.apply_bounce(projectile_pos, delta, collision))
    }

    #[must_use]
    pub fn handle_wall_bounce(&mut self, projectile_pos: &Position, delta: f32, wall: &Wall) -> Option<Position> {
        let collision = sweep_projectile_vs_wall(projectile_pos, self, delta, wall)?;
        Some(self.apply_bounce(projectile_pos, delta, collision))
    }

    #[must_use]
    pub fn handle_roof_bounce(&mut self, projectile_pos: &Position, delta: f32, roof: &Roof) -> Option<Position> {
        let collision = sweep_projectile_vs_roof(projectile_pos, self, delta, roof)?;
        Some(self.apply_bounce(projectile_pos, delta, collision))
    }

    #[must_use]
    pub fn handle_ground_bounce(&mut self, projectile_pos: &Position, delta: f32) -> Option<Position> {
        if self.velocity.y >= 0.0 {
            return None;
        }

        let t = (PROJECTILE_RADIUS - projectile_pos.y) / (self.velocity.y * delta);
        if !(0.0..=1.0).contains(&t) {
            return None;
        }

        let collision = Collision { normal: Vec3::Y, t };
        Some(self.apply_bounce(projectile_pos, delta, collision))
    }

    // Applies bounce physics: reflects velocity off the surface and returns the new position.
    fn apply_bounce(&mut self, projectile_pos: &Position, delta: f32, collision: Collision) -> Position {
        // Calculate collision point
        let collision_pos = Vec3::from(*projectile_pos) + self.velocity * delta * collision.t;

        // Reflect velocity: v' = v - 2(v·n)n
        let dot = self.velocity.dot(collision.normal);
        self.velocity -= 2.0 * dot * collision.normal;

        // Separate from surface and continue with remaining time
        const SEPARATION_EPSILON: f32 = 0.01;
        let separated_pos = collision_pos + collision.normal * SEPARATION_EPSILON;
        let remaining_time = delta * (1.0 - collision.t);

        (separated_pos + self.velocity * remaining_time).into()
    }
}

// === Projectile sweep helpers ===

fn sweep_projectile_vs_ramp(
    proj_pos: &Position,
    projectile: &Projectile,
    delta: f32,
    ramp: &Ramp,
) -> Option<Collision> {
    let (min_x, max_x, min_z, max_z) = ramp.bounds_xz();
    let (min_y, max_y) = ramp.bounds_y();

    let ray_dir_x = projectile.velocity.x * delta;
    let ray_dir_y = projectile.velocity.y * delta;
    let ray_dir_z = projectile.velocity.z * delta;

    let seg_min_x = proj_pos.x.min(proj_pos.x + ray_dir_x) - PROJECTILE_RADIUS;
    let seg_max_x = proj_pos.x.max(proj_pos.x + ray_dir_x) + PROJECTILE_RADIUS;
    let seg_min_y = proj_pos.y.min(proj_pos.y + ray_dir_y) - PROJECTILE_RADIUS;
    let seg_max_y = proj_pos.y.max(proj_pos.y + ray_dir_y) + PROJECTILE_RADIUS;
    let seg_min_z = proj_pos.z.min(proj_pos.z + ray_dir_z) - PROJECTILE_RADIUS;
    let seg_max_z = proj_pos.z.max(proj_pos.z + ray_dir_z) + PROJECTILE_RADIUS;

    if seg_max_x < min_x || seg_min_x > max_x || seg_max_z < min_z || seg_min_z > max_z {
        return None;
    }
    if seg_max_y < min_y - PROJECTILE_RADIUS || seg_min_y > max_y + PROJECTILE_RADIUS {
        return None;
    }

    let along_x = (ramp.x2 - ramp.x1).abs() >= (ramp.z2 - ramp.z1).abs();
    let slope = if along_x {
        (ramp.y2 - ramp.y1) / (ramp.x2 - ramp.x1 + PHYSICS_EPSILON)
    } else {
        (ramp.y2 - ramp.y1) / (ramp.z2 - ramp.z1 + PHYSICS_EPSILON)
    };

    let height_at = |x: f32, z: f32| {
        if along_x {
            let t = ((x - ramp.x1) / (ramp.x2 - ramp.x1 + PHYSICS_EPSILON)).clamp(0.0, 1.0);
            ramp.y1 + t * (ramp.y2 - ramp.y1)
        } else {
            let t = ((z - ramp.z1) / (ramp.z2 - ramp.z1 + PHYSICS_EPSILON)).clamp(0.0, 1.0);
            ramp.y1 + t * (ramp.y2 - ramp.y1)
        }
    };

    let exp_min_x = min_x - PROJECTILE_RADIUS;
    let exp_max_x = max_x + PROJECTILE_RADIUS;
    let exp_min_z = min_z - PROJECTILE_RADIUS;
    let exp_max_z = max_z + PROJECTILE_RADIUS;

    let mut t_enter = 0.0_f32;
    let mut t_exit = 1.0_f32;
    let mut hit_normal_x = 0.0_f32;
    let mut hit_normal_z = 0.0_f32;

    if ray_dir_x.abs() < PHYSICS_EPSILON {
        if proj_pos.x < exp_min_x || proj_pos.x > exp_max_x {
            return None;
        }
    } else {
        let tx1 = (exp_min_x - proj_pos.x) / ray_dir_x;
        let tx2 = (exp_max_x - proj_pos.x) / ray_dir_x;
        let (tx_min, tx_max) = if tx1 < tx2 { (tx1, tx2) } else { (tx2, tx1) };
        if tx_min > t_enter {
            t_enter = tx_min;
            hit_normal_x = if tx1 < tx2 { -1.0 } else { 1.0 };
            hit_normal_z = 0.0;
        }
        if tx_max < t_exit {
            t_exit = tx_max;
        }
        if t_enter > t_exit || t_exit < 0.0 || t_enter > 1.0 {
            return None;
        }
    }

    if ray_dir_z.abs() < PHYSICS_EPSILON {
        if proj_pos.z < exp_min_z || proj_pos.z > exp_max_z {
            return None;
        }
    } else {
        let tz1 = (exp_min_z - proj_pos.z) / ray_dir_z;
        let tz2 = (exp_max_z - proj_pos.z) / ray_dir_z;
        let (tz_min, tz_max) = if tz1 < tz2 { (tz1, tz2) } else { (tz2, tz1) };
        if tz_min > t_enter {
            t_enter = tz_min;
            hit_normal_x = 0.0;
            hit_normal_z = if tz1 < tz2 { -1.0 } else { 1.0 };
        }
        if tz_max < t_exit {
            t_exit = tz_max;
        }
        if t_enter > t_exit || t_exit < 0.0 || t_enter > 1.0 {
            return None;
        }
    }

    let mut best_t = f32::INFINITY;
    let mut best_normal = (0.0_f32, 0.0_f32, 0.0_f32);

    let test_side = |t: f32, nx: f32, nz: f32, height_at: &dyn Fn(f32, f32) -> f32| -> Option<Collision> {
        if !(0.0..=1.0).contains(&t) {
            return None;
        }
        let cx = ray_dir_x.mul_add(t, proj_pos.x);
        let cz = ray_dir_z.mul_add(t, proj_pos.z);
        let cy = ray_dir_y.mul_add(t, proj_pos.y);

        let clamped_x = cx.clamp(min_x, max_x);
        let clamped_z = cz.clamp(min_z, max_z);
        let h = height_at(clamped_x, clamped_z) + PROJECTILE_RADIUS;
        let floor = min_y - PROJECTILE_RADIUS;

        if cy >= floor && cy <= h {
            Some(Collision {
                normal: Vec3::new(nx, 0.0, nz),
                t,
            })
        } else {
            None
        }
    };

    if let Some(collision) = test_side(t_enter, hit_normal_x, hit_normal_z, &height_at) {
        best_t = collision.t;
        best_normal = (collision.normal.x, collision.normal.y, collision.normal.z);
    }

    let height_linear = if along_x {
        let c0 = (proj_pos.x - ramp.x1).mul_add(slope, ramp.y1);
        let c1 = slope * ray_dir_x;
        (c0, c1)
    } else {
        let c0 = (proj_pos.z - ramp.z1).mul_add(slope, ramp.y1);
        let c1 = slope * ray_dir_z;
        (c0, c1)
    };

    let top_c0 = height_linear.0 + PROJECTILE_RADIUS;
    let top_c1 = height_linear.1;
    let f0 = proj_pos.y - top_c0;
    let f1 = ray_dir_y - top_c1;

    let mut top_hit: Option<f32> = None;
    if f1.abs() < PHYSICS_EPSILON {
        if f0 <= 0.0 {
            top_hit = Some(0.0);
        }
    } else {
        let t_top = -f0 / f1;
        if (0.0..=1.0).contains(&t_top) {
            top_hit = Some(t_top);
        }
    }

    if let Some(t_top) = top_hit {
        let cx = ray_dir_x.mul_add(t_top, proj_pos.x);
        let cz = ray_dir_z.mul_add(t_top, proj_pos.z);
        if cx >= min_x - PHYSICS_EPSILON
            && cx <= max_x + PHYSICS_EPSILON
            && cz >= min_z - PHYSICS_EPSILON
            && cz <= max_z + PHYSICS_EPSILON
            && t_top < best_t
        {
            let denom = slope.mul_add(slope, 1.0).sqrt();
            let normal_x = if along_x { -slope / denom } else { 0.0 };
            let normal_z = if along_x { 0.0 } else { -slope / denom };
            let normal_y = 1.0 / denom;
            best_t = t_top;
            best_normal = (normal_x, normal_y, normal_z);
        }
    }

    if best_t.is_finite() {
        Some(Collision {
            normal: Vec3::new(best_normal.0, best_normal.1, best_normal.2),
            t: best_t,
        })
    } else {
        None
    }
}

// Generic oriented bounding box collision detection for projectiles vs cuboids
#[must_use]
pub fn sweep_projectile_vs_cuboid(
    proj_pos: &Position,
    projectile: &Projectile,
    delta: f32,
    cuboid_pos: &Position,
    cuboid_center_y: f32,
    cuboid_face_dir: f32,
    cuboid_width: f32,
    cuboid_height: f32,
    cuboid_depth: f32,
) -> Option<HitDirection> {
    let y_diff = (proj_pos.y - cuboid_center_y).abs();
    if y_diff > cuboid_height / 2.0 + PROJECTILE_RADIUS {
        return None;
    }

    let ray_dir_x = projectile.velocity.x * delta;
    let ray_dir_y = projectile.velocity.y * delta;
    let ray_dir_z = projectile.velocity.z * delta;

    let dx = proj_pos.x - cuboid_pos.x;
    let dz = proj_pos.z - cuboid_pos.z;

    // Transform to cuboid's local coordinate system
    let cos_rot = cuboid_face_dir.cos();
    let sin_rot = cuboid_face_dir.sin();

    let local_x = dx.mul_add(cos_rot, -(dz * sin_rot));
    let local_z = dx.mul_add(sin_rot, dz * cos_rot);
    let local_y = proj_pos.y - cuboid_center_y;

    let ray_local_x = ray_dir_x.mul_add(cos_rot, -(ray_dir_z * sin_rot));
    let ray_local_z = ray_dir_x.mul_add(sin_rot, ray_dir_z * cos_rot);
    let ray_local_y = ray_dir_y;

    let half_width = cuboid_width / 2.0 + PROJECTILE_RADIUS;
    let half_height = cuboid_height / 2.0 + PROJECTILE_RADIUS;
    let half_depth = cuboid_depth / 2.0 + PROJECTILE_RADIUS;

    let mut t_min = 0.0_f32;
    let mut t_max = 1.0_f32;

    let (new_min, new_max) = sweep_slab_interval(local_x, ray_local_x, half_width, t_min, t_max)?;
    t_min = new_min;
    t_max = new_max;

    let (new_min, new_max) = sweep_slab_interval(local_y, ray_local_y, half_height, t_min, t_max)?;
    t_min = new_min;
    t_max = new_max;

    let (new_min, new_max) = sweep_slab_interval(local_z, ray_local_z, half_depth, t_min, t_max)?;
    t_min = new_min;
    t_max = new_max;

    if t_min <= t_max && t_max >= 0.0 && t_min <= 1.0 {
        let vel_len = projectile.velocity.x.hypot(projectile.velocity.z);
        let (x, z) = if vel_len > 0.0 {
            (projectile.velocity.x / vel_len, projectile.velocity.z / vel_len)
        } else {
            (0.0, 0.0)
        };

        Some(HitDirection { x, z })
    } else {
        None
    }
}

#[must_use]
pub fn sweep_projectile_vs_player(
    proj_pos: &Position,
    projectile: &Projectile,
    delta: f32,
    player_pos: &Position,
    player_face_dir: f32,
) -> Option<HitDirection> {
    let player_center_y = player_pos.y + PLAYER_HEIGHT / 2.0;
    sweep_projectile_vs_cuboid(
        proj_pos,
        projectile,
        delta,
        player_pos,
        player_center_y,
        player_face_dir,
        PLAYER_WIDTH,
        PLAYER_HEIGHT,
        PLAYER_DEPTH,
    )
}

fn sweep_projectile_vs_wall(
    proj_pos: &Position,
    projectile: &Projectile,
    delta: f32,
    wall: &Wall,
) -> Option<Collision> {
    let wall_center_x = f32::midpoint(wall.x1, wall.x2);
    let wall_center_z = f32::midpoint(wall.z1, wall.z2);
    let wall_center_y = WALL_HEIGHT / 2.0;

    let dx = (wall.x2 - wall.x1).abs();
    let dz = (wall.z2 - wall.z1).abs();
    let wall_half_thickness = wall.width / 2.0;
    let is_horizontal = dx > dz;

    let half_x = if is_horizontal { dx / 2.0 } else { wall_half_thickness } + PROJECTILE_RADIUS;
    let half_z = if is_horizontal { wall_half_thickness } else { dz / 2.0 } + PROJECTILE_RADIUS;
    let half_y = WALL_HEIGHT / 2.0 + PROJECTILE_RADIUS;

    let ray_dir_x = projectile.velocity.x * delta;
    let ray_dir_y = projectile.velocity.y * delta;
    let ray_dir_z = projectile.velocity.z * delta;

    sweep_point_vs_cuboid(
        proj_pos,
        ray_dir_x,
        ray_dir_y,
        ray_dir_z,
        wall_center_x,
        wall_center_y,
        wall_center_z,
        half_x,
        half_y,
        half_z,
    )
}

fn sweep_projectile_vs_roof(
    proj_pos: &Position,
    projectile: &Projectile,
    delta: f32,
    roof: &Roof,
) -> Option<Collision> {
    let (min_x, max_x, min_z, max_z) = roof.bounds_xz();

    let center_x = f32::midpoint(min_x, max_x);
    let center_z = f32::midpoint(min_z, max_z);
    let center_y = ROOF_HEIGHT - roof.thickness / 2.0;

    let half_x = (max_x - min_x) / 2.0 + PROJECTILE_RADIUS;
    let half_z = (max_z - min_z) / 2.0 + PROJECTILE_RADIUS;
    let half_y = roof.thickness / 2.0 + PROJECTILE_RADIUS;

    let ray_dir_x = projectile.velocity.x * delta;
    let ray_dir_y = projectile.velocity.y * delta;
    let ray_dir_z = projectile.velocity.z * delta;

    sweep_point_vs_cuboid(
        proj_pos, ray_dir_x, ray_dir_y, ray_dir_z, center_x, center_y, center_z, half_x, half_y, half_z,
    )
}

// Sample-based ramp hit test for projectiles.
#[must_use]
pub fn projectile_hits_ramp(proj_pos: &Position, projectile_velocity: &Vec3, delta: f32, ramp: &Ramp) -> bool {
    let (min_x, max_x, min_z, max_z) = ramp.bounds_xz();
    let num_samples = 5;
    for i in 0..=num_samples {
        let t = i as f32 / num_samples as f32;
        let sample_x = (projectile_velocity.x * delta).mul_add(t, proj_pos.x);
        let sample_y = (projectile_velocity.y * delta).mul_add(t, proj_pos.y);
        let sample_z = (projectile_velocity.z * delta).mul_add(t, proj_pos.z);

        if sample_x >= min_x && sample_x <= max_x && sample_z >= min_z && sample_z <= max_z {
            let ramp_height = crate::map::height_on_ramp(&[*ramp], sample_x, sample_z);

            if (sample_y - ramp_height).abs() < PROJECTILE_RADIUS * 2.0 {
                return true;
            }
        }
    }

    false
}

#[must_use]
pub fn projectile_hits_sentry(
    proj_pos: &Position,
    projectile: &Projectile,
    delta: f32,
    sentry_pos: &Position,
    sentry_face_dir: f32,
) -> bool {
    let sentry_center_y = sentry_pos.y + SENTRY_HEIGHT / 2.0;
    sweep_projectile_vs_cuboid(
        proj_pos,
        projectile,
        delta,
        sentry_pos,
        sentry_center_y,
        sentry_face_dir,
        SENTRY_WIDTH,
        SENTRY_HEIGHT,
        SENTRY_DEPTH,
    )
    .is_some()
}
