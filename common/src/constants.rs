// ============================================================================
// Networking
// ============================================================================

pub const UPDATE_BROADCAST_INTERVAL: f32 = 0.25; // seconds

// ============================================================================
// Physics
// ============================================================================

// Small value for floating-point comparisons (near-zero checks, division guards).
pub const PHYSICS_EPSILON: f32 = 1e-6;

// ============================================================================
// Grid & Field
// ============================================================================

pub const GRID_SIZE: f32 = 4.0; // Each grid cell size in meters
pub const GRID_COLS: i32 = 20; // Number of grid columns (X axis)
pub const GRID_ROWS: i32 = 20; // Number of grid rows (Z axis)
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

// Speed (meters per second)
pub const PLAYER_SPEED: f32 = 9.0;

// Gravity acting on falling players. Higher than real-world (9.81) for snappier
// game feel.
pub const PLAYER_GRAVITY: f32 = 25.0; // m/s²

// Hard cap on a falling player's downward speed. Prevents arbitrarily large
// velocities from very tall drops.
pub const PLAYER_TERMINAL_VELOCITY: f32 = 50.0; // m/s

// Vertical slop for floor-support detection: a player whose feet are within this
// distance of a floor's surface is considered supported by it.
pub const PLAYER_LANDING_EPSILON: f32 = 0.5;

// Players whose Y falls below this die and respawn.
pub const PLAYER_DEATH_Y: f32 = -100.0;

// ============================================================================
// Projectiles
// ============================================================================

pub const PROJECTILE_SPEED: f32 = 70.0; // meters per second
pub const PROJECTILE_LIFETIME: f32 = 8.0; // seconds
pub const PROJECTILE_SPAWN_OFFSET: f32 = 1.0; // meters in front of thrower
pub const PROJECTILE_RADIUS: f32 = 0.11; // meters
pub const PROJECTILE_COOLDOWN_TIME: f32 = 0.1; // Minimum time between shots
pub const PROJECTILE_GRAVITY: f32 = 9.81; // m/s² (real-world for nice arcs)
pub const PROJECTILE_DRAG_FACTOR: f32 = 0.01; // Air resistance coefficient applied per frame
pub const PROJECTILE_BOUNCE_RETENTION: f32 = 0.9; // fraction of speed retained after bounce (0.0-1.0)

// ============================================================================
// Map Geometry
// ============================================================================

// Walls
pub const WALL_THICKNESS: f32 = 0.3;
pub const WALL_HEIGHT: f32 = 4.0;

// Floors
pub const FLOOR_THICKNESS: f32 = 0.4;
// Vertical spacing per level. The standing surface of level k is at `LEVEL_HEIGHT * k`.
pub const LEVEL_HEIGHT: f32 = WALL_HEIGHT + FLOOR_THICKNESS;

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
