// ============================================================================
// Map Geometry
// ============================================================================

// Grid
pub const GRID_CELL_SIZE: f32 = 3.4;

// Walls
pub const WALL_THICKNESS: f32 = 0.3;
pub const WALL_HALF_THICKNESS: f32 = WALL_THICKNESS / 2.0;
pub const WALL_HEIGHT: f32 = 4.0;

// Barriers. Force-field segments authored on grid edges; same shape as walls
// but rendered as translucent pulsating geometry on the client. Each kind
// gets its own collision group (`barrier_collision_group`) so held keys and
// open pressure plates gate pass-through per color
// (`passable_barrier_kinds`).
pub const BARRIER_THICKNESS: f32 = WALL_THICKNESS / 6.0;
pub const BARRIER_HEIGHT: f32 = WALL_HEIGHT;
// Visual overlap into adjacent floors and walls to win the depth test where
// surfaces would otherwise be coplanar. The barrier mesh grows by this amount
// at each end (along the segment) and at top/bottom (in Y).
pub const BARRIER_OVERLAP_EPS: f32 = 0.01;

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

// Rate at which the server broadcasts the full-state `SSnapshot` snapshot to
// all clients. The snapshot is the authoritative source of presence and
// state for entities that aren't carried by one-shot cues.
pub const SNAPSHOT_HZ: u32 = 4;
pub const SNAPSHOT_SECS: f32 = 1.0 / SNAPSHOT_HZ as f32;

// ============================================================================
// Game Tick
// ============================================================================

// Shared game tick rate. Drives the server simulation loop, the client's
// physics-prediction `FixedUpdate`, server actor-AI decisions, and client
// player-input commits. Lower = less CPU and bandwidth, higher = more
// responsive AI and input.
pub const TICK_HZ: u32 = 30;
pub const TICK_SECS: f32 = 1.0 / TICK_HZ as f32;

// ============================================================================
// Physics
// ============================================================================

// Small value for floating-point comparisons (near-zero checks, division guards).
pub const PHYSICS_EPSILON: f32 = 1e-6;

// ============================================================================
// Characters
// ============================================================================

// Characters whose Y falls below this are killed (players run through the
// normal death/respawn flow; actors are despawned outright).
pub const CHARACTER_FALL_DEATH_Y: f32 = -100.0;

// Hard cap on a falling character's downward speed. Prevents arbitrarily large
// velocities from very tall drops.
pub const CHARACTER_TERMINAL_VELOCITY: f32 = 50.0; // m/s

// How far the Rapier character controller may snap downward to stay attached to
// valid ground while walking over seams, ramps, and small frame-step gaps.
pub const CHARACTER_GROUND_SNAP_DISTANCE: f32 = 0.5;

// Inside this fraction of the blast radius the blast is at full strength;
// past it, strength falls off quadratically to zero at the rim (closer to
// real overpressure decay than a straight lerp — point blank is decisively
// worse than a rim graze).
pub const EXPLOSION_BLAST_CORE_FRACTION: f32 = 0.25;

// Blast knockback: horizontal shove speed at the blast center and the
// vertical launch speed added to `CharacterVerticalVelocity`. Both fall off
// with distance like damage but ignore armor — armor protects health, not
// momentum.
pub const EXPLOSION_KNOCKBACK_MAX_SPEED: f32 = 15.0; // m/s
pub const EXPLOSION_KNOCKBACK_UP_SPEED: f32 = 7.0; // m/s
// Ground-friction-style linear deceleration of the horizontal shove: a hard
// hit that dies cleanly (~0.4 s from full speed), no exponential crawl tail.
pub const KNOCKBACK_DECELERATION: f32 = 35.0; // m/s²

// Maximum low ledge height the Rapier character controller may auto-step over.
pub const CHARACTER_STEP_HEIGHT: f32 = 0.2;

// Minimum forward clearance Rapier requires after an auto-step. This must be
// large enough to carry the character past thin slab/trim edges, not just onto
// the edge contact itself.
pub const CHARACTER_STEP_MIN_WIDTH: f32 = 0.2;

// Horizontal speed a perched character (support probe airborne, collider
// still resting on an edge sliver) is pushed off its support. Below walk
// speed so player input can always override it and walk back on.
pub const CHARACTER_PERCH_SLIDE_SPEED: f32 = 3.0; // m/s

// ============================================================================
// Power-Ups
// ============================================================================

pub const POWER_UP_SPEED_MULTIPLIER: f32 = 1.8;
pub const POWER_UP_MULTI_SHOT_MULTIPLIER: i32 = 5;
pub const POWER_UP_MULTI_SHOT_ANGLE: f32 = 1.5;
