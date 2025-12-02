use crate::systems::Projectile;
use crate::{
    constants::*,
    protocol::{Position, Wall, WallOrientation},
};

// ============================================================================
// Projectile Hit Detection
// ============================================================================

// Result of a hit detection check
#[derive(Debug, Clone, Copy)]
pub struct HitResult {
    pub hit: bool,
    pub hit_dir_x: f32,
    pub hit_dir_z: f32,
}

// Check if a projectile hits a player using swept sphere collision
// Returns HitResult with hit flag and normalized direction
#[must_use]
pub fn check_projectile_player_hit(
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

// Check if a projectile hits a wall
// Returns true if the projectile intersects with the wall
#[must_use]
pub fn check_projectile_wall_hit(proj_pos: &Position, projectile: &Projectile, delta: f32, wall: &Wall) -> bool {
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
        return false;
    }

    if let Some((min_y, max_y)) = slab_interval(local_y, ray_dir_y, half_height, t_min, t_max) {
        t_min = min_y;
        t_max = max_y;
    } else {
        return false;
    }

    if let Some((min_z, max_z)) = slab_interval(local_z, ray_dir_z, half_z, t_min, t_max) {
        t_min = min_z;
        t_max = max_z;
    } else {
        return false;
    }

    t_min <= t_max && t_max >= 0.0 && t_min <= 1.0
}

// ============================================================================
// Player Collisions Detection
// ============================================================================

// Check if a player position intersects with a wall
#[must_use]
pub fn check_player_wall_collision(player_pos: &Position, wall: &Wall) -> bool {
    let player_half_x = PLAYER_WIDTH / 2.0;
    let player_half_z = PLAYER_DEPTH / 2.0;

    let (wall_half_x, wall_half_z) = match wall.orientation {
        WallOrientation::Horizontal => (WALL_LENGTH / 2.0, WALL_WIDTH / 2.0),
        WallOrientation::Vertical => (WALL_WIDTH / 2.0, WALL_LENGTH / 2.0),
    };

    let player_min_x = player_pos.x - player_half_x;
    let player_max_x = player_pos.x + player_half_x;
    let player_min_z = player_pos.z - player_half_z;
    let player_max_z = player_pos.z + player_half_z;

    let wall_min_x = wall.x - wall_half_x;
    let wall_max_x = wall.x + wall_half_x;
    let wall_min_z = wall.z - wall_half_z;
    let wall_max_z = wall.z + wall_half_z;

    ranges_overlap(player_min_x, player_max_x, wall_min_x, wall_max_x)
        && ranges_overlap(player_min_z, player_max_z, wall_min_z, wall_max_z)
}

// Check if a ghost position intersects with a wall
#[must_use]
pub fn check_ghost_wall_collision(ghost_pos: &Position, wall: &Wall) -> bool {
    let ghost_half_size = GHOST_SIZE / 2.0;

    let (wall_half_x, wall_half_z) = match wall.orientation {
        WallOrientation::Horizontal => (WALL_LENGTH / 2.0, WALL_WIDTH / 2.0),
        WallOrientation::Vertical => (WALL_WIDTH / 2.0, WALL_LENGTH / 2.0),
    };

    let ghost_min_x = ghost_pos.x - ghost_half_size;
    let ghost_max_x = ghost_pos.x + ghost_half_size;
    let ghost_min_z = ghost_pos.z - ghost_half_size;
    let ghost_max_z = ghost_pos.z + ghost_half_size;

    let wall_min_x = wall.x - wall_half_x;
    let wall_max_x = wall.x + wall_half_x;
    let wall_min_z = wall.z - wall_half_z;
    let wall_max_z = wall.z + wall_half_z;

    ranges_overlap(ghost_min_x, ghost_max_x, wall_min_x, wall_max_x)
        && ranges_overlap(ghost_min_z, ghost_max_z, wall_min_z, wall_max_z)
}

// Check if two players collide with each other
#[must_use]
pub fn check_player_player_collision(pos1: &Position, pos2: &Position) -> bool {
    // Player dimensions
    let player_half_width = PLAYER_WIDTH / 2.0;
    let player_half_depth = PLAYER_DEPTH / 2.0;

    // Calculate AABBs for both players
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

// Check if a player position is close enough to an item to collect it
#[must_use]
pub fn check_player_item_collision(player_pos: &Position, item_pos: &Position, collection_radius: f32) -> bool {
    let dx = player_pos.x - item_pos.x;
    let dz = player_pos.z - item_pos.z;
    let dist_sq = dx.mul_add(dx, dz * dz);
    dist_sq <= collection_radius * collection_radius
}

// ============================================================================
// Wall Sliding
// ============================================================================

// Calculate sliding movement along a wall when a collision occurs
// Returns the new position that slides along the wall surface
#[must_use]
pub fn calculate_wall_slide(
    walls: &[Wall],
    current_pos: &Position,
    target_pos: &Position,
    velocity_x: f32,
    velocity_z: f32,
    delta: f32,
) -> Position {
    // Find which wall we're hitting
    for wall in walls {
        if !check_player_wall_collision(target_pos, wall) {
            continue;
        }

        // Get wall normal based on orientation
        let (wall_normal_x, wall_normal_z) = match wall.orientation {
            WallOrientation::Horizontal => (0.0, 1.0), // Normal points along Z
            WallOrientation::Vertical => (1.0, 0.0),   // Normal points along X
        };

        // Calculate which side of the wall we're on
        let to_wall_x = target_pos.x - wall.x;
        let to_wall_z = target_pos.z - wall.z;
        let dot = to_wall_x.mul_add(wall_normal_x, to_wall_z * wall_normal_z);

        // Flip normal if we're on the other side
        let (normal_x, normal_z) = if dot < 0.0 {
            (-wall_normal_x, -wall_normal_z)
        } else {
            (wall_normal_x, wall_normal_z)
        };

        // Calculate slide vector by removing the component of velocity along the normal
        let vel_dot_normal = velocity_x.mul_add(normal_x, velocity_z * normal_z);
        let slide_vel_x = vel_dot_normal.mul_add(-normal_x, velocity_x);
        let slide_vel_z = vel_dot_normal.mul_add(-normal_z, velocity_z);

        // Apply slide velocity from current position
        let slide_pos = Position {
            x: slide_vel_x.mul_add(delta, current_pos.x),
            y: current_pos.y,
            z: slide_vel_z.mul_add(delta, current_pos.z),
        };

        // Make sure the slide position doesn't collide with ANY wall
        let hits_any_wall = walls.iter().any(|w| check_player_wall_collision(&slide_pos, w));
        if !hits_any_wall {
            return slide_pos;
        }

        // If it still collides, just return current position
        return *current_pos;
    }

    // No collision found (shouldn't happen), return target
    *target_pos
}

// ============================================================================
// Helper Functions
// ============================================================================

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

const fn no_hit() -> HitResult {
    HitResult {
        hit: false,
        hit_dir_x: 0.0,
        hit_dir_z: 0.0,
    }
}

fn ranges_overlap(a_min: f32, a_max: f32, b_min: f32, b_max: f32) -> bool {
    a_max >= b_min && a_min <= b_max
}
