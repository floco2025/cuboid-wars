// ============================================================================
// Networking
// ============================================================================

pub const UPDATE_BROADCAST_INTERVAL: f32 = 0.25; // seconds

// ============================================================================
// Floating-Point Comparisons
// ============================================================================

// Small value for floating-point comparisons (near-zero checks, division guards).
pub const PHYSICS_EPSILON: f32 = 1e-6;

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

//pub const PROJECTILE_SPEED: f32 = 25.0; // meters per second
pub const PROJECTILE_SPEED: f32 = 60.0; // meters per second
pub const PROJECTILE_LIFETIME: f32 = 6.0; // seconds
//pub const PROJECTILE_LIFETIME: f32 = 10.0; // seconds
pub const PROJECTILE_SPAWN_OFFSET: f32 = 1.0; // meters in front of thrower
pub const PROJECTILE_RADIUS: f32 = 0.11; // meters
pub const PROJECTILE_COOLDOWN_TIME: f32 = 0.1; // Minimum time between shots
pub const PROJECTILE_GRAVITY: f32 = 9.81; // meters per second squared
//pub const PROJECTILE_GRAVITY: f32 = 3.0; // meters per second squared

// Air resistance: F_drag = 0.5 * rho * v^2 * C_d * A
// Deceleration = F_drag / mass = (0.5 * rho * C_d * A / mass) * v^2
// We precompute the coefficient: 0.5 * rho * C_d * A / mass
// const PROJECTILE_MASS: f32 = 1.0; // kg
// const AIR_DENSITY: f32 = 1.225; // kg/m^3 at sea level
// const SPHERE_DRAG_COEFFICIENT: f32 = 0.47; // dimensionless
// const PROJECTILE_CROSS_SECTION: f32 = std::f32::consts::PI * PROJECTILE_RADIUS * PROJECTILE_RADIUS;
// pub const PROJECTILE_DRAG_FACTOR: f32 =
//     0.5 * AIR_DENSITY * SPHERE_DRAG_COEFFICIENT * PROJECTILE_CROSS_SECTION / PROJECTILE_MASS;
pub const PROJECTILE_DRAG_FACTOR: f32 = 0.01;
pub const PROJECTILE_BOUNCE_RETENTION: f32 = 0.8; // fraction of speed retained after bounce (0.0-1.0)

// ============================================================================
// Sentries
// ============================================================================

// Dimensions (meters)
pub const SENTRY_HEIGHT: f32 = 2.5; // up/down
pub const SENTRY_WIDTH: f32 = 3.7; // side to side
pub const SENTRY_DEPTH: f32 = 2.8; // front to back

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
