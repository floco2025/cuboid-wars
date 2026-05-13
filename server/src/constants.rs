// ============================================================================
// Lighting
// ============================================================================

// Vertical position of every wall lamp, measured from the floor it sits on.
// Lamps are placed manually in the editor (per-level `lights` arrays in
// `map.json`); this constant sets their height above the level floor.
pub const WALL_LIGHT_HEIGHT: f32 = 2.5;

// ============================================================================
// Cookies
// ============================================================================

pub const COOKIE_SPAWNING_ENABLED: bool = true;
pub const COOKIE_RESPAWN_TIME: f32 = 60.0;
pub const COOKIE_POINTS: i32 = 1; // points per cookie

// ============================================================================
// Keys
// ============================================================================

// Time before a collected world-key reappears in the same cell. Mirrors the
// cookie respawn mechanism (server tick decrements `spawn_time`; broadcast
// hides the entity while it's > 0).
pub const KEY_RESPAWN_TIME: f32 = 30.0;

// ============================================================================
// Items
// ============================================================================

pub const ITEM_LIFETIME: f32 = 60.0;
pub const ITEM_COLLECTION_RADIUS: f32 = 1.0;
// Target number of active power-ups in the world at any time. The actual
// count is capped at the number of eligible floor cells so tiny test maps
// degrade gracefully.
pub const ITEM_TARGET_ACTIVE: usize = 50;

// ============================================================================
// Power-Ups
// ============================================================================

pub const POWER_UP_SPEED_DURATION: f32 = 30.0;
pub const POWER_UP_MULTI_SHOT_DURATION: f32 = 25.0;
pub const POWER_UP_PHASING_DURATION: f32 = 15.0;
pub const POWER_UP_ANTI_GRAVITY_DURATION: f32 = 20.0;
