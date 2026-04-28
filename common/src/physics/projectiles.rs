use std::sync::LazyLock;

use bevy_ecs::prelude::*;
use bevy_math::Vec3;
use bevy_time::{Timer, TimerMode};

use super::helpers::{Collision, sweep_point_vs_cuboid, sweep_slab_interval};
use crate::{
    constants::*,
    protocol::{Floor, Position, Ramp, Wall},
};

// Direction of a projectile hit (normalized XZ vector).
#[derive(Debug, Clone, Copy)]
pub struct HitDirection {
    pub x: f32,
    pub z: f32,
}

const SEPARATION_EPSILON: f32 = 0.01;
const MAX_SURFACE_BOUNCES: usize = 3;

// Velocity gained from falling the epsilon separation distance: sqrt(2 * g * h)
static EPSILON_FALL_VELOCITY: LazyLock<f32> = LazyLock::new(|| (2.0 * PROJECTILE_GRAVITY * SEPARATION_EPSILON).sqrt());

// Component attached to projectile entities to track velocity, lifetime, and bounce behavior
#[derive(Component)]
pub struct ProjectileMotion {
    pub velocity: Vec3,
    pub lifetime: Timer,
}

impl ProjectileMotion {
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

    // Applies gravity to the projectile's velocity.
    pub fn apply_gravity(&mut self, delta: f32) {
        if PROJECTILE_GRAVITY > 0.0 {
            self.velocity.y -= PROJECTILE_GRAVITY * delta;
        }
    }

    // Applies air resistance (drag) to the projectile's velocity.
    // Drag force opposes motion and is proportional to velocity squared.
    pub fn apply_drag(&mut self, delta: f32) {
        if PROJECTILE_DRAG_FACTOR > 0.0 {
            let speed = self.velocity.length();
            if speed > PHYSICS_EPSILON {
                // Deceleration magnitude = drag_factor * v^2
                let deceleration = PROJECTILE_DRAG_FACTOR * speed * speed;
                // Apply deceleration opposite to velocity direction
                let speed_reduction = deceleration * delta;
                // Don't reduce speed below zero
                let new_speed = (speed - speed_reduction).max(0.0);
                self.velocity = self.velocity.normalize() * new_speed;
            }
        }
    }

    // Reflect velocity about the surface normal with angle-dependent energy retention.
    fn reflect_with_retention(&mut self, normal: Vec3) {
        // Calculate impact angle: |dot|/speed = cos(angle from surface normal)
        // cos = 1.0 means head-on (perpendicular), cos = 0.0 means glancing (parallel)
        let speed = self.velocity.length();
        let dot = self.velocity.dot(normal);
        let cos_impact = if speed > PHYSICS_EPSILON {
            (dot.abs() / speed).min(1.0)
        } else {
            0.0
        };

        // Reflect velocity: v' = v - 2(v·n)n
        self.velocity -= 2.0 * dot * normal;

        // Apply energy loss based on impact angle:
        // - Head-on (cos=1): full energy loss (use PROJECTILE_BOUNCE_RETENTION)
        // - Glancing (cos=0): minimal energy loss (retention = 1.0)
        let retention = 1.0 - cos_impact * (1.0 - PROJECTILE_BOUNCE_RETENTION);
        self.velocity *= retention;
    }

    // Advance to collision point, reflect velocity, return separated position and remaining delta time.
    fn step_after_collision(&mut self, start_pos: &Position, delta: f32, collision: Collision) -> (Position, f32) {
        let collision_pos = Vec3::from(*start_pos) + self.velocity * delta * collision.t;
        self.reflect_with_retention(collision.normal);
        let separated_pos = collision_pos + collision.normal * SEPARATION_EPSILON;

        // Compensate for energy the epsilon push adds. Gravity will convert the Y component
        // of the push into downward velocity, so subtract that amount to keep energy balanced.
        self.velocity.y -= *EPSILON_FALL_VELOCITY * collision.normal.y;

        let remaining_time = delta * (1.0 - collision.t);
        (separated_pos.into(), remaining_time)
    }

    // Applies bounce physics: reflects velocity off the surface and returns the new position.
    #[must_use]
    pub fn handle_bounces(
        &mut self,
        projectile_pos: &Position,
        delta: f32,
        walls: &[Wall],
        floors: &[Floor],
        ramps: &[Ramp],
    ) -> Option<Position> {
        let mut current_pos = *projectile_pos;
        let mut remaining_delta = delta;
        let mut collided = false;

        for _ in 0..MAX_SURFACE_BOUNCES {
            let mut earliest: Option<Collision> = None;

            for wall in walls {
                if let Some(collision) = sweep_projectile_vs_wall(&current_pos, self, remaining_delta, wall)
                    && earliest.is_none_or(|current| collision.t < current.t)
                {
                    earliest = Some(collision);
                }
            }

            for floor in floors {
                if let Some(collision) = sweep_projectile_vs_floor(&current_pos, self, remaining_delta, floor)
                    && earliest.is_none_or(|current| collision.t < current.t)
                {
                    earliest = Some(collision);
                }
            }

            for ramp in ramps {
                if let Some(collision) = sweep_projectile_vs_ramp(&current_pos, self, remaining_delta, ramp)
                    && earliest.is_none_or(|current| collision.t < current.t)
                {
                    earliest = Some(collision);
                }
            }

            if let Some(collision) = sweep_projectile_vs_ground(&current_pos, self, remaining_delta)
                && earliest.is_none_or(|current| collision.t < current.t)
            {
                earliest = Some(collision);
            }

            let Some(collision) = earliest else {
                break;
            };

            let (next_pos, next_delta) = self.step_after_collision(&current_pos, remaining_delta, collision);
            current_pos = next_pos;
            remaining_delta = next_delta;
            collided = true;

            if remaining_delta <= PHYSICS_EPSILON {
                break;
            }
        }

        if collided {
            let final_pos = Vec3::from(current_pos) + self.velocity * remaining_delta;
            Some(final_pos.into())
        } else {
            None
        }
    }
}

// === Projectile sweep helpers ===

fn sweep_projectile_vs_ground(proj_pos: &Position, proj_motion: &ProjectileMotion, delta: f32) -> Option<Collision> {
    let half_width = FIELD_WIDTH / 2.0;
    let half_depth = FIELD_DEPTH / 2.0;

    // No ground outside the playing field (field is centered at origin)
    if proj_pos.x < -half_width || proj_pos.x > half_width || proj_pos.z < -half_depth || proj_pos.z > half_depth {
        return None;
    }

    let ground_level = PROJECTILE_RADIUS;

    // If already at or below ground and moving downward, treat as immediate collision (t=0)
    if proj_pos.y <= ground_level {
        if proj_motion.velocity.y >= 0.0 {
            return None;
        }
        return Some(Collision {
            normal: Vec3::Y,
            t: 0.0,
        });
    }

    // Sweep test: will we hit the ground this frame?
    if proj_motion.velocity.y >= 0.0 {
        return None;
    }

    let t = (ground_level - proj_pos.y) / (proj_motion.velocity.y * delta);
    if !(0.0..=1.0).contains(&t) {
        return None;
    }

    // Check if collision point is within field bounds
    let collision = Vec3::from(*proj_pos) + proj_motion.velocity * delta * t;
    if collision.x < -half_width || collision.x > half_width || collision.z < -half_depth || collision.z > half_depth {
        return None;
    }

    Some(Collision { normal: Vec3::Y, t })
}

fn sweep_projectile_vs_ramp(
    proj_pos: &Position,
    proj_motion: &ProjectileMotion,
    delta: f32,
    ramp: &Ramp,
) -> Option<Collision> {
    let (min_x, max_x, min_z, max_z) = ramp.bounds_xz();
    let (min_y, max_y) = ramp.bounds_y();

    let ray_dir = proj_motion.velocity * delta;

    let pos = Vec3::from(*proj_pos);
    let end = pos + ray_dir;
    let seg_min = pos.min(end) - PROJECTILE_RADIUS;
    let seg_max = pos.max(end) + PROJECTILE_RADIUS;
    if seg_max.x < min_x || seg_min.x > max_x || seg_max.z < min_z || seg_min.z > max_z {
        return None;
    }
    if seg_max.y < min_y - PROJECTILE_RADIUS || seg_min.y > max_y + PROJECTILE_RADIUS {
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

    if ray_dir.x.abs() < PHYSICS_EPSILON {
        if proj_pos.x < exp_min_x || proj_pos.x > exp_max_x {
            return None;
        }
    } else {
        let tx1 = (exp_min_x - proj_pos.x) / ray_dir.x;
        let tx2 = (exp_max_x - proj_pos.x) / ray_dir.x;
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

    if ray_dir.z.abs() < PHYSICS_EPSILON {
        if proj_pos.z < exp_min_z || proj_pos.z > exp_max_z {
            return None;
        }
    } else {
        let tz1 = (exp_min_z - proj_pos.z) / ray_dir.z;
        let tz2 = (exp_max_z - proj_pos.z) / ray_dir.z;
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
    let mut best_normal = Vec3::ZERO;

    let test_side = |t: f32, nx: f32, nz: f32, height_at: &dyn Fn(f32, f32) -> f32| -> Option<Collision> {
        if !(0.0..=1.0).contains(&t) {
            return None;
        }
        let c = pos + ray_dir * t;

        let clamped_x = c.x.clamp(min_x, max_x);
        let clamped_z = c.z.clamp(min_z, max_z);
        let h = height_at(clamped_x, clamped_z) + PROJECTILE_RADIUS;
        let floor = min_y - PROJECTILE_RADIUS;

        if c.y >= floor && c.y <= h {
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
        best_normal = collision.normal;
    }

    let height_linear = if along_x {
        let c0 = (proj_pos.x - ramp.x1).mul_add(slope, ramp.y1);
        let c1 = slope * ray_dir.x;
        (c0, c1)
    } else {
        let c0 = (proj_pos.z - ramp.z1).mul_add(slope, ramp.y1);
        let c1 = slope * ray_dir.z;
        (c0, c1)
    };

    let top_c0 = height_linear.0 + PROJECTILE_RADIUS;
    let top_c1 = height_linear.1;
    let f0 = proj_pos.y - top_c0;
    let f1 = ray_dir.y - top_c1;

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
        let c = pos + ray_dir * t_top;
        if c.x >= min_x - PHYSICS_EPSILON
            && c.x <= max_x + PHYSICS_EPSILON
            && c.z >= min_z - PHYSICS_EPSILON
            && c.z <= max_z + PHYSICS_EPSILON
            && t_top < best_t
        {
            let denom = slope.mul_add(slope, 1.0).sqrt();
            best_t = t_top;
            best_normal = Vec3::new(
                if along_x { -slope / denom } else { 0.0 },
                1.0 / denom,
                if along_x { 0.0 } else { -slope / denom },
            );
        }
    }

    if best_t.is_finite() {
        Some(Collision {
            normal: best_normal,
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
    proj_motion: &ProjectileMotion,
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

    let ray_dir = proj_motion.velocity * delta;

    let dx = proj_pos.x - cuboid_pos.x;
    let dz = proj_pos.z - cuboid_pos.z;

    // Transform to cuboid's local coordinate system
    let cos_rot = cuboid_face_dir.cos();
    let sin_rot = cuboid_face_dir.sin();

    let local_x = dx.mul_add(cos_rot, -(dz * sin_rot));
    let local_z = dx.mul_add(sin_rot, dz * cos_rot);
    let local_y = proj_pos.y - cuboid_center_y;

    let ray_local_x = ray_dir.x.mul_add(cos_rot, -(ray_dir.z * sin_rot));
    let ray_local_z = ray_dir.x.mul_add(sin_rot, ray_dir.z * cos_rot);
    let ray_local_y = ray_dir.y;

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
        let vel_len = proj_motion.velocity.x.hypot(proj_motion.velocity.z);
        let (x, z) = if vel_len > 0.0 {
            (proj_motion.velocity.x / vel_len, proj_motion.velocity.z / vel_len)
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
    proj_motion: &ProjectileMotion,
    delta: f32,
    player_pos: &Position,
    player_face_dir: f32,
) -> Option<HitDirection> {
    let player_center_y = player_pos.y + PLAYER_HEIGHT / 2.0;
    sweep_projectile_vs_cuboid(
        proj_pos,
        proj_motion,
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
    proj_motion: &ProjectileMotion,
    delta: f32,
    wall: &Wall,
) -> Option<Collision> {
    let dx = (wall.x2 - wall.x1).abs();
    let dz = (wall.z2 - wall.z1).abs();
    let wall_half_thickness = wall.width / 2.0;
    let is_horizontal = dx > dz;

    let center = Vec3::new(
        f32::midpoint(wall.x1, wall.x2),
        f32::from(wall.level).mul_add(LEVEL_HEIGHT, WALL_HEIGHT / 2.0),
        f32::midpoint(wall.z1, wall.z2),
    );
    let half_extents = Vec3::new(
        if is_horizontal { dx / 2.0 } else { wall_half_thickness } + PROJECTILE_RADIUS,
        WALL_HEIGHT / 2.0 + PROJECTILE_RADIUS,
        if is_horizontal { wall_half_thickness } else { dz / 2.0 } + PROJECTILE_RADIUS,
    );

    sweep_point_vs_cuboid(proj_pos, proj_motion.velocity * delta, center, half_extents)
}

fn sweep_projectile_vs_floor(
    proj_pos: &Position,
    proj_motion: &ProjectileMotion,
    delta: f32,
    floor: &Floor,
) -> Option<Collision> {
    let (min_x, max_x, min_z, max_z) = floor.bounds_xz();

    let center = Vec3::new(
        f32::midpoint(min_x, max_x),
        floor.y - floor.thickness / 2.0,
        f32::midpoint(min_z, max_z),
    );
    let half_extents = Vec3::new(
        (max_x - min_x) / 2.0 + PROJECTILE_RADIUS,
        floor.thickness / 2.0 + PROJECTILE_RADIUS,
        (max_z - min_z) / 2.0 + PROJECTILE_RADIUS,
    );

    sweep_point_vs_cuboid(proj_pos, proj_motion.velocity * delta, center, half_extents)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_projectile_motion(velocity: Vec3) -> ProjectileMotion {
        ProjectileMotion {
            velocity,
            lifetime: Timer::from_seconds(PROJECTILE_LIFETIME, TimerMode::Once),
        }
    }

    fn test_wall(level: u8) -> Wall {
        Wall {
            x1: -2.0,
            z1: 1.0,
            x2: 2.0,
            z2: 1.0,
            width: WALL_THICKNESS,
            level,
        }
    }

    #[test]
    fn ground_projectile_ignores_upper_level_wall() {
        let pos = Position {
            x: 0.0,
            y: PROJECTILE_RADIUS,
            z: 0.0,
        };
        let motion = test_projectile_motion(Vec3::new(0.0, 0.0, 20.0));

        assert!(sweep_projectile_vs_wall(&pos, &motion, 0.1, &test_wall(0)).is_some());
        assert!(sweep_projectile_vs_wall(&pos, &motion, 0.1, &test_wall(1)).is_none());
    }

    #[test]
    fn upper_level_projectile_hits_upper_level_wall() {
        let pos = Position {
            x: 0.0,
            y: LEVEL_HEIGHT + PROJECTILE_RADIUS,
            z: 0.0,
        };
        let motion = test_projectile_motion(Vec3::new(0.0, 0.0, 20.0));

        assert!(sweep_projectile_vs_wall(&pos, &motion, 0.1, &test_wall(1)).is_some());
    }
}
