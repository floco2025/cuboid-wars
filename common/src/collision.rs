use crate::{constants::*, protocol::{Movement, Position, Wall, WallOrientation}};
use crate::systems::Projectile;

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
pub fn check_projectile_hit(
    proj_pos: &Position,
    projectile: &Projectile,
    delta: f32,
    player_pos: &Position,
    player_mov: &Movement,
) -> HitResult {
    // Calculate projectile movement this frame
    let ray_dir_x = projectile.velocity.x * delta;
    let ray_dir_y = projectile.velocity.y * delta;
    let ray_dir_z = projectile.velocity.z * delta;

    // Transform projectile position and ray into player's local space
    let dx = proj_pos.x - player_pos.x;
    let dz = proj_pos.z - player_pos.z;

    let cos_rot = player_mov.face_dir.cos();
    let sin_rot = player_mov.face_dir.sin();

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
    if ray_local_x.abs() > 1e-6 {
        let t1 = (-half_width - local_x) / ray_local_x;
        let t2 = (half_width - local_x) / ray_local_x;
        t_min = t_min.max(t1.min(t2));
        t_max = t_max.min(t1.max(t2));
    } else if local_x.abs() > half_width {
        return HitResult { hit: false, hit_dir_x: 0.0, hit_dir_z: 0.0 };
    }

    // Check Y slab
    if ray_local_y.abs() > 1e-6 {
        let t1 = (-half_height - local_y) / ray_local_y;
        let t2 = (half_height - local_y) / ray_local_y;
        t_min = t_min.max(t1.min(t2));
        t_max = t_max.min(t1.max(t2));
    } else if local_y.abs() > half_height {
        return HitResult { hit: false, hit_dir_x: 0.0, hit_dir_z: 0.0 };
    }

    // Check Z slab
    if ray_local_z.abs() > 1e-6 {
        let t1 = (-half_depth - local_z) / ray_local_z;
        let t2 = (half_depth - local_z) / ray_local_z;
        t_min = t_min.max(t1.min(t2));
        t_max = t_max.min(t1.max(t2));
    } else if local_z.abs() > half_depth {
        return HitResult { hit: false, hit_dir_x: 0.0, hit_dir_z: 0.0 };
    }

    // Hit if intervals overlap
    if t_min <= t_max && t_max >= 0.0 && t_min <= 1.0 {
        // Normalize the projectile velocity to get hit direction
        let vel_len = (projectile.velocity.x * projectile.velocity.x
            + projectile.velocity.z * projectile.velocity.z)
            .sqrt();
        let hit_dir_x = if vel_len > 0.0 { projectile.velocity.x / vel_len } else { 0.0 };
        let hit_dir_z = if vel_len > 0.0 { projectile.velocity.z / vel_len } else { 0.0 };
        
        HitResult { hit: true, hit_dir_x, hit_dir_z }
    } else {
        HitResult { hit: false, hit_dir_x: 0.0, hit_dir_z: 0.0 }
    }
}

// Check if a projectile hits a wall
// Returns true if the projectile intersects with the wall
pub fn check_projectile_wall_hit(
    proj_pos: &Position,
    projectile: &Projectile,
    delta: f32,
    wall: &Wall,
) -> bool {
    // Calculate projectile movement this frame
    let ray_start_x = proj_pos.x;
    let ray_start_y = proj_pos.y;
    let ray_start_z = proj_pos.z;
    
    let ray_dir_x = projectile.velocity.x * delta;
    let ray_dir_y = projectile.velocity.y * delta;
    let ray_dir_z = projectile.velocity.z * delta;
    
    // Wall dimensions
    let half_thickness = WALL_WIDTH / 2.0 + PROJECTILE_RADIUS;
    let half_height = WALL_HEIGHT / 2.0 + PROJECTILE_RADIUS;
    
    match wall.orientation {
        WallOrientation::Horizontal => {
            // Wall extends along X axis at position (wall.x, wall.z)
            // Wall center is at (wall.x, WALL_HEIGHT/2, wall.z)
            let half_length = WALL_LENGTH / 2.0 + PROJECTILE_RADIUS;
            
            // Transform to wall's local space (centered at wall position)
            let local_x = ray_start_x - wall.x;
            let local_y = ray_start_y - WALL_HEIGHT / 2.0;
            let local_z = ray_start_z - wall.z;
            
            // Check intersection with axis-aligned box using slab method
            let mut t_min = 0.0_f32;
            let mut t_max = 1.0_f32;
            
            // X slab (length of wall)
            if ray_dir_x.abs() > 1e-6 {
                let t1 = (-half_length - local_x) / ray_dir_x;
                let t2 = (half_length - local_x) / ray_dir_x;
                t_min = t_min.max(t1.min(t2));
                t_max = t_max.min(t1.max(t2));
            } else if local_x.abs() > half_length {
                return false;
            }
            
            // Y slab (height of wall)
            if ray_dir_y.abs() > 1e-6 {
                let t1 = (-half_height - local_y) / ray_dir_y;
                let t2 = (half_height - local_y) / ray_dir_y;
                t_min = t_min.max(t1.min(t2));
                t_max = t_max.min(t1.max(t2));
            } else if local_y.abs() > half_height {
                return false;
            }
            
            // Z slab (thickness of wall)
            if ray_dir_z.abs() > 1e-6 {
                let t1 = (-half_thickness - local_z) / ray_dir_z;
                let t2 = (half_thickness - local_z) / ray_dir_z;
                t_min = t_min.max(t1.min(t2));
                t_max = t_max.min(t1.max(t2));
            } else if local_z.abs() > half_thickness {
                return false;
            }
            
            t_min <= t_max && t_max >= 0.0 && t_min <= 1.0
        },
        WallOrientation::Vertical => {
            // Wall extends along Z axis at position (wall.x, wall.z)
            // Wall center is at (wall.x, WALL_HEIGHT/2, wall.z)
            let half_length = WALL_LENGTH / 2.0 + PROJECTILE_RADIUS;
            
            // Transform to wall's local space
            let local_x = ray_start_x - wall.x;
            let local_y = ray_start_y - WALL_HEIGHT / 2.0;
            let local_z = ray_start_z - wall.z;
            
            // Check intersection with axis-aligned box using slab method
            let mut t_min = 0.0_f32;
            let mut t_max = 1.0_f32;
            
            // X slab (thickness of wall)
            if ray_dir_x.abs() > 1e-6 {
                let t1 = (-half_thickness - local_x) / ray_dir_x;
                let t2 = (half_thickness - local_x) / ray_dir_x;
                t_min = t_min.max(t1.min(t2));
                t_max = t_max.min(t1.max(t2));
            } else if local_x.abs() > half_thickness {
                return false;
            }
            
            // Y slab (height of wall)
            if ray_dir_y.abs() > 1e-6 {
                let t1 = (-half_height - local_y) / ray_dir_y;
                let t2 = (half_height - local_y) / ray_dir_y;
                t_min = t_min.max(t1.min(t2));
                t_max = t_max.min(t1.max(t2));
            } else if local_y.abs() > half_height {
                return false;
            }
            
            // Z slab (length of wall)
            if ray_dir_z.abs() > 1e-6 {
                let t1 = (-half_length - local_z) / ray_dir_z;
                let t2 = (half_length - local_z) / ray_dir_z;
                t_min = t_min.max(t1.min(t2));
                t_max = t_max.min(t1.max(t2));
            } else if local_z.abs() > half_length {
                return false;
            }
            
            t_min <= t_max && t_max >= 0.0 && t_min <= 1.0
        },
    }
}

// Check if a player position intersects with a wall
pub fn check_player_wall_collision(player_pos: &Position, wall: &Wall) -> bool {
    // Player dimensions
    let player_half_width = PLAYER_WIDTH / 2.0;
    let player_half_depth = PLAYER_DEPTH / 2.0;
    
    match wall.orientation {
        WallOrientation::Horizontal => {
            // Wall extends along X axis at (wall.x, wall.z)
            let wall_half_length = WALL_LENGTH / 2.0;
            let wall_half_thickness = WALL_WIDTH / 2.0;
            
            // Check if player AABB overlaps with wall AABB
            let player_min_x = player_pos.x - player_half_width;
            let player_max_x = player_pos.x + player_half_width;
            let player_min_z = player_pos.z - player_half_depth;
            let player_max_z = player_pos.z + player_half_depth;
            
            let wall_min_x = wall.x - wall_half_length;
            let wall_max_x = wall.x + wall_half_length;
            let wall_min_z = wall.z - wall_half_thickness;
            let wall_max_z = wall.z + wall_half_thickness;
            
            // AABB overlap test
            player_max_x > wall_min_x && player_min_x < wall_max_x &&
            player_max_z > wall_min_z && player_min_z < wall_max_z
        },
        WallOrientation::Vertical => {
            // Wall extends along Z axis at (wall.x, wall.z)
            let wall_half_length = WALL_LENGTH / 2.0;
            let wall_half_thickness = WALL_WIDTH / 2.0;
            
            // Check if player AABB overlaps with wall AABB
            let player_min_x = player_pos.x - player_half_width;
            let player_max_x = player_pos.x + player_half_width;
            let player_min_z = player_pos.z - player_half_depth;
            let player_max_z = player_pos.z + player_half_depth;
            
            let wall_min_x = wall.x - wall_half_thickness;
            let wall_max_x = wall.x + wall_half_thickness;
            let wall_min_z = wall.z - wall_half_length;
            let wall_max_z = wall.z + wall_half_length;
            
            // AABB overlap test
            player_max_x > wall_min_x && player_min_x < wall_max_x &&
            player_max_z > wall_min_z && player_min_z < wall_max_z
        },
    }
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
    
    // AABB overlap test
    p1_max_x > p2_min_x && p1_min_x < p2_max_x &&
    p1_max_z > p2_min_z && p1_min_z < p2_max_z
}

