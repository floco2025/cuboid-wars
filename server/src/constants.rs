// ============================================================================
// Map Generation
// ============================================================================

// Walls
pub const WALL_NUM_SEGMENTS: usize = 35;
pub const WALL_2ND_PROBABILITY_RATIO: f64 = 5.0; // Probability of 2nd wall relative to 1st
pub const WALL_3RD_PROBABILITY_RATIO: f64 = 0.2; // Probability of 3rd wall relative to 1st
pub const WALL_OVERLAP: bool = false; // Non-overlapping mode by default
pub const WALL_MERGE_SEGMENTS: bool = true; // Reduce draw calls

// Map shape helpers
pub const MAP_FOOTPRINT_CELLS: i32 = 20; // Map fills the whole grid; field is the map
pub const ROOFTOP_FOOTPRINT_CELLS: i32 = 16; // Setback at the top (still wide enough to include both stair shafts)
pub const ATRIUM_CELLS: i32 = 5; // Side length of the central atrium void (5x5 cells = 20m x 20m)
pub const RAMP_LENGTH_CELLS: i32 = 2;
pub const RAMP_WIDTH_CELLS: i32 = 1;

// Floor mesh
pub const FLOOR_OVERLAP: bool = false; // Non-overlapping mode (uses edge fillers)
pub const FLOOR_MERGE_SEGMENTS: bool = true; // Reduce draw calls

// ============================================================================
// Lighting
// ============================================================================

pub const WALL_LIGHT_HEIGHT: f32 = 2.5; // meters above ground
pub const WALL_LIGHT_EXPOSURE_THRESHOLD: f32 = 0.3;
pub const EXTERIOR_LIGHT_SEED: f32 = 1.0;
pub const EXTERIOR_LIGHT_STEP_RETENTION: f32 = 0.75;

// ============================================================================
// Cookies
// ============================================================================

pub const COOKIE_SPAWNING_ENABLED: bool = false;
pub const COOKIE_RESPAWN_TIME: f32 = 30.0; // seconds
pub const COOKIE_POINTS: i32 = 1; // points per cookie

// ============================================================================
// Items
// ============================================================================

pub const ITEM_LIFETIME: f32 = 60.0; // seconds
pub const ITEM_COLLECTION_RADIUS: f32 = 1.0; // meters
pub const ITEM_CELLS_PER_ACTIVE: usize = 60;
pub const ITEM_MIN_ACTIVE: usize = 2;
pub const ITEM_MAX_ACTIVE: usize = 20;

// ============================================================================
// Power-Ups
// ============================================================================

pub const POWER_UP_SPEED_DURATION: f32 = 20.0; // seconds
pub const POWER_UP_MULTI_SHOT_DURATION: f32 = 20.0; // seconds
pub const POWER_UP_PHASING_DURATION: f32 = 15.0; // seconds
