// ============================================================================
// Map Generation
// ============================================================================

// Walls
pub const WALL_NUM_SEGMENTS: usize = 35;
pub const WALL_2ND_PROBABILITY_RATIO: f64 = 5.0; // Probability of 2nd wall relative to 1st
pub const WALL_3RD_PROBABILITY_RATIO: f64 = 0.2; // Probability of 3rd wall relative to 1st
pub const WALL_OVERLAP: bool = false; // Non-overlapping mode by default
pub const WALL_MERGE_SEGMENTS: bool = true; // Reduce draw calls

// Roofs
pub const ROOF_NUM_SEGMENTS: usize = 45;
pub const ROOF_NEIGHBOR_PREFERENCE: f64 = 4.0; // Multiplier for cells with roofed neighbors
pub const ROOF_OVERLAP: bool = false; // Non-overlapping mode by default
pub const ROOF_MERGE_SEGMENTS: bool = true; // Reduce draw calls

// Ramps
pub const RAMP_COUNT: usize = 5; // Max number of ramps
pub const RAMP_LENGTH_CELLS: i32 = 2; // Run length in grid cells
pub const RAMP_WIDTH_CELLS: i32 = 1; // Footprint width in grid cells
pub const RAMP_MIN_SEPARATION_CELLS: i32 = 3; // Minimum empty cells between ramps

// ============================================================================
// Lighting
// ============================================================================

pub const WALL_LIGHT_HEIGHT: f32 = 2.5; // meters above ground

// ============================================================================
// Sentries
// ============================================================================

pub const SENTRIES_NUM: u32 = 4;
pub const SENTRY_SPEED: f32 = 6.0; // m/s (patrol mode)
pub const SENTRY_FOLLOW_SPEED: f32 = 8.0; // m/s (follow mode)
pub const SENTRY_RANDOM_TURN_PROBABILITY: f64 = 0.3;
pub const SENTRY_TARGET_DURATION: f32 = 10.0; // seconds
pub const SENTRY_COOLDOWN_DURATION: f32 = 15.0; // seconds
pub const SENTRY_VISION_RANGE: f32 = 64.0; // meters (whole map)
pub const SENTRY_STUN_DURATION: f32 = 3.0; // seconds
pub const SENTRY_HIT_PENALTY: i32 = 10; // points lost when hit
pub const SENTRY_HIT_REWARD: i32 = 20; // points for hitting fleeing sentry

// ============================================================================
// Cookies
// ============================================================================

pub const COOKIE_RESPAWN_TIME: f32 = 30.0; // seconds
pub const COOKIE_POINTS: i32 = 1; // points per cookie

// ============================================================================
// Items
// ============================================================================

pub const ITEM_SPAWN_INTERVAL: f32 = 8.0; // seconds
pub const ITEM_LIFETIME: f32 = 60.0; // seconds
pub const ITEM_COLLECTION_RADIUS: f32 = 1.0; // meters

// ============================================================================
// Power-Ups
// ============================================================================

pub const POWER_UP_SPEED_DURATION: f32 = 20.0; // seconds
pub const POWER_UP_MULTI_SHOT_DURATION: f32 = 20.0; // seconds
pub const POWER_UP_PHASING_DURATION: f32 = 15.0; // seconds
pub const POWER_UP_SENTRY_HUNT_DURATION: f32 = 15.0; // seconds
