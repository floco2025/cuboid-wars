use bevy_ecs::prelude::*;
use bevy_math::Vec3;
use bevy_time::{Timer, TimerMode};

use super::helpers::{sweep_point_vs_cuboid, sweep_slab_interval};
use crate::{
    constants::*,
    protocol::{Position, Ramp, Roof, Wall},
};

// Result of a projectile hit detection check
#[derive(Debug, Clone, Copy)]
pub struct HitResult {
    pub hit: bool,
    pub hit_dir_x: f32,
    pub hit_dir_z: f32,
}

const fn no_hit() -> HitResult {
    HitResult {
        hit: false,
        hit_dir_x: 0.0,
        hit_dir_z: 0.0,
    }
}

// Component attached to projectile entities to track velocity, lifetime, and bounce behavior
#[derive(Component)]
pub struct Projectile {
    pub velocity: Vec3,
    pub lifetime: Timer,
    pub reflects: bool,
}

impl Projectile {
    #[must_use]
    pub fn new(face_dir: f32, reflects: bool) -> Self {
        let velocity = Vec3::new(
            face_dir.sin() * PROJECTILE_SPEED,
            0.0,
            face_dir.cos() * PROJECTILE_SPEED,
        );

        Self {
            velocity,
            lifetime: Timer::from_seconds(PROJECTILE_LIFETIME, TimerMode::Once),
            reflects,
        }
    }

    #[must_use]
    pub fn handle_ramp_bounce(&mut self, projectile_pos: &Position, delta: f32, ramp: &Ramp) -> Option<Position> {
        if let Some((normal_x, normal_y, normal_z, t_collision)) =
            sweep_projectile_vs_ramp(projectile_pos, self, delta, ramp)
        {
            let collision_x = self.velocity.x.mul_add(delta * t_collision, projectile_pos.x);
            let collision_y = self.velocity.y.mul_add(delta * t_collision, projectile_pos.y);
            let collision_z = self.velocity.z.mul_add(delta * t_collision, projectile_pos.z);

            if self.reflects {
                let dot = self
                    .velocity
                    .x
                    .mul_add(normal_x, self.velocity.y.mul_add(normal_y, self.velocity.z * normal_z));
                self.velocity.x -= 2.0 * dot * normal_x;
                self.velocity.y -= 2.0 * dot * normal_y;
                self.velocity.z -= 2.0 * dot * normal_z;

                const SEPARATION_EPSILON: f32 = 0.01;
                let separated_x = normal_x.mul_add(SEPARATION_EPSILON, collision_x);
                let separated_y = normal_y.mul_add(SEPARATION_EPSILON, collision_y);
                let separated_z = normal_z.mul_add(SEPARATION_EPSILON, collision_z);

                let remaining_time = delta * (1.0 - t_collision);
                Some(Position {
                    x: self.velocity.x.mul_add(remaining_time, separated_x),
                    y: self.velocity.y.mul_add(remaining_time, separated_y),
                    z: self.velocity.z.mul_add(remaining_time, separated_z),
                })
            } else {
                Some(*projectile_pos)
            }
        } else {
            None
        }
    }

    #[must_use]
    pub fn handle_wall_bounce(&mut self, projectile_pos: &Position, delta: f32, wall: &Wall) -> Option<Position> {
        if let Some((normal_x, normal_y, normal_z, t_collision)) =
            sweep_projectile_vs_wall(projectile_pos, self, delta, wall)
        {
            if self.reflects {
                let collision_x = self.velocity.x.mul_add(delta * t_collision, projectile_pos.x);
                let collision_y = self.velocity.y.mul_add(delta * t_collision, projectile_pos.y);
                let collision_z = self.velocity.z.mul_add(delta * t_collision, projectile_pos.z);

                let dot = self
                    .velocity
                    .x
                    .mul_add(normal_x, self.velocity.y.mul_add(normal_y, self.velocity.z * normal_z));
                self.velocity.x -= 2.0 * dot * normal_x;
                self.velocity.y -= 2.0 * dot * normal_y;
                self.velocity.z -= 2.0 * dot * normal_z;

                const SEPARATION_EPSILON: f32 = 0.01;
                let separated_x = normal_x.mul_add(SEPARATION_EPSILON, collision_x);
                let separated_y = normal_y.mul_add(SEPARATION_EPSILON, collision_y);
                let separated_z = normal_z.mul_add(SEPARATION_EPSILON, collision_z);

                let remaining_time = delta * (1.0 - t_collision);
                Some(Position {
                    x: self.velocity.x.mul_add(remaining_time, separated_x),
                    y: self.velocity.y.mul_add(remaining_time, separated_y),
                    z: self.velocity.z.mul_add(remaining_time, separated_z),
                })
            } else {
                Some(*projectile_pos)
            }
        } else {
            None
        }
    }

    #[must_use]
    pub fn handle_roof_bounce(&mut self, projectile_pos: &Position, delta: f32, roof: &Roof) -> Option<Position> {
        if let Some((normal_x, normal_y, normal_z, t_collision)) =
            sweep_projectile_vs_roof(projectile_pos, self, delta, roof)
        {
            if self.reflects {
                let collision_x = self.velocity.x.mul_add(delta * t_collision, projectile_pos.x);
                let collision_y = self.velocity.y.mul_add(delta * t_collision, projectile_pos.y);
                let collision_z = self.velocity.z.mul_add(delta * t_collision, projectile_pos.z);

                let dot = self
                    .velocity
                    .x
                    .mul_add(normal_x, self.velocity.y.mul_add(normal_y, self.velocity.z * normal_z));
                self.velocity.x -= 2.0 * dot * normal_x;
                self.velocity.y -= 2.0 * dot * normal_y;
                self.velocity.z -= 2.0 * dot * normal_z;

                const SEPARATION_EPSILON: f32 = 0.01;
                let separated_x = normal_x.mul_add(SEPARATION_EPSILON, collision_x);
                let separated_y = normal_y.mul_add(SEPARATION_EPSILON, collision_y);
                let separated_z = normal_z.mul_add(SEPARATION_EPSILON, collision_z);

                let remaining_time = delta * (1.0 - t_collision);
                Some(Position {
                    x: self.velocity.x.mul_add(remaining_time, separated_x),
                    y: self.velocity.y.mul_add(remaining_time, separated_y),
                    z: self.velocity.z.mul_add(remaining_time, separated_z),
                })
            } else {
                Some(*projectile_pos)
            }
        } else {
            None
        }
    }

    // Bounce off the ground plane (y=0) if moving downward and reflects is true.
    #[must_use]
    pub fn handle_ground_bounce(&mut self, projectile_pos: &Position, delta: f32) -> Option<Position> {
        let vy = self.velocity.y;

        if vy >= 0.0 {
            return None;
        }

        let t_hit = (PROJECTILE_RADIUS - projectile_pos.y) / (vy * delta);

        if t_hit < 0.0 || t_hit > 1.0 {
            return None;
        }

        let collision_x = self.velocity.x.mul_add(delta * t_hit, projectile_pos.x);
        let collision_y = self.velocity.y.mul_add(delta * t_hit, projectile_pos.y);
        let collision_z = self.velocity.z.mul_add(delta * t_hit, projectile_pos.z);

        let normal_x = 0.0;
        let normal_y = 1.0;
        let normal_z = 0.0;

        if self.reflects {
            let dot = self
                .velocity
                .x
                .mul_add(normal_x, self.velocity.y.mul_add(normal_y, self.velocity.z * normal_z));
            self.velocity.x -= 2.0 * dot * normal_x;
            self.velocity.y -= 2.0 * dot * normal_y;
            self.velocity.z -= 2.0 * dot * normal_z;

            const SEPARATION_EPSILON: f32 = 0.01;
            let separated_x = normal_x.mul_add(SEPARATION_EPSILON, collision_x);
            let separated_y = normal_y.mul_add(SEPARATION_EPSILON, collision_y);
            let separated_z = normal_z.mul_add(SEPARATION_EPSILON, collision_z);

            let remaining_time = delta * (1.0 - t_hit);
            Some(Position {
                x: self.velocity.x.mul_add(remaining_time, separated_x),
                y: self.velocity.y.mul_add(remaining_time, separated_y),
                z: self.velocity.z.mul_add(remaining_time, separated_z),
            })
        } else {
            Some(*projectile_pos)
        }
    }
}

// === Projectile sweep helpers ===

#[must_use]
pub fn sweep_projectile_vs_ramp(
    proj_pos: &Position,
    projectile: &Projectile,
    delta: f32,
    ramp: &Ramp,
) -> Option<(f32, f32, f32, f32)> {
    let min_x = ramp.x1.min(ramp.x2);
    let max_x = ramp.x1.max(ramp.x2);
    let min_z = ramp.z1.min(ramp.z2);
    let max_z = ramp.z1.max(ramp.z2);
    let min_y = ramp.y1.min(ramp.y2);
    let max_y = ramp.y1.max(ramp.y2);

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
        (ramp.y2 - ramp.y1) / (ramp.x2 - ramp.x1 + 1e-6)
    } else {
        (ramp.y2 - ramp.y1) / (ramp.z2 - ramp.z1 + 1e-6)
    };

    let height_at = |x: f32, z: f32| {
        if along_x {
            let t = ((x - ramp.x1) / (ramp.x2 - ramp.x1 + 1e-6)).clamp(0.0, 1.0);
            ramp.y1 + t * (ramp.y2 - ramp.y1)
        } else {
            let t = ((z - ramp.z1) / (ramp.z2 - ramp.z1 + 1e-6)).clamp(0.0, 1.0);
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

    if ray_dir_x.abs() < 1e-6 {
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

    if ray_dir_z.abs() < 1e-6 {
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

    let test_side = |t: f32, nx: f32, nz: f32, height_at: &dyn Fn(f32, f32) -> f32| -> Option<(f32, f32, f32, f32)> {
        if !(0.0..=1.0).contains(&t) {
            return None;
        }
        let cx = proj_pos.x + ray_dir_x * t;
        let cz = proj_pos.z + ray_dir_z * t;
        let cy = proj_pos.y + ray_dir_y * t;

        let clamped_x = cx.clamp(min_x, max_x);
        let clamped_z = cz.clamp(min_z, max_z);
        let h = height_at(clamped_x, clamped_z) + PROJECTILE_RADIUS;
        let floor = min_y - PROJECTILE_RADIUS;

        if cy >= floor && cy <= h {
            Some((nx, 0.0, nz, t))
        } else {
            None
        }
    };

    if let Some((nx, ny, nz, t)) = test_side(t_enter, hit_normal_x, hit_normal_z, &height_at) {
        best_t = t;
        best_normal = (nx, ny, nz);
    }

    let height_linear = if along_x {
        let c0 = ramp.y1 + ((proj_pos.x - ramp.x1) * slope);
        let c1 = slope * ray_dir_x;
        (c0, c1)
    } else {
        let c0 = ramp.y1 + ((proj_pos.z - ramp.z1) * slope);
        let c1 = slope * ray_dir_z;
        (c0, c1)
    };

    let top_c0 = height_linear.0 + PROJECTILE_RADIUS;
    let top_c1 = height_linear.1;
    let f0 = proj_pos.y - top_c0;
    let f1 = ray_dir_y - top_c1;

    let mut top_hit: Option<f32> = None;
    if f1.abs() < 1e-6 {
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
        let cx = proj_pos.x + ray_dir_x * t_top;
        let cz = proj_pos.z + ray_dir_z * t_top;
        if cx >= min_x - 1e-4 && cx <= max_x + 1e-4 && cz >= min_z - 1e-4 && cz <= max_z + 1e-4 {
            if t_top < best_t {
                let denom = (1.0 + slope * slope).sqrt();
                let normal_x = if along_x { -slope / denom } else { 0.0 };
                let normal_z = if along_x { 0.0 } else { -slope / denom };
                let normal_y = 1.0 / denom;
                best_t = t_top;
                best_normal = (normal_x, normal_y, normal_z);
            }
        }
    }

    if best_t.is_finite() {
        Some((best_normal.0, best_normal.1, best_normal.2, best_t))
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
) -> HitResult {
    let player_center_y = player_pos.y + PLAYER_HEIGHT / 2.0;
    let y_diff = (proj_pos.y - player_center_y).abs();
    if y_diff > PLAYER_HEIGHT / 2.0 + PROJECTILE_RADIUS {
        return no_hit();
    }

    let ray_dir_x = projectile.velocity.x * delta;
    let ray_dir_y = projectile.velocity.y * delta;
    let ray_dir_z = projectile.velocity.z * delta;

    let dx = proj_pos.x - player_pos.x;
    let dz = proj_pos.z - player_pos.z;

    let cos_rot = player_face_dir.cos();
    let sin_rot = player_face_dir.sin();

    let local_x = dx.mul_add(cos_rot, -(dz * sin_rot));
    let local_z = dx.mul_add(sin_rot, dz * cos_rot);
    let local_y = proj_pos.y - player_center_y;

    let ray_local_x = ray_dir_x.mul_add(cos_rot, -(ray_dir_z * sin_rot));
    let ray_local_z = ray_dir_x.mul_add(sin_rot, ray_dir_z * cos_rot);
    let ray_local_y = ray_dir_y;

    let half_width = PLAYER_WIDTH / 2.0 + PROJECTILE_RADIUS;
    let half_height = PLAYER_HEIGHT / 2.0 + PROJECTILE_RADIUS;
    let half_depth = PLAYER_DEPTH / 2.0 + PROJECTILE_RADIUS;

    let mut t_min = 0.0_f32;
    let mut t_max = 1.0_f32;

    if let Some((new_min, new_max)) = sweep_slab_interval(local_x, ray_local_x, half_width, t_min, t_max) {
        t_min = new_min;
        t_max = new_max;
    } else {
        return no_hit();
    }

    if let Some((new_min, new_max)) = sweep_slab_interval(local_y, ray_local_y, half_height, t_min, t_max) {
        t_min = new_min;
        t_max = new_max;
    } else {
        return no_hit();
    }

    if let Some((new_min, new_max)) = sweep_slab_interval(local_z, ray_local_z, half_depth, t_min, t_max) {
        t_min = new_min;
        t_max = new_max;
    } else {
        return no_hit();
    }

    if t_min <= t_max && t_max >= 0.0 && t_min <= 1.0 {
        let vel_len = projectile.velocity.x.hypot(projectile.velocity.z);
        let hit_dir_x = if vel_len > 0.0 {
            projectile.velocity.x / vel_len
        } else {
            0.0
        };
        let hit_dir_z = if vel_len > 0.0 {
            projectile.velocity.z / vel_len
        } else {
            0.0
        };

        HitResult {
            hit: true,
            hit_dir_x,
            hit_dir_z,
        }
    } else {
        no_hit()
    }
}

#[must_use]
pub fn sweep_projectile_vs_wall(
    proj_pos: &Position,
    projectile: &Projectile,
    delta: f32,
    wall: &Wall,
) -> Option<(f32, f32, f32, f32)> {
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

#[must_use]
pub fn sweep_projectile_vs_roof(
    proj_pos: &Position,
    projectile: &Projectile,
    delta: f32,
    roof: &Roof,
) -> Option<(f32, f32, f32, f32)> {
    let min_x = roof.x1.min(roof.x2);
    let max_x = roof.x1.max(roof.x2);
    let min_z = roof.z1.min(roof.z2);
    let max_z = roof.z1.max(roof.z2);

    let center_x = (min_x + max_x) / 2.0;
    let center_z = (min_z + max_z) / 2.0;
    let center_y = ROOF_HEIGHT - roof.thickness / 2.0;

    let half_x = (max_x - min_x) / 2.0 + PROJECTILE_RADIUS;
    let half_z = (max_z - min_z) / 2.0 + PROJECTILE_RADIUS;
    let half_y = roof.thickness / 2.0 + PROJECTILE_RADIUS;

    let ray_dir_x = projectile.velocity.x * delta;
    let ray_dir_y = projectile.velocity.y * delta;
    let ray_dir_z = projectile.velocity.z * delta;

    sweep_point_vs_cuboid(
        proj_pos,
        ray_dir_x,
        ray_dir_y,
        ray_dir_z,
        center_x,
        center_y,
        center_z,
        half_x,
        half_y,
        half_z,
    )
}

// Sample-based ramp hit test for projectiles.
#[must_use]
pub fn projectile_hits_ramp(
    proj_pos: &Position,
    projectile_velocity: &Vec3,
    delta: f32,
    ramp: &Ramp,
) -> bool {
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
            let ramp_height = crate::ramps::calculate_height_at_position(&[*ramp], sample_x, sample_z);

            if (sample_y - ramp_height).abs() < PROJECTILE_RADIUS * 2.0 {
                return true;
            }
        }
    }

    false
}

#[must_use]
pub fn projectile_hits_wall(proj_pos: &Position, projectile: &Projectile, delta: f32, wall: &Wall) -> bool {
    sweep_projectile_vs_wall(proj_pos, projectile, delta, wall).is_some()
}

#[must_use]
pub fn projectile_hits_roof(proj_pos: &Position, projectile: &Projectile, delta: f32, roof: &Roof) -> bool {
    sweep_projectile_vs_roof(proj_pos, projectile, delta, roof).is_some()
}

#[must_use]
pub fn projectile_hits_ghost(
    proj_pos: &Position,
    projectile: &Projectile,
    delta: f32,
    ghost_pos: &Position,
) -> bool {
    let ghost_center_y = GHOST_SIZE / 2.0;
    let y_diff = (proj_pos.y - ghost_center_y).abs();
    if y_diff > GHOST_SIZE / 2.0 + PROJECTILE_RADIUS {
        return false;
    }

    let ray_dir_x = projectile.velocity.x * delta;
    let ray_dir_z = projectile.velocity.z * delta;

    let dx = proj_pos.x - ghost_pos.x;
    let dz = proj_pos.z - ghost_pos.z;

    let collision_radius = PROJECTILE_RADIUS + GHOST_SIZE / 2.0;
    let radius_sq = collision_radius * collision_radius;

    let dist_sq = dx.mul_add(dx, dz * dz);
    if dist_sq <= radius_sq {
        return true;
    }

    let a = ray_dir_x.mul_add(ray_dir_x, ray_dir_z * ray_dir_z);

    if a < 1e-6 {
        return false;
    }

    let b = 2.0 * dx.mul_add(ray_dir_x, dz * ray_dir_z);
    let c = dist_sq - radius_sq;
    let discriminant = b.mul_add(b, -4.0 * a * c);

    if discriminant < 0.0 {
        return false;
    }

    let sqrt_disc = discriminant.sqrt();
    let t1 = (-b - sqrt_disc) / (2.0 * a);
    let t2 = (-b + sqrt_disc) / (2.0 * a);

    (0.0..=1.0).contains(&t1) || (0.0..=1.0).contains(&t2) || (t1 < 0.0 && t2 > 1.0)
}
