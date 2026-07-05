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
// for now but rendered as translucent pulsating geometry on the client.
// Currently no collision — players walk through them. A future "key" item
// will gate pass-through per color.
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

// Horizontal speed a perched character (support probe airborne, collider
// still resting on an edge sliver) is pushed off its support. Below walk
// speed so player input can always override it and walk back on.
pub const CHARACTER_PERCH_SLIDE_SPEED: f32 = 3.0; // m/s

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
pub const POWER_UP_MULTI_SHOT_ANGLE: f32 = 1.5;
pub const POWER_UP_ANTI_GRAVITY_MULTIPLIER: f32 = 0.2;

// Spawn gates. Set any flag to `false` to keep that power-up off the spawn
// pool — the type's collection / status / movement code stays in place, so a
// player still benefits if they somehow obtain one (e.g., via the `ALWAYS_*`
// debug flags below). With every flag `false`, only cookies spawn.
//
// These intentionally live in compile-time constants alongside `ALWAYS_*`,
// not in `PowerUpsConfig` JSON. Cookies have a separate
// `cookies.spawning_enabled` runtime toggle because cookies double as quest
// progress and a designer might want them off per-map; power-ups are
// session-global and the constant form matches the other tuning here.
pub const POWER_UP_SPEED_ENABLED: bool = true;
pub const POWER_UP_MULTI_SHOT_ENABLED: bool = true;
pub const POWER_UP_PHASING_ENABLED: bool = false;
pub const POWER_UP_ANTI_GRAVITY_ENABLED: bool = true;
pub const POWER_UP_HEALTH_POTION_ENABLED: bool = true;

// ============================================================================
// Debug Flags
// ============================================================================

pub const ALWAYS_SPEED: bool = false;
pub const ALWAYS_MULTI_SHOT: bool = false;
pub const ALWAYS_PHASING: bool = false;
pub const ALWAYS_ANTI_GRAVITY: bool = false;
