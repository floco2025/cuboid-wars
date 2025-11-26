// ============================================================================
// Shared Game Constants
// ============================================================================

// Playing field dimensions (meters)
pub const FIELD_WIDTH: f32 = 40.0; // X axis - total width
pub const FIELD_DEPTH: f32 = 40.0; // Z axis - total depth
pub const SPAWN_RANGE_X: f32 = FIELD_WIDTH / 2.0;
pub const SPAWN_RANGE_Z: f32 = FIELD_DEPTH / 2.0;

// Player dimensions (meters)
pub const PLAYER_WIDTH: f32 = 0.5; // side to side
pub const PLAYER_HEIGHT: f32 = 1.8; // up/down
pub const PLAYER_DEPTH: f32 = 0.3; // front to back

// Player movement speeds (meters per second)
pub const WALK_SPEED: f32 = 5.0;
pub const RUN_SPEED: f32 = 8.0;

// Projectile constants
pub const PROJECTILE_SPEED: f32 = 20.0; // meters per second (dodgeball throw speed)
pub const PROJECTILE_LIFETIME: f32 = 3.0; // seconds
pub const PROJECTILE_SPAWN_OFFSET: f32 = 0.5; // meters in front of thrower
pub const PROJECTILE_SPAWN_HEIGHT: f32 = 1.5; // meters above ground (shoulder height)
pub const PROJECTILE_RADIUS: f32 = 0.11; // meters (22cm diameter dodgeball)

// Visual details (meters)
pub const PLAYER_NOSE_RADIUS: f32 = 0.08;
pub const PLAYER_EYE_RADIUS: f32 = 0.05;
pub const PLAYER_EYE_SPACING: f32 = 0.1; // distance from center
pub const PLAYER_EYE_HEIGHT: f32 = 0.7; // relative to ground
pub const PLAYER_NOSE_HEIGHT: f32 = 0.5; // relative to ground
