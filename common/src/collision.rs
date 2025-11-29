use crate::systems::Projectile;
use crate::{
    constants::*,
    protocol::{Position, Wall, WallOrientation},
};

// ============================================================================
// Hit Detection
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
    let local_x = dx * cos_rot - dz * sin_rot;
    let local_z = dx * sin_rot + dz * cos_rot;
    let local_y = proj_pos.y - (player_pos.y + PLAYER_HEIGHT / 2.0);

    // Ray direction in local space
    let ray_local_x = ray_dir_x * cos_rot - ray_dir_z * sin_rot;
    let ray_local_z = ray_dir_x * sin_rot + ray_dir_z * cos_rot;
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
        let vel_len =
            (projectile.velocity.x * projectile.velocity.x + projectile.velocity.z * projectile.velocity.z).sqrt();
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

// Check if a player position intersects with a wall
pub fn check_player_wall_collision(player_pos: &Position, wall: &Wall) -> bool {
    // Player dimensions
    let player_half_width = PLAYER_WIDTH / 2.0;
    let player_half_depth = PLAYER_DEPTH / 2.0;

    let (wall_half_x, wall_half_z) = match wall.orientation {
        WallOrientation::Horizontal => (WALL_LENGTH / 2.0, WALL_WIDTH / 2.0),
        WallOrientation::Vertical => (WALL_WIDTH / 2.0, WALL_LENGTH / 2.0),
    };

    let player_min_x = player_pos.x - player_half_width;
    let player_max_x = player_pos.x + player_half_width;
    let player_min_z = player_pos.z - player_half_depth;
    let player_max_z = player_pos.z + player_half_depth;

    let wall_min_x = wall.x - wall_half_x;
    let wall_max_x = wall.x + wall_half_x;
    let wall_min_z = wall.z - wall_half_z;
    let wall_max_z = wall.z + wall_half_z;

    ranges_overlap(player_min_x, player_max_x, wall_min_x, wall_max_x)
        && ranges_overlap(player_min_z, player_max_z, wall_min_z, wall_max_z)
}

// Check if two players collide with each other
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

    ranges_overlap(p1_min_x, p1_max_x, p2_min_x, p2_max_x)
        && ranges_overlap(p1_min_z, p1_max_z, p2_min_z, p2_max_z)
}

fn slab_interval(
    local_coord: f32,
    ray_dir: f32,
    half_extent: f32,
    t_min: f32,
    t_max: f32,
) -> Option<(f32, f32)> {
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

fn no_hit() -> HitResult {
    HitResult {
        hit: false,
        hit_dir_x: 0.0,
        hit_dir_z: 0.0,
    }
}

fn ranges_overlap(a_min: f32, a_max: f32, b_min: f32, b_max: f32) -> bool {
    a_max > b_min && a_min < b_max
}
