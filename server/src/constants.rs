// ============================================================================
// Server Game Constants
// ============================================================================

// Total number of wall segments to place
pub const NUM_WALL_SEGMENTS: usize = 35;

// Roof generation settings
pub const ROOF_PROBABILITY_3_WALLS_EDGE: f64 = 0.0; // Chance if cell has 3 walls and at edge
pub const ROOF_PROBABILITY_3_WALLS_INTERIOR: f64 = 0.7; // Chance if cell has 3 walls and not at edge

// Item settings
pub const ITEM_SPAWN_INTERVAL: f32 = 30.0; // seconds
pub const ITEM_LIFETIME: f32 = 60.0; // seconds
pub const ITEM_COLLECTION_RADIUS: f32 = 1.0; // Distance to collect an item

// Power-Up settings
pub const SPEED_POWER_UP_DURATION: f32 = 30.0; // seconds
pub const MULTI_SHOT_POWER_UP_DURATION: f32 = 15.0; // seconds
