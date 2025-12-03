use bevy_ecs::prelude::*;
use bevy_math::Vec3;
use bevy_time::{Timer, TimerMode};

use crate::{
    constants::*,
    protocol::{Position, Wall, WallOrientation},
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
    let (wall_half_x, wall_half_z) = match wall.orientation {
        WallOrientation::Horizontal => (WALL_LENGTH / 2.0, WALL_WIDTH / 2.0),
        WallOrientation::Vertical => (WALL_WIDTH / 2.0, WALL_LENGTH / 2.0),
    };

    let entity_min_x = entity_pos.x - half_x;
    let entity_max_x = entity_pos.x + half_x;
    let entity_min_z = entity_pos.z - half_z;
    let entity_max_z = entity_pos.z + half_z;

    let wall_min_x = wall.x - wall_half_x;
    let wall_max_x = wall.x + wall_half_x;
    let wall_min_z = wall.z - wall_half_z;
    let wall_max_z = wall.z + wall_half_z;

    ranges_overlap(entity_min_x, entity_max_x, wall_min_x, wall_max_x)
        && ranges_overlap(entity_min_z, entity_max_z, wall_min_z, wall_max_z)
}

// Generic swept AABB wall collision check with parameterized entity dimensions
fn check_aabb_wall_sweep(start_pos: &Position, end_pos: &Position, wall: &Wall, half_x: f32, half_z: f32) -> bool {
    let (wall_half_x, wall_half_z) = match wall.orientation {
        WallOrientation::Horizontal => (WALL_LENGTH / 2.0, WALL_WIDTH / 2.0),
        WallOrientation::Vertical => (WALL_WIDTH / 2.0, WALL_LENGTH / 2.0),
    };

    // Movement vector
    let ray_dir_x = end_pos.x - start_pos.x;
    let ray_dir_z = end_pos.z - start_pos.z;

    // Expanded AABB dimensions (entity + wall)
    let combined_half_x = half_x + wall_half_x;
    let combined_half_z = half_z + wall_half_z;

    // Position relative to wall center
    let local_x = start_pos.x - wall.x;
    let local_z = start_pos.z - wall.z;

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
fn calculate_entity_wall_slide<F>(
    walls: &[Wall],
    current_pos: &Position,
    velocity_x: f32,
    velocity_z: f32,
    delta: f32,
    collision_check: F,
) -> Position
where
    F: Fn(&Position, &Wall) -> bool,
{
    // Try moving only in X direction
    let x_only_pos = Position {
        x: velocity_x.mul_add(delta, current_pos.x),
        y: current_pos.y,
        z: current_pos.z,
    };

    let x_collides = walls.iter().any(|w| collision_check(&x_only_pos, w));

    // Try moving only in Z direction
    let z_only_pos = Position {
        x: current_pos.x,
        y: current_pos.y,
        z: velocity_z.mul_add(delta, current_pos.z),
    };

    let z_collides = walls.iter().any(|w| collision_check(&z_only_pos, w));

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

    // Calculate the spawn position in front of a shooter
    #[must_use]
    pub fn calculate_spawn_position(shooter_pos: Vec3, face_dir: f32) -> Vec3 {
        Vec3::new(
            face_dir.sin().mul_add(PROJECTILE_SPAWN_OFFSET, shooter_pos.x),
            PROJECTILE_SPAWN_HEIGHT,
            face_dir.cos().mul_add(PROJECTILE_SPAWN_OFFSET, shooter_pos.z),
        )
    }

    // Handle wall collision and bounce if reflects is true
    //
    // Returns:
    // - `Some(new_position)` if wall was hit
    // - `None` if no collision
    //
    // If reflects=false and wall hit, caller should despawn the projectile
    #[must_use]
    pub fn handle_wall_bounce(&mut self, projectile_pos: &Position, delta: f32, wall: &Wall) -> Option<Position> {
        if let Some((normal_x, normal_z, t_collision)) =
            check_projectile_wall_sweep_hit(projectile_pos, self, delta, wall)
        {
            if self.reflects {
                // Move projectile to collision point
                let collision_x = self.velocity.x.mul_add(delta * t_collision, projectile_pos.x);
                let collision_y = self.velocity.y.mul_add(delta * t_collision, projectile_pos.y);
                let collision_z = self.velocity.z.mul_add(delta * t_collision, projectile_pos.z);

                // Reflect velocity off the wall normal
                let dot = self.velocity.x.mul_add(normal_x, self.velocity.z * normal_z);
                self.velocity.x -= 2.0 * dot * normal_x;
                self.velocity.z -= 2.0 * dot * normal_z;

                // Continue moving for remaining time after bounce
                let remaining_time = delta * (1.0 - t_collision);
                Some(Position {
                    x: self.velocity.x.mul_add(remaining_time, collision_x),
                    y: self.velocity.y.mul_add(remaining_time, collision_y),
                    z: self.velocity.z.mul_add(remaining_time, collision_z),
                })
            } else {
                // Hit wall without reflect - return current position, caller should despawn
                Some(*projectile_pos)
            }
        } else {
            None
        }
    }
}

// ============================================================================
// Collision Detection - Projectiles
// ============================================================================

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
    let local_y = proj_pos.y - (player_pos.y + PLAYER_HEIGHT / 2.0);

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

// Check if a projectile hits a wall using swept sphere collision
//
// Returns:
// - `Some((normal_x, normal_z, t_collision))` if hit
// - `None` if no collision
//
// `t_collision` is between 0.0 and 1.0, representing how far along the movement the collision
// occurs
#[must_use]
pub fn check_projectile_wall_sweep_hit(
    proj_pos: &Position,
    projectile: &Projectile,
    delta: f32,
    wall: &Wall,
) -> Option<(f32, f32, f32)> {
    // Calculate projectile movement this frame
    let ray_start_x = proj_pos.x;
    let ray_start_y = proj_pos.y;
    let ray_start_z = proj_pos.z;

    let ray_dir_x = projectile.velocity.x * delta;
    let ray_dir_y = projectile.velocity.y * delta;
    let ray_dir_z = projectile.velocity.z * delta;

    // Wall dimensions
    let half_height = WALL_HEIGHT / 2.0 + PROJECTILE_RADIUS;
    let half_thickness = WALL_WIDTH / 2.0 + PROJECTILE_RADIUS;
    let half_length = WALL_LENGTH / 2.0 + PROJECTILE_RADIUS;

    let (half_x, half_z) = match wall.orientation {
        WallOrientation::Horizontal => (half_length, half_thickness),
        WallOrientation::Vertical => (half_thickness, half_length),
    };

    let local_x = ray_start_x - wall.x;
    let local_y = ray_start_y - WALL_HEIGHT / 2.0;
    let local_z = ray_start_z - wall.z;

    let mut t_min = 0.0_f32;
    let mut t_max = 1.0_f32;

    if let Some((min_x, max_x)) = slab_interval(local_x, ray_dir_x, half_x, t_min, t_max) {
        t_min = min_x;
        t_max = max_x;
    } else {
        return None;
    }

    if let Some((min_y, max_y)) = slab_interval(local_y, ray_dir_y, half_height, t_min, t_max) {
        t_min = min_y;
        t_max = max_y;
    } else {
        return None;
    }

    if let Some((min_z, max_z)) = slab_interval(local_z, ray_dir_z, half_z, t_min, t_max) {
        t_min = min_z;
        t_max = max_z;
    } else {
        return None;
    }

    if t_min <= t_max && t_max >= 0.0 && t_min <= 1.0 {
        // Return the normal based on wall orientation
        let (normal_x, normal_z) = match wall.orientation {
            WallOrientation::Horizontal => {
                // Determine which side based on position relative to wall
                if local_z > 0.0 { (0.0, 1.0) } else { (0.0, -1.0) }
            }
            WallOrientation::Vertical => {
                if local_x > 0.0 {
                    (1.0, 0.0)
                } else {
                    (-1.0, 0.0)
                }
            }
        };
        // Clamp t_min to [0.0, 1.0] for the collision time
        let t_collision = t_min.max(0.0).min(1.0);
        Some((normal_x, normal_z, t_collision))
    } else {
        None
    }
}

// Check if a projectile hits a wall (simplified version without normal/time)
#[must_use]
pub fn check_projectile_wall_sweep(proj_pos: &Position, projectile: &Projectile, delta: f32, wall: &Wall) -> bool {
    check_projectile_wall_sweep_hit(proj_pos, projectile, delta, wall).is_some()
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

// Calculate sliding movement along a wall when a collision occurs
//
// Returns the new position that slides along the wall surface
#[must_use]
pub fn calculate_wall_slide(
    walls: &[Wall],
    current_pos: &Position,
    velocity_x: f32,
    velocity_z: f32,
    delta: f32,
) -> Position {
    calculate_entity_wall_slide(
        walls,
        current_pos,
        velocity_x,
        velocity_z,
        delta,
        check_player_wall_overlap,
    )
}

// Calculate sliding movement along a wall when a collision occurs for ghosts
//
// Returns the new position that slides along the wall surface
#[must_use]
pub fn calculate_ghost_wall_slide(
    walls: &[Wall],
    current_pos: &Position,
    velocity_x: f32,
    velocity_z: f32,
    delta: f32,
) -> Position {
    calculate_entity_wall_slide(
        walls,
        current_pos,
        velocity_x,
        velocity_z,
        delta,
        check_ghost_wall_overlap,
    )
}

// Check if two players collide with each other (AABB collision).
#[must_use]
pub fn check_player_player_overlap(pos1: &Position, pos2: &Position) -> bool {
    let player_half_width = PLAYER_WIDTH / 2.0;
    let player_half_depth = PLAYER_DEPTH / 2.0;

    let p1_min_x = pos1.x - player_half_width;
    let p1_max_x = pos1.x + player_half_width;
    let p1_min_z = pos1.z - player_half_depth;
    let p1_max_z = pos1.z + player_half_depth;

    let p2_min_x = pos2.x - player_half_width;
    let p2_max_x = pos2.x + player_half_width;
    let p2_min_z = pos2.z - player_half_depth;
    let p2_max_z = pos2.z + player_half_depth;

    ranges_overlap(p1_min_x, p1_max_x, p2_min_x, p2_max_x) && ranges_overlap(p1_min_z, p1_max_z, p2_min_z, p2_max_z)
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

// Check if a player is close enough to an item to collect it (circle collision)
#[must_use]
pub fn check_player_item_overlap(player_pos: &Position, item_pos: &Position, collection_radius: f32) -> bool {
    let dx = player_pos.x - item_pos.x;
    let dz = player_pos.z - item_pos.z;
    let dist_sq = dx.mul_add(dx, dz * dz);
    dist_sq <= collection_radius * collection_radius
}
