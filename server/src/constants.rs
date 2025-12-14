// ============================================================================
// Server Game Constants
// ============================================================================

// Wall generation settings
pub const WALL_NUM_SEGMENTS: usize = 30;
pub const WALL_2ND_PROBABILITY_RATIO: f64 = 5.0; // Probability of 2nd wall relative to 1st
pub const WALL_3RD_PROBABILITY_RATIO: f64 = 0.2; // Probability of 3rd wall relative to 1st

// Wall/Roof generation mode
// false: Non-overlapping mode - walls meet exactly at corners using smart extension logic
// true: Overlapping mode - all walls extend by wall_width/2 on both ends for guaranteed no-gap coverage
pub const OVERLAP_WALLS: bool = false;
pub const OVERLAP_ROOFS: bool = false;

// Optional merging of collinear/adjacent segments to reduce draw calls
pub const MERGE_WALL_SEGMENTS: bool = true;
pub const MERGE_ROOF_SEGMENTS: bool = true;

// Roof generation settings
pub const ROOF_NUM_SEGMENTS: usize = 20; // Target number of roof segments to generate
pub const ROOF_NEIGHBOR_PREFERENCE: f64 = 4.0; // Multiplier for cells with roofed neighbors

// Item settings
pub const ITEM_SPAWN_INTERVAL: f32 = 10.0; // seconds
pub const ITEM_LIFETIME: f32 = 60.0; // seconds
pub const ITEM_COLLECTION_RADIUS: f32 = 1.0; // Distance to collect an item

// Power-Up settings
pub const POWER_UP_SPEED_DURATION: f32 = 20.0; // seconds
pub const POWER_UP_MULTI_SHOT_DURATION: f32 = 20.0; // seconds
pub const POWER_UP_REFLECT_DURATION: f32 = 30.0; // seconds
pub const POWER_UP_PHASING_DURATION: f32 = 15.0; // seconds

// Ghost settings
pub const GHOSTS_NUM: u32 = 3; // Number of ghosts to spawn
pub const GHOST_SPEED: f32 = 6.0; // Speed in m/s (patrol mode)
pub const GHOST_FOLLOW_SPEED: f32 = 8.0; // Speed in m/s (follow mode)
pub const GHOST_RANDOM_TURN_PROBABILITY: f64 = 0.3; // Probability ghost randomly changes direction at intersection
pub const GHOST_FOLLOW_DURATION: f32 = 10.0; // How long ghost follows player (seconds)
pub const GHOST_COOLDOWN_DURATION: f32 = 15.0; // Cooldown before ghost can detect players again (seconds)
pub const GHOST_VISION_RANGE: f32 = 64.0; // Maximum distance to detect players (whole map)
pub const GHOST_STUN_DURATION: f32 = 3.0; // How long player is stunned after ghost hit (seconds)
pub const GHOST_HIT_PENALTY: i32 = 10; // Points lost when hit by ghost

// Cookie settings
pub const COOKIE_RESPAWN_TIME: f32 = 30.0; // seconds
pub const COOKIE_POINTS: i32 = 1; // points awarded per cookie
