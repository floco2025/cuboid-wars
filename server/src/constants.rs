// ============================================================================
// Map Generation
// ============================================================================

// Walls
pub const WALL_NUM_SEGMENTS: usize = 35;
pub const WALL_2ND_PROBABILITY_RATIO: f64 = 5.0; // Probability of 2nd wall relative to 1st
pub const WALL_3RD_PROBABILITY_RATIO: f64 = 0.2; // Probability of 3rd wall relative to 1st
pub const WALL_OVERLAP: bool = false; // Non-overlapping mode by default
pub const WALL_MERGE_SEGMENTS: bool = true; // Reduce draw calls

// Building shape (deterministic blueprint generator)
pub const NUM_LEVELS: u32 = 5; // basement / lobby / rooms-low / rooms-high / rooftop
pub const BUILDING_FOOTPRINT_CELLS: i32 = 20; // Building fills the whole grid; field is the building
pub const ROOFTOP_FOOTPRINT_CELLS: i32 = 14; // Setback at the top
pub const RAMP_LENGTH_CELLS: i32 = 2;
pub const RAMP_WIDTH_CELLS: i32 = 1;

// Floor mesh
pub const FLOOR_OVERLAP: bool = false; // Non-overlapping mode (uses edge fillers)
pub const FLOOR_MERGE_SEGMENTS: bool = true; // Reduce draw calls

// ============================================================================
// Lighting
// ============================================================================

pub const WALL_LIGHT_HEIGHT: f32 = 2.5; // meters above ground

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
