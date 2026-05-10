// ============================================================================
// Map Generation
// ============================================================================

// Wall meshes overlap at corners when true; non-overlapping mode adjusts
// segment ends so neighbours abut cleanly.
pub const WALL_OVERLAP: bool = false;

// Floor meshes overlap at cell boundaries when true; non-overlapping mode
// uses thin edge fillers to bridge corners.
pub const FLOOR_OVERLAP: bool = false;

// ============================================================================
// Lighting
// ============================================================================

// Generated interior lights are skipped in cells that are naturally lit by the
// exterior. Exposure starts at sky-open cells and boundary openings, then fades
// by `EXTERIOR_LIGHT_STEP_RETENTION` for each open grid step; cells below
// `WALL_LIGHT_EXPOSURE_THRESHOLD` are considered dark enough for wall lights.
pub const WALL_LIGHT_HEIGHT: f32 = 2.5; // meters above ground
pub const WALL_LIGHT_EXPOSURE_THRESHOLD: f32 = 0.3;
pub const EXTERIOR_LIGHT_STEP_RETENTION: f32 = 0.75;

// ============================================================================
// Actors
// ============================================================================

// Actor movement-intent broadcast throttling.
pub const ACTOR_MOVE_INTENT_SEND_COOLDOWN: f32 = 0.1;
pub const ACTOR_MOVE_INTENT_DIR_CHANGE_THRESHOLD: f32 = 2.0; // degrees

// ============================================================================
// Cookies
// ============================================================================

pub const COOKIE_SPAWNING_ENABLED: bool = false;
pub const COOKIE_RESPAWN_TIME: f32 = 30.0;
pub const COOKIE_POINTS: i32 = 1; // points per cookie

// ============================================================================
// Items
// ============================================================================

pub const ITEM_LIFETIME: f32 = 60.0;
pub const ITEM_COLLECTION_RADIUS: f32 = 1.0;
pub const ITEM_CELLS_PER_ACTIVE: usize = 60;
pub const ITEM_MIN_ACTIVE: usize = 2;
pub const ITEM_MAX_ACTIVE: usize = 20;

// ============================================================================
// Power-Ups
// ============================================================================

pub const POWER_UP_SPEED_DURATION: f32 = 20.0;
pub const POWER_UP_MULTI_SHOT_DURATION: f32 = 20.0;
pub const POWER_UP_PHASING_DURATION: f32 = 15.0;
