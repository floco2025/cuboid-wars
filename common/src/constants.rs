// ============================================================================
// Shared Game Constants
// ============================================================================

// Grid-based playing field dimensions
pub const GRID_SIZE: f32 = 8.0; // Each grid cell is 5 meters
pub const GRID_COLS: i32 = 10; // Number of grid columns (X axis)
pub const GRID_ROWS: i32 = 10; // Number of grid rows (Z axis)

// Calculated field dimensions (meters)
pub const FIELD_WIDTH: f32 = GRID_COLS as f32 * GRID_SIZE; // Total width
pub const FIELD_DEPTH: f32 = GRID_ROWS as f32 * GRID_SIZE; // Total depth

// Player dimensions (meters)
pub const PLAYER_WIDTH: f32 = 0.5; // side to side
pub const PLAYER_HEIGHT: f32 = 1.8; // up/down
pub const PLAYER_DEPTH: f32 = 0.3; // front to back

// Player speeds (meters per second)
pub const SPEED_WALK: f32 = 9.0;
pub const SPEED_RUN: f32 = 9.0;

// Projectile constants
pub const PROJECTILE_SPEED: f32 = 25.0; // meters per second
pub const PROJECTILE_LIFETIME: f32 = 4.0; // seconds
pub const PROJECTILE_SPAWN_OFFSET: f32 = 1.0; // meters in front of thrower
// Spawn from camera/eye height (match FPV camera height): ~90% of player height above ground
pub const PROJECTILE_SPAWN_HEIGHT: f32 = PLAYER_HEIGHT * 0.9;
pub const PROJECTILE_RADIUS: f32 = 0.11; // meters

// Wall dimensions (meters)
pub const WALL_THICKNESS: f32 = 0.3; // Wall thickness
pub const WALL_HEIGHT: f32 = 4.0; // Wall height
pub const WALL_LENGTH: f32 = 8.2; // Slightly longer than grid to avoid corner gaps

// Roof dimensions (meters)
pub const ROOF_THICKNESS: f32 = 0.4; // Roof thickness
pub const ROOF_HEIGHT: f32 = WALL_HEIGHT + ROOF_THICKNESS; // Top of roof
pub const ROOF_WALL_THICKNESS: f32 = 0.01; // Invisible roof-edge guard

// Visual details (meters)
pub const PLAYER_NOSE_RADIUS: f32 = 0.08;
pub const PLAYER_EYE_RADIUS: f32 = 0.05;
pub const PLAYER_EYE_SPACING: f32 = 0.1; // distance from center
pub const PLAYER_EYE_HEIGHT: f32 = 0.7; // relative to ground
pub const PLAYER_NOSE_HEIGHT: f32 = 0.5; // relative to ground

// Server update interval
pub const UPDATE_BROADCAST_INTERVAL: f32 = 0.25; // seconds

// Power-up setting
pub const POWER_UP_SPEED_MULTIPLIER: f32 = 1.8;
pub const POWER_UP_MULTI_SHOT_MULTIPLER: i32 = 5;
pub const POWER_UP_MULTI_SHOT_ANGLE: f32 = 2.0;

// Debug: Always enable power-ups (for testing)
pub const ALWAYS_SPEED: bool = false;
pub const ALWAYS_MULTI_SHOT: bool = true;
pub const ALWAYS_REFLECT: bool = true;
pub const ALWAYS_PHASING: bool = false;
pub const ALWAYS_GHOST_HUNT: bool = false;

// Ghost dimensions (meters)
pub const GHOST_SIZE: f32 = 3.0; // Cube side length
