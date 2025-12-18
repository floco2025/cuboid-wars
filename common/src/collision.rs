use bevy_ecs::prelude::*;
use bevy_math::Vec3;
use bevy_time::{Timer, TimerMode};

use crate::{
    constants::*,
    protocol::{Position, Ramp, Roof, Wall},
    ramps::calculate_height_at_position,
};

// ============================================================================
// Helper Functions
// ============================================================================

// Result of a projectile hit detection check
#[derive(Debug, Clone, Copy)]
pub struct HitResult {
    pub hit: bool,
    pub hit_dir_x: f32,
    pub hit_dir_z: f32,
}

// Create a HitResult indicating no hit
const fn no_hit() -> HitResult {
    HitResult {
        hit: false,
        hit_dir_x: 0.0,
        hit_dir_z: 0.0,
    }
}

// Generic AABB wall overlap check with parameterized entity dimensions
fn check_aabb_wall_overlap(entity_pos: &Position, wall: &Wall, half_x: f32, half_z: f32) -> bool {
    // Calculate wall dimensions and orientation
    let dx = (wall.x2 - wall.x1).abs();
    let dz = (wall.z2 - wall.z1).abs();
    let wall_half_width = wall.width / 2.0;

    // Determine wall bounding box based on orientation
    // Only expand perpendicular to the wall direction, not along its length
    let (wall_min_x, wall_max_x, wall_min_z, wall_max_z) = if dx > dz {
        // Horizontal wall (runs along X axis) - expand in Z, not X
        (
            wall.x1.min(wall.x2),
            wall.x1.max(wall.x2),
            wall.z1.min(wall.z2) - wall_half_width,
            wall.z1.max(wall.z2) + wall_half_width,
        )
    } else {
        // Vertical wall (runs along Z axis) - expand in X, not Z
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

    ranges_overlap(entity_min_x, entity_max_x, wall_min_x, wall_max_x)
        && ranges_overlap(entity_min_z, entity_max_z, wall_min_z, wall_max_z)
}

// Generic swept AABB wall collision check with parameterized entity dimensions
// NOTE: Assumes axis-aligned walls (horizontal or vertical, not diagonal)
fn check_aabb_wall_sweep(start_pos: &Position, end_pos: &Position, wall: &Wall, half_x: f32, half_z: f32) -> bool {
    // Calculate wall center and half dimensions
    let wall_center_x = f32::midpoint(wall.x1, wall.x2);
    let wall_center_z = f32::midpoint(wall.z1, wall.z2);

    // Calculate wall dimensions from corners
    // For axis-aligned walls, either dx or dz is zero
    let dx = (wall.x2 - wall.x1).abs();
    let dz = (wall.z2 - wall.z1).abs();
    let wall_half_width = wall.width / 2.0;

    // Determine wall orientation and set AABB dimensions
    // Horizontal wall: extends along X axis, thin in Z direction
    // Vertical wall: extends along Z axis, thin in X direction
    let (wall_half_x, wall_half_z) = if dx > dz {
        (dx / 2.0, wall_half_width)
    } else {
        (wall_half_width, dz / 2.0)
    };

    // Movement vector
    let ray_dir_x = end_pos.x - start_pos.x;
    let ray_dir_z = end_pos.z - start_pos.z;

    // Expanded AABB dimensions (entity + wall)
    let combined_half_x = half_x + wall_half_x;
    let combined_half_z = half_z + wall_half_z;

    // Position relative to wall center
    let local_x = start_pos.x - wall_center_x;
    let local_z = start_pos.z - wall_center_z;

    let mut t_min = 0.0_f32;
    let mut t_max = 1.0_f32;

    // Check X axis
    if let Some((min_x, max_x)) = slab_interval(local_x, ray_dir_x, combined_half_x, t_min, t_max) {
        t_min = min_x;
        t_max = max_x;
    } else {
        return false;
    }

    // Check Z axis
    if let Some((min_z, max_z)) = slab_interval(local_z, ray_dir_z, combined_half_z, t_min, t_max) {
        t_min = min_z;
        t_max = max_z;
    } else {
        return false;
    }

    // Collision occurs if intervals overlap within the movement range
    t_min <= t_max && t_max >= 0.0 && t_min <= 1.0
}

// Generic wall sliding calculation with parameterized collision check function
fn calculate_entity_slide<F, R>(
    walls: &[Wall],
    ramps: &[Ramp],
    current_pos: &Position,
    velocity_x: f32,
    velocity_z: f32,
    delta: f32,
    collision_check: F,
    ramp_edge_check: R,
) -> Position
where
    F: Fn(&Position, &Position, &Wall) -> bool,
    R: Fn(&Position, &Position, &Ramp) -> bool,
{
    // Try moving only in X direction
    let x_only_x = velocity_x.mul_add(delta, current_pos.x);
    let x_only_pos = Position {
        x: x_only_x,
        y: calculate_height_at_position(ramps, x_only_x, current_pos.z),
        z: current_pos.z,
    };

    let x_collides = walls.iter().any(|w| collision_check(current_pos, &x_only_pos, w))
        || ramps.iter().any(|r| ramp_edge_check(current_pos, &x_only_pos, r));

    // Try moving only in Z direction
    let z_only_z = velocity_z.mul_add(delta, current_pos.z);
    let z_only_pos = Position {
        x: current_pos.x,
        y: calculate_height_at_position(ramps, current_pos.x, z_only_z),
        z: z_only_z,
    };

    let z_collides = walls.iter().any(|w| collision_check(current_pos, &z_only_pos, w))
        || ramps.iter().any(|r| ramp_edge_check(current_pos, &z_only_pos, r));

    // If neither axis causes collision, use the one with larger movement
    if !x_collides && !z_collides {
        if velocity_x.abs() > velocity_z.abs() {
            x_only_pos
        } else {
            z_only_pos
        }
    } else if !x_collides {
        x_only_pos
    } else if !z_collides {
        z_only_pos
    } else {
        // Both directions blocked, stay in place
        *current_pos
    }
}

// Compute the intersection interval of a ray with a slab (used in ray-AABB tests)
fn slab_interval(local_coord: f32, ray_dir: f32, half_extent: f32, t_min: f32, t_max: f32) -> Option<(f32, f32)> {
    if ray_dir.abs() > 1e-6 {
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

// Check if two 1D ranges overlap.
fn ranges_overlap(a_min: f32, a_max: f32, b_min: f32, b_max: f32) -> bool {
    a_max >= b_min && a_min <= b_max
}

// ============================================================================
// Projectile Component
// ============================================================================

// Component attached to projectile entities to track velocity, lifetime, and bounce behavior
#[derive(Component)]
pub struct Projectile {
    pub velocity: Vec3,
    pub lifetime: Timer,
    pub reflects: bool, // Whether this projectile bounces off walls
}

impl Projectile {
    // Create a new projectile traveling along the provided facing direction
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

    // Handle ramp collision and bounce if reflects is true
    //
    // Returns:
    // - `Some(new_position)` if wall was hit
    // - `None` if no collision
    //
    // If reflects=false and wall hit, caller should despawn the projectile
    #[must_use]
    pub fn handle_ramp_bounce(&mut self, projectile_pos: &Position, delta: f32, ramp: &Ramp) -> Option<Position> {
        if let Some((normal_x, normal_y, normal_z, t_collision)) =
            check_projectile_ramp_sweep_hit(projectile_pos, self, delta, ramp)
        {
            // Move projectile to collision point
            let collision_x = self.velocity.x.mul_add(delta * t_collision, projectile_pos.x);
            let collision_y = self.velocity.y.mul_add(delta * t_collision, projectile_pos.y);
            let collision_z = self.velocity.z.mul_add(delta * t_collision, projectile_pos.z);

            if self.reflects {
                // Reflect velocity off the hit normal
                let dot = self
                    .velocity
                    .x
                    .mul_add(normal_x, self.velocity.y.mul_add(normal_y, self.velocity.z * normal_z));
                self.velocity.x -= 2.0 * dot * normal_x;
                self.velocity.y -= 2.0 * dot * normal_y;
                self.velocity.z -= 2.0 * dot * normal_z;

                // Separate slightly to avoid re-penetration
                const SEPARATION_EPSILON: f32 = 0.01;
                let separated_x = normal_x.mul_add(SEPARATION_EPSILON, collision_x);
                let separated_y = normal_y.mul_add(SEPARATION_EPSILON, collision_y);
                let separated_z = normal_z.mul_add(SEPARATION_EPSILON, collision_z);

                // Continue moving for remaining time after bounce
                let remaining_time = delta * (1.0 - t_collision);
                Some(Position {
                    x: self.velocity.x.mul_add(remaining_time, separated_x),
                    y: self.velocity.y.mul_add(remaining_time, separated_y),
                    z: self.velocity.z.mul_add(remaining_time, separated_z),
                })
            } else {
                // Hit ramp without reflect - return current position, caller should despawn
                Some(*projectile_pos)
            }
        } else {
            None
        }
    }

    #[must_use]
    pub fn handle_wall_bounce(&mut self, projectile_pos: &Position, delta: f32, wall: &Wall) -> Option<Position> {
        if let Some((normal_x, normal_y, normal_z, t_collision)) =
            check_projectile_wall_sweep_hit(projectile_pos, self, delta, wall)
        {
            if self.reflects {
                // Move projectile to collision point
                let collision_x = self.velocity.x.mul_add(delta * t_collision, projectile_pos.x);
                let collision_y = self.velocity.y.mul_add(delta * t_collision, projectile_pos.y);
                let collision_z = self.velocity.z.mul_add(delta * t_collision, projectile_pos.z);

                // Reflect velocity off the wall normal
                let dot = self.velocity.x.mul_add(normal_x,
                    self.velocity.y.mul_add(normal_y, self.velocity.z * normal_z));
                self.velocity.x -= 2.0 * dot * normal_x;
                self.velocity.y -= 2.0 * dot * normal_y;
                self.velocity.z -= 2.0 * dot * normal_z;

                // Push projectile slightly away from wall surface to prevent getting stuck inside
                const SEPARATION_EPSILON: f32 = 0.01;
                let separated_x = normal_x.mul_add(SEPARATION_EPSILON, collision_x);
                let separated_y = normal_y.mul_add(SEPARATION_EPSILON, collision_y);
                let separated_z = normal_z.mul_add(SEPARATION_EPSILON, collision_z);

                // Continue moving for remaining time after bounce
                let remaining_time = delta * (1.0 - t_collision);
                Some(Position {
                    x: self.velocity.x.mul_add(remaining_time, separated_x),
                    y: self.velocity.y.mul_add(remaining_time, separated_y),
                    z: self.velocity.z.mul_add(remaining_time, separated_z),
                })
            } else {
                // Hit wall without reflect - return current position, caller should despawn
                Some(*projectile_pos)
            }
        } else {
            None
        }
    }

    #[must_use]
    pub fn handle_roof_bounce(&mut self, projectile_pos: &Position, delta: f32, roof: &Roof) -> Option<Position> {
        if let Some((normal_x, normal_y, normal_z, t_collision)) =
            check_projectile_roof_sweep_hit(projectile_pos, self, delta, roof)
        {
            if self.reflects {
                let collision_x = self.velocity.x.mul_add(delta * t_collision, projectile_pos.x);
                let collision_y = self.velocity.y.mul_add(delta * t_collision, projectile_pos.y);
                let collision_z = self.velocity.z.mul_add(delta * t_collision, projectile_pos.z);

                let dot = self.velocity.x.mul_add(normal_x,
                    self.velocity.y.mul_add(normal_y, self.velocity.z * normal_z));
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

        // Only consider if moving downward toward the ground.
        if vy >= 0.0 {
            return None;
        }

        // Solve for time when bottom of sphere reaches y=0.
        let t_hit = (PROJECTILE_RADIUS - projectile_pos.y) / (vy * delta);

        if t_hit < 0.0 || t_hit > 1.0 {
            return None;
        }

        // Move to collision point.
        let collision_x = self.velocity.x.mul_add(delta * t_hit, projectile_pos.x);
        let collision_y = self.velocity.y.mul_add(delta * t_hit, projectile_pos.y);
        let collision_z = self.velocity.z.mul_add(delta * t_hit, projectile_pos.z);

        let normal_x = 0.0;
        let normal_y = 1.0;
        let normal_z = 0.0;

        if self.reflects {
            // Reflect velocity on ground plane.
            let dot = self.velocity.x.mul_add(normal_x,
                self.velocity.y.mul_add(normal_y, self.velocity.z * normal_z));
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
            // No reflect: treat as hit and let caller despawn
            Some(*projectile_pos)
        }
    }
}

// ============================================================================
// Collision Detection - Projectiles
// ============================================================================

// Check if a projectile hits a ramp using swept sphere collision (treating ramp as box)
//
// Returns:
// - `Some((normal_x, normal_y, normal_z, t_collision))` if hit
// - `None` if no collision
#[must_use]
pub fn check_projectile_ramp_sweep_hit(
    proj_pos: &Position,
    projectile: &Projectile,
    delta: f32,
    ramp: &Ramp,
) -> Option<(f32, f32, f32, f32)> {
    // Geometry setup
    let min_x = ramp.x1.min(ramp.x2);
    let max_x = ramp.x1.max(ramp.x2);
    let min_z = ramp.z1.min(ramp.z2);
    let max_z = ramp.z1.max(ramp.z2);
    let min_y = ramp.y1.min(ramp.y2);
    let max_y = ramp.y1.max(ramp.y2);

    let ray_dir_x = projectile.velocity.x * delta;
    let ray_dir_y = projectile.velocity.y * delta;
    let ray_dir_z = projectile.velocity.z * delta;

    // Early out: AABB broad-phase with radius padding
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

    // Helper: height along dominant axis
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

    // 2D slab on footprint expanded by radius to find when we touch vertical sides
    let exp_min_x = min_x - PROJECTILE_RADIUS;
    let exp_max_x = max_x + PROJECTILE_RADIUS;
    let exp_min_z = min_z - PROJECTILE_RADIUS;
    let exp_max_z = max_z + PROJECTILE_RADIUS;

    let mut t_enter = 0.0_f32;
    let mut t_exit = 1.0_f32;
    let mut hit_normal_x = 0.0_f32;
    let mut hit_normal_z = 0.0_f32;

    // X slab
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

    // Z slab
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

    // Candidate: side hit at t_enter (footprint boundary)
    let mut best_t = f32::INFINITY;
    let mut best_normal = (0.0_f32, 0.0_f32, 0.0_f32);

    let test_side = |t: f32, nx: f32, nz: f32, height_at: &dyn Fn(f32, f32) -> f32| -> Option<(f32, f32, f32, f32)> {
        if t < 0.0 || t > 1.0 {
            return None;
        }
        let cx = proj_pos.x + ray_dir_x * t;
        let cz = proj_pos.z + ray_dir_z * t;
        let cy = proj_pos.y + ray_dir_y * t;

        // Clamp to actual footprint for height evaluation
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

    // Candidate: top face hit (solve analytically)
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
        if t_top >= 0.0 && t_top <= 1.0 {
            top_hit = Some(t_top);
        }
    }

    if let Some(t_top) = top_hit {
        let cx = proj_pos.x + ray_dir_x * t_top;
        let cz = proj_pos.z + ray_dir_z * t_top;
        if cx >= min_x - 1e-4 && cx <= max_x + 1e-4 && cz >= min_z - 1e-4 && cz <= max_z + 1e-4 {
            if t_top < best_t {
                // Slope normal from plane y = slope*(axis) + b
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

// Check if a projectile hits a player using swept sphere collision
//
// Returns HitResult with hit flag and normalized direction
#[must_use]
pub fn check_projectile_player_sweep_hit(
    proj_pos: &Position,
    projectile: &Projectile,
    delta: f32,
    player_pos: &Position,
    player_face_dir: f32,
) -> HitResult {
    // Height check: projectile and player must be at similar heights
    // Player center is at player_pos.y + PLAYER_HEIGHT/2
    let player_center_y = player_pos.y + PLAYER_HEIGHT / 2.0;
    let y_diff = (proj_pos.y - player_center_y).abs();
    // Allow some tolerance for height matching (half player height)
    if y_diff > PLAYER_HEIGHT / 2.0 + PROJECTILE_RADIUS {
        return no_hit();
    }

    // Calculate projectile movement this frame
    let ray_dir_x = projectile.velocity.x * delta;
    let ray_dir_y = projectile.velocity.y * delta;
    let ray_dir_z = projectile.velocity.z * delta;

    // Transform projectile position and ray into player's local space
    let dx = proj_pos.x - player_pos.x;
    let dz = proj_pos.z - player_pos.z;

    let cos_rot = player_face_dir.cos();
    let sin_rot = player_face_dir.sin();

    // Current position in local space
    let local_x = dx.mul_add(cos_rot, -(dz * sin_rot));
    let local_z = dx.mul_add(sin_rot, dz * cos_rot);
    let local_y = proj_pos.y - player_center_y;

    // Ray direction in local space
    let ray_local_x = ray_dir_x.mul_add(cos_rot, -(ray_dir_z * sin_rot));
    let ray_local_z = ray_dir_x.mul_add(sin_rot, ray_dir_z * cos_rot);
    let ray_local_y = ray_dir_y;

    // Expanded box for swept sphere collision
    let half_width = PLAYER_WIDTH / 2.0 + PROJECTILE_RADIUS;
    let half_height = PLAYER_HEIGHT / 2.0 + PROJECTILE_RADIUS;
    let half_depth = PLAYER_DEPTH / 2.0 + PROJECTILE_RADIUS;

    // Ray-box intersection using slab method
    let mut t_min = 0.0_f32;
    let mut t_max = 1.0_f32;

    // Check X slab
    if let Some((new_min, new_max)) = slab_interval(local_x, ray_local_x, half_width, t_min, t_max) {
        t_min = new_min;
        t_max = new_max;
    } else {
        return no_hit();
    }

    // Check Y slab
    if let Some((new_min, new_max)) = slab_interval(local_y, ray_local_y, half_height, t_min, t_max) {
        t_min = new_min;
        t_max = new_max;
    } else {
        return no_hit();
    }

    // Check Z slab
    if let Some((new_min, new_max)) = slab_interval(local_z, ray_local_z, half_depth, t_min, t_max) {
        t_min = new_min;
        t_max = new_max;
    } else {
        return no_hit();
    }

    // Hit if intervals overlap
    if t_min <= t_max && t_max >= 0.0 && t_min <= 1.0 {
        // Normalize the projectile velocity to get hit direction
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

// Generic swept sphere vs axis-aligned cuboid.
#[must_use]
pub fn check_projectile_cuboid_sweep_hit(
    proj_pos: &Position,
    projectile: &Projectile,
    delta: f32,
    center_x: f32,
    center_y: f32,
    center_z: f32,
    half_x: f32,
    half_y: f32,
    half_z: f32,
) -> Option<(f32, f32, f32, f32)> {
    let ray_dir_x = projectile.velocity.x * delta;
    let ray_dir_y = projectile.velocity.y * delta;
    let ray_dir_z = projectile.velocity.z * delta;

    let local_x = proj_pos.x - center_x;
    let local_y = proj_pos.y - center_y;
    let local_z = proj_pos.z - center_z;

    let mut t_enter = 0.0_f32;
    let mut t_exit = 1.0_f32;
    let mut hit_normal = (0.0_f32, 0.0_f32, 0.0_f32);

    // X slab
    if ray_dir_x.abs() < 1e-6 {
        if local_x.abs() > half_x {
            return None;
        }
    } else {
        let tx1 = (-half_x - local_x) / ray_dir_x;
        let tx2 = (half_x - local_x) / ray_dir_x;
        let (tx_min, tx_max) = if tx1 < tx2 { (tx1, tx2) } else { (tx2, tx1) };
        if tx_min > t_enter {
            t_enter = tx_min;
            hit_normal = (if ray_dir_x > 0.0 { -1.0 } else { 1.0 }, 0.0, 0.0);
        }
        t_exit = t_exit.min(tx_max);
        if t_enter > t_exit {
            return None;
        }
    }

    // Y slab
    if ray_dir_y.abs() < 1e-6 {
        if local_y.abs() > half_y {
            return None;
        }
    } else {
        let ty1 = (-half_y - local_y) / ray_dir_y;
        let ty2 = (half_y - local_y) / ray_dir_y;
        let (ty_min, ty_max) = if ty1 < ty2 { (ty1, ty2) } else { (ty2, ty1) };
        if ty_min > t_enter {
            t_enter = ty_min;
            hit_normal = (0.0, if ray_dir_y > 0.0 { -1.0 } else { 1.0 }, 0.0);
        }
        t_exit = t_exit.min(ty_max);
        if t_enter > t_exit {
            return None;
        }
    }

    // Z slab
    if ray_dir_z.abs() < 1e-6 {
        if local_z.abs() > half_z {
            return None;
        }
    } else {
        let tz1 = (-half_z - local_z) / ray_dir_z;
        let tz2 = (half_z - local_z) / ray_dir_z;
        let (tz_min, tz_max) = if tz1 < tz2 { (tz1, tz2) } else { (tz2, tz1) };
        if tz_min > t_enter {
            t_enter = tz_min;
            hit_normal = (0.0, 0.0, if ray_dir_z > 0.0 { -1.0 } else { 1.0 });
        }
        t_exit = t_exit.min(tz_max);
        if t_enter > t_exit {
            return None;
        }
    }

    if t_exit < 0.0 || t_enter > 1.0 {
        return None;
    }

    if hit_normal.0 == 0.0 && hit_normal.1 == 0.0 && hit_normal.2 == 0.0 {
        return None;
    }

    let t_collision = t_enter.clamp(0.0, 1.0);
    Some((hit_normal.0, hit_normal.1, hit_normal.2, t_collision))
}

// Walls: thin cuboid aligned to the grid.
#[must_use]
pub fn check_projectile_wall_sweep_hit(
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

    check_projectile_cuboid_sweep_hit(
        proj_pos,
        projectile,
        delta,
        wall_center_x,
        wall_center_y,
        wall_center_z,
        half_x,
        half_y,
        half_z,
    )
}

// Roofs: flat cuboid at roof height.
#[must_use]
pub fn check_projectile_roof_sweep_hit(
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

    check_projectile_cuboid_sweep_hit(
        proj_pos,
        projectile,
        delta,
        center_x,
        center_y,
        center_z,
        half_x,
        half_y,
        half_z,
    )
}

// Check if a projectile hits a wall (simplified version without normal/time)
#[must_use]
pub fn check_projectile_wall_sweep(proj_pos: &Position, projectile: &Projectile, delta: f32, wall: &Wall) -> bool {
    check_projectile_wall_sweep_hit(proj_pos, projectile, delta, wall).is_some()
}

// Check if a projectile hits a roof (simplified version without normal/time)
#[must_use]
pub fn check_projectile_roof_sweep(proj_pos: &Position, projectile: &Projectile, delta: f32, roof: &Roof) -> bool {
    check_projectile_roof_sweep_hit(proj_pos, projectile, delta, roof).is_some()
}

// ============================================================================
// Collision Detection - Players and Walls
// ============================================================================

// Check if a player position intersects with a wall (AABB collision)
#[must_use]
pub fn check_player_wall_overlap(player_pos: &Position, wall: &Wall) -> bool {
    check_aabb_wall_overlap(player_pos, wall, PLAYER_WIDTH / 2.0, PLAYER_DEPTH / 2.0)
}

// Check if a player moving from start_pos to end_pos would collide with a wall
// Uses swept AABB collision to prevent tunneling through walls
//
// Returns true if collision occurs during the movement
#[must_use]
pub fn check_player_wall_sweep(start_pos: &Position, end_pos: &Position, wall: &Wall) -> bool {
    check_aabb_wall_sweep(start_pos, end_pos, wall, PLAYER_WIDTH / 2.0, PLAYER_DEPTH / 2.0)
}

// Calculate sliding movement when a collision occurs
//
// Returns the new position that slides along the surface
#[must_use]
pub fn calculate_wall_slide(
    walls: &[Wall],
    ramps: &[Ramp],
    current_pos: &Position,
    velocity_x: f32,
    velocity_z: f32,
    delta: f32,
) -> Position {
    calculate_entity_slide(
        walls,
        ramps,
        current_pos,
        velocity_x,
        velocity_z,
        delta,
        check_player_wall_sweep,
        check_player_ramp_edge_sweep,
    )
}

// Calculate sliding movement when a collision occurs for ghosts
//
// Returns the new position that slides along the surface
#[must_use]
pub fn calculate_ghost_slide(
    walls: &[Wall],
    ramps: &[Ramp],
    current_pos: &Position,
    velocity_x: f32,
    velocity_z: f32,
    delta: f32,
) -> Position {
    calculate_entity_slide(
        walls,
        ramps,
        current_pos,
        velocity_x,
        velocity_z,
        delta,
        check_ghost_wall_sweep,
        check_ghost_ramp_edge_sweep,
    )
}

// Swept player-player collision (prevents tunneling during a frame).
#[must_use]
pub fn check_player_player_sweep(start1: &Position, end1: &Position, start2: &Position, end2: &Position) -> bool {
    // Height gate: if both endpoints are well separated vertically, skip.
    let y_diff_start = (start1.y - start2.y).abs();
    let y_diff_end = (end1.y - end2.y).abs();
    if y_diff_start >= PLAYER_HEIGHT && y_diff_end >= PLAYER_HEIGHT {
        return false;
    }

    let half_x = PLAYER_WIDTH / 2.0 + PLAYER_WIDTH / 2.0;
    let half_z = PLAYER_DEPTH / 2.0 + PLAYER_DEPTH / 2.0;

    // Relative motion: treat player2 as static, player1 moves by (d1 - d2)
    let rel_start_x = start1.x - start2.x;
    let rel_start_z = start1.z - start2.z;
    let rel_dir_x = (end1.x - start1.x) - (end2.x - start2.x);
    let rel_dir_z = (end1.z - start1.z) - (end2.z - start2.z);

    let mut t_min = 0.0_f32;
    let mut t_max = 1.0_f32;

    // X slab
    if let Some((new_min, new_max)) = slab_interval(rel_start_x, rel_dir_x, half_x, t_min, t_max) {
        t_min = new_min;
        t_max = new_max;
    } else {
        return false;
    }

    // Z slab
    if let Some((new_min, new_max)) = slab_interval(rel_start_z, rel_dir_z, half_z, t_min, t_max) {
        t_min = new_min;
        t_max = new_max;
    } else {
        return false;
    }

    t_min <= t_max && t_max >= 0.0 && t_min <= 1.0
}

// ============================================================================
// Collision Detection - Ghosts and Items
// ============================================================================

// Check if a ghost position intersects with a wall (AABB collision)
#[must_use]
pub fn check_ghost_wall_overlap(ghost_pos: &Position, wall: &Wall) -> bool {
    let ghost_half_size = GHOST_SIZE / 2.0;
    check_aabb_wall_overlap(ghost_pos, wall, ghost_half_size, ghost_half_size)
}

// Swept ghost vs wall to prevent tunneling.
#[must_use]
pub fn check_ghost_wall_sweep(start_pos: &Position, end_pos: &Position, wall: &Wall) -> bool {
    let ghost_half_size = GHOST_SIZE / 2.0;
    check_aabb_wall_sweep(start_pos, end_pos, wall, ghost_half_size, ghost_half_size)
}

// Check if a ghost and player are overlapping (circle collision)
#[must_use]
pub fn check_ghost_player_overlap(ghost_pos: &Position, player_pos: &Position) -> bool {
    // Height check: ghost and player must be at similar heights
    let player_center_y = player_pos.y + PLAYER_HEIGHT / 2.0;
    let ghost_center_y = GHOST_SIZE / 2.0; // Ghost cube center
    let y_diff = (player_center_y - ghost_center_y).abs();
    if y_diff > (PLAYER_HEIGHT + GHOST_SIZE) / 2.0 {
        return false;
    }

    // AABB vs AABB in XZ
    let player_half_x = PLAYER_WIDTH / 2.0;
    let player_half_z = PLAYER_DEPTH / 2.0;
    let ghost_half = GHOST_SIZE / 2.0;

    let p_min_x = player_pos.x - player_half_x;
    let p_max_x = player_pos.x + player_half_x;
    let p_min_z = player_pos.z - player_half_z;
    let p_max_z = player_pos.z + player_half_z;

    let g_min_x = ghost_pos.x - ghost_half;
    let g_max_x = ghost_pos.x + ghost_half;
    let g_min_z = ghost_pos.z - ghost_half;
    let g_max_z = ghost_pos.z + ghost_half;

    ranges_overlap(p_min_x, p_max_x, g_min_x, g_max_x) && ranges_overlap(p_min_z, p_max_z, g_min_z, g_max_z)
}

// Check if a projectile hits a ghost using swept sphere collision
#[must_use]
pub fn check_projectile_ghost_sweep_hit(
    proj_pos: &Position,
    projectile: &Projectile,
    delta: f32,
    ghost_pos: &Position,
) -> bool {
    // Height check: projectile and ghost must be at similar heights
    // Ghost is always at ground level (y=0), center at GHOST_SIZE/2
    let ghost_center_y = 0.0 + GHOST_SIZE / 2.0;
    let y_diff = (proj_pos.y - ghost_center_y).abs();
    // Allow collision if projectile is within ghost's vertical bounds
    if y_diff > GHOST_SIZE / 2.0 + PROJECTILE_RADIUS {
        return false;
    }

    // Calculate projectile movement this frame
    let ray_dir_x = projectile.velocity.x * delta;
    let ray_dir_z = projectile.velocity.z * delta;

    // Current position relative to ghost
    let dx = proj_pos.x - ghost_pos.x;
    let dz = proj_pos.z - ghost_pos.z;

    // Combined collision radius (projectile + ghost)
    let collision_radius = PROJECTILE_RADIUS + GHOST_SIZE / 2.0;
    let radius_sq = collision_radius * collision_radius;

    // Check if already overlapping
    let dist_sq = dx.mul_add(dx, dz * dz);
    if dist_sq <= radius_sq {
        return true;
    }

    // Ray-sphere intersection for swept collision
    // Solving: |start + t*dir - center|^2 = radius^2
    let a = ray_dir_x.mul_add(ray_dir_x, ray_dir_z * ray_dir_z);

    if a < 1e-6 {
        return false; // No movement
    }

    let b = 2.0 * dx.mul_add(ray_dir_x, dz * ray_dir_z);
    let c = dist_sq - radius_sq;
    let discriminant = b.mul_add(b, -4.0 * a * c);

    if discriminant < 0.0 {
        return false; // No intersection
    }

    let sqrt_disc = discriminant.sqrt();
    let t1 = (-b - sqrt_disc) / (2.0 * a);
    let t2 = (-b + sqrt_disc) / (2.0 * a);

    // Hit if any intersection point is within [0, 1]
    (0.0..=1.0).contains(&t1) || (0.0..=1.0).contains(&t2) || (t1 < 0.0 && t2 > 1.0)
}

// Check if a player is close enough to an item to collect it (circle collision)
#[must_use]
pub fn check_player_item_overlap(player_pos: &Position, item_pos: &Position, collection_radius: f32) -> bool {
    let dx = player_pos.x - item_pos.x;
    let dz = player_pos.z - item_pos.z;
    let dist_sq = dx.mul_add(dx, dz * dz);
    dist_sq <= collection_radius * collection_radius
}

// ============================================================================
// Collision Detection - Ramps
// ============================================================================

// Check if a swept AABB collides with ramp edges (side guards) without using wall helpers.
#[must_use]
pub fn check_aabb_ramp_edge_sweep(
    start_pos: &Position,
    end_pos: &Position,
    ramp: &Ramp,
    half_x: f32,
    half_z: f32,
) -> bool {
    // Ramp footprint bounds
    let min_x = ramp.x1.min(ramp.x2);
    let max_x = ramp.x1.max(ramp.x2);
    let min_z = ramp.z1.min(ramp.z2);
    let max_z = ramp.z1.max(ramp.z2);

    // Choose which edges to block: side edges perpendicular to ramp direction
    let dx = (ramp.x2 - ramp.x1).abs();
    let dz = (ramp.z2 - ramp.z1).abs();
    let block_sides_along_z = dx >= dz; // X-ramp blocks Z edges; Z-ramp blocks X edges

    // Helper: swept AABB vs thin edge box in XZ
    let sweep_edge = |center_x: f32, center_z: f32, half_x_edge: f32, half_z_edge: f32| -> bool {
        let dir_x = end_pos.x - start_pos.x;
        let dir_z = end_pos.z - start_pos.z;

        let local_x = start_pos.x - center_x;
        let local_z = start_pos.z - center_z;

        let mut t_min = 0.0_f32;
        let mut t_max = 1.0_f32;

        if let Some((new_min, new_max)) = slab_interval(local_x, dir_x, half_x + half_x_edge, t_min, t_max) {
            t_min = new_min;
            t_max = new_max;
        } else {
            return false;
        }

        if let Some((new_min, new_max)) = slab_interval(local_z, dir_z, half_z + half_z_edge, t_min, t_max) {
            t_min = new_min;
            t_max = new_max;
        } else {
            return false;
        }

        t_min <= t_max && t_max >= 0.0 && t_min <= 1.0
    };

    let edge_half = RAMP_EDGE_WIDTH / 2.0;

    if block_sides_along_z {
        // Block Z edges at min_z and max_z (span X)
        let center_x = (min_x + max_x) / 2.0;
        let half_x_edge = (max_x - min_x) / 2.0;
        sweep_edge(center_x, min_z, half_x_edge, edge_half) || sweep_edge(center_x, max_z, half_x_edge, edge_half)
    } else {
        // Block X edges at min_x and max_x (span Z)
        let center_z = (min_z + max_z) / 2.0;
        let half_z_edge = (max_z - min_z) / 2.0;
        sweep_edge(min_x, center_z, edge_half, half_z_edge) || sweep_edge(max_x, center_z, edge_half, half_z_edge)
    }
}

// Check if a projectile hits a ramp
// Returns true if the projectile at its height intersects with the ramp surface
#[must_use]
pub fn check_projectile_ramp_hit(
    proj_pos: &Position,
    projectile: &Projectile,
    delta: f32,
    ramp: &Ramp,
) -> bool {
    use crate::ramps::calculate_height_at_position;
    
    // Sample multiple points along the projectile's path
    let num_samples = 5;
    for i in 0..=num_samples {
        let t = i as f32 / num_samples as f32;
        let sample_x = proj_pos.x + projectile.velocity.x * delta * t;
        let sample_y = proj_pos.y + projectile.velocity.y * delta * t;
        let sample_z = proj_pos.z + projectile.velocity.z * delta * t;
        
        // Check if this sample point is within the ramp footprint
        let min_x = ramp.x1.min(ramp.x2);
        let max_x = ramp.x1.max(ramp.x2);
        let min_z = ramp.z1.min(ramp.z2);
        let max_z = ramp.z1.max(ramp.z2);
        
        if sample_x >= min_x && sample_x <= max_x && sample_z >= min_z && sample_z <= max_z {
            // Get the ramp height at this XZ position
            let ramp_height = calculate_height_at_position(&[*ramp], sample_x, sample_z);
            
            // Check if projectile is close to the ramp surface (within radius)
            if (sample_y - ramp_height).abs() < PROJECTILE_RADIUS * 2.0 {
                return true;
            }
        }
    }
    
    false
}

// Check if a player moving from start to end hits a ramp edge
#[must_use]
pub fn check_player_ramp_edge_sweep(start_pos: &Position, end_pos: &Position, ramp: &Ramp) -> bool {
    check_aabb_ramp_edge_sweep(start_pos, end_pos, ramp, PLAYER_WIDTH / 2.0, PLAYER_DEPTH / 2.0)
}

// Check if a ghost moving hits a ramp edge
#[must_use]
pub fn check_ghost_ramp_edge_sweep(start_pos: &Position, end_pos: &Position, ramp: &Ramp) -> bool {
    let ghost_half_size = GHOST_SIZE / 2.0;
    check_aabb_ramp_edge_sweep(start_pos, end_pos, ramp, ghost_half_size, ghost_half_size)
}
