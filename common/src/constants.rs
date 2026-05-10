// ============================================================================
// Map Geometry
// ============================================================================

// Grid
pub const GRID_CELL_SIZE: f32 = 4.0;

// Walls
pub const WALL_THICKNESS: f32 = 0.3;
pub const WALL_HALF_THICKNESS: f32 = WALL_THICKNESS / 2.0;
pub const WALL_HEIGHT: f32 = 4.0;

// Floors
pub const FLOOR_THICKNESS: f32 = 0.4;

// Levels
pub const LEVEL_HEIGHT: f32 = WALL_HEIGHT + FLOOR_THICKNESS;

// Y tolerance for mapping a world position to a discrete map level. This keeps
// brief jumps and small vertical prediction differences from changing render/filter level.
pub const LEVEL_CLASSIFICATION_TOLERANCE: f32 = 0.5;

// ============================================================================
// Networking
// ============================================================================

pub const UPDATE_BROADCAST_INTERVAL: f32 = 0.25;

// ============================================================================
// Physics
// ============================================================================

// Small value for floating-point comparisons (near-zero checks, division guards).
pub const PHYSICS_EPSILON: f32 = 1e-6;

// ============================================================================
// Characters
// ============================================================================

// Characters whose Y falls below this are teleported back to a spawn position.
pub const CHARACTER_FALL_TELEPORT_Y: f32 = -100.0;

// Gravity acting on falling characters. Higher than real-world (9.81) for snappier
// game feel.
pub const CHARACTER_GRAVITY: f32 = 25.0; // m/s²

// Hard cap on a falling character's downward speed. Prevents arbitrarily large
// velocities from very tall drops.
pub const CHARACTER_TERMINAL_VELOCITY: f32 = 50.0; // m/s

// How far the Rapier character controller may snap downward to stay attached to
// valid ground while walking over seams, ramps, and small frame-step gaps.
pub const CHARACTER_GROUND_SNAP_DISTANCE: f32 = 0.5;

// Maximum low ledge height the Rapier character controller may auto-step over.
pub const CHARACTER_STEP_HEIGHT: f32 = 0.2;

// Minimum forward clearance Rapier requires after an auto-step. This must be
// large enough to carry the character past thin slab/trim edges, not just onto
// the edge contact itself.
pub const CHARACTER_STEP_MIN_WIDTH: f32 = 0.2;

// ============================================================================
// Player
// ============================================================================

pub const PLAYER_JUMP_SPEED: f32 = 12.0; // m/s initial upward velocity

// ============================================================================
// Projectiles
// ============================================================================

pub const PROJECTILE_SPEED: f32 = 70.0;
pub const PROJECTILE_LIFETIME: f32 = 8.0;
pub const PROJECTILE_SPAWN_OFFSET: f32 = 1.0; // in front of thrower
pub const PROJECTILE_RADIUS: f32 = 0.11;
pub const PROJECTILE_COOLDOWN_TIME: f32 = 0.1; // Minimum time between shots
pub const PROJECTILE_GRAVITY: f32 = 9.81; // m/s² (real-world for nice arcs)
pub const PROJECTILE_DRAG_FACTOR: f32 = 0.01; // Air resistance coefficient applied per frame
pub const PROJECTILE_BOUNCE_RETENTION: f32 = 0.9; // fraction of speed retained after bounce (0.0-1.0)

// ============================================================================
// Power-Ups
// ============================================================================

pub const POWER_UP_SPEED_MULTIPLIER: f32 = 1.8;
pub const POWER_UP_MULTI_SHOT_MULTIPLIER: i32 = 5;
pub const POWER_UP_MULTI_SHOT_ANGLE: f32 = 2.0;

// ============================================================================
// Debug Flags
// ============================================================================

pub const ALWAYS_SPEED: bool = false;
pub const ALWAYS_MULTI_SHOT: bool = false;
pub const ALWAYS_PHASING: bool = false;
