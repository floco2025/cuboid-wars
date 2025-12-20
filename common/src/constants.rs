// ============================================================================
// Networking
// ============================================================================

pub const UPDATE_BROADCAST_INTERVAL: f32 = 0.25; // seconds

// ============================================================================
// Grid & Field
// ============================================================================

pub const GRID_SIZE: f32 = 8.0; // Each grid cell size in meters
pub const GRID_COLS: i32 = 10; // Number of grid columns (X axis)
pub const GRID_ROWS: i32 = 10; // Number of grid rows (Z axis)
pub const FIELD_WIDTH: f32 = GRID_COLS as f32 * GRID_SIZE; // Total field width (80m)
pub const FIELD_DEPTH: f32 = GRID_ROWS as f32 * GRID_SIZE; // Total field depth (80m)

// ============================================================================
// Player
// ============================================================================

// Dimensions (meters)
pub const PLAYER_HEIGHT: f32 = 1.8; // up/down
pub const PLAYER_WIDTH: f32 = 1.0; // side to side
pub const PLAYER_DEPTH: f32 = 0.6; // front to back
pub const PLAYER_EYE_HEIGHT_RATIO: f32 = 0.9; // Eye/camera height as ratio of player height

// Speeds (meters per second)
pub const SPEED_WALK: f32 = 9.0;
pub const SPEED_RUN: f32 = 9.0;

// ============================================================================
// Projectiles
// ============================================================================

pub const PROJECTILE_SPEED: f32 = 25.0; // meters per second
pub const PROJECTILE_LIFETIME: f32 = 4.0; // seconds
pub const PROJECTILE_SPAWN_OFFSET: f32 = 1.0; // meters in front of thrower
pub const PROJECTILE_RADIUS: f32 = 0.11; // meters
pub const PROJECTILE_COOLDOWN_TIME: f32 = 0.1; // Minimum time between shots

// ============================================================================
// Sentries
// ============================================================================

// Dimensions (meters)
pub const SENTRY_HEIGHT: f32 = 3.0; // up/down
pub const SENTRY_WIDTH: f32 = 2.0; // side to side
pub const SENTRY_DEPTH: f32 = 1.5; // front to back

// ============================================================================
// Map Geometry
// ============================================================================

// Walls
pub const WALL_THICKNESS: f32 = 0.3;
pub const WALL_HEIGHT: f32 = 4.0;
pub const WALL_LENGTH: f32 = 8.2; // Slightly longer than grid to avoid corner gaps

// Roofs
pub const ROOF_THICKNESS: f32 = 0.4;
pub const ROOF_HEIGHT: f32 = WALL_HEIGHT + ROOF_THICKNESS; // Top of roof
pub const ROOF_WALL_THICKNESS: f32 = 0.1; // Roof-edge collision barrier

// ============================================================================
// Power-Ups
// ============================================================================

pub const POWER_UP_SPEED_MULTIPLIER: f32 = 1.8;
pub const POWER_UP_MULTI_SHOT_MULTIPLER: i32 = 5;
pub const POWER_UP_MULTI_SHOT_ANGLE: f32 = 2.0;

// ============================================================================
// Debug Flags
// ============================================================================

pub const ALWAYS_SPEED: bool = false;
pub const ALWAYS_MULTI_SHOT: bool = false;
pub const ALWAYS_PHASING: bool = false;
pub const ALWAYS_SENTRY_HUNT: bool = false;
