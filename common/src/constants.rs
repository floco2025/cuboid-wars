use std::time::Duration;

// ============================================================================
// Map Geometry
// ============================================================================

// The grid cell size, storey height, and floor and wall thicknesses are per
// map (`MapGeometryConfig` in the map settings); every size that follows them
// is a fraction here, computed there.

// Barriers. Force-field segments authored on grid edges; same shape as walls
// (wall height, this fraction of the wall thickness) but rendered as
// translucent pulsating geometry on the client. Every barrier shares one
// collision group and a query passes the instances its holder's keys and
// the open pressure plates allow (`passable_barriers`).
pub const BARRIER_THICKNESS_FRACTION: f32 = 1.0 / 6.0;

// Slab thickness of a light bridge, as a fraction of the floor thickness.
pub const BRIDGE_THICKNESS_FRACTION: f32 = 0.25;

// Height of a wall light above its floor, as a fraction of the wall height.
pub const WALL_LIGHT_HEIGHT_FRACTION: f32 = 0.625;

// Ladders. Freestanding climbable elements anchored on grid edges. One-sided:
// the rail side (front) climbs and fences; the back is passed through. No
// Rapier collider — the character step queries the derived volumes directly.
pub const LADDER_WIDTH: f32 = 1.2;
// How far the climb volume reaches in front of the rail plane (the back of
// a ladder is not a ladder).
pub const LADDER_VOLUME_DEPTH: f32 = 0.8;
// Volume and rails extend this far above the top storey's floor surface so
// the last climb tick leaves the feet over the landing and stepping off the
// landing edge immediately re-enters the volume.
pub const LADDER_OVERSHOOT: f32 = 0.5;
// The climb volume also reaches this far below the base, so stepping through
// the back at the lowest storey still grabs the ladder, and a descent ends
// hanging at the last rung instead of sliding off the end.
pub const LADDER_BASE_OVERSHOOT: f32 = 0.4;
// How far the rails stand off the anchoring grid edge, on the authored
// side. The physics plane runs THROUGH the rails — the ladder is where it
// looks like it is — so both the climb hold and the fence are measured from
// here, and the client mesh places the rails here.
pub const LADDER_RAIL_INSET: f32 = 0.22;
// Gap between the movement capsule and the ladder rail plane.
pub const LADDER_STANDOFF_CLEARANCE: f32 = 0.05;
// A move must point mostly INTO the ladder face to start a climb: its
// into-face component must be at least this fraction of the whole horizontal
// move (0.5 ≈ within 60° of straight-in). Keeps a grazing walk past a ladder
// from lifting off.
pub const LADDER_CLIMB_FACING_FRACTION: f32 = 0.5;
// ...and carry at least this much speed into the face (m/s), so micro drift
// (knockback tails, carrier jitter) never reads as climbing.
pub const LADDER_CLIMB_MIN_SPEED: f32 = 1.0;
// While ascending or descending, a character gets an additive pull along
// the ladder face toward its center axis; idle latch does not pull. Pull
// speed is lateral offset in meters × `GAIN`. Player movement is applied
// alongside the pull, allowing enough sideways input to overpower it.
pub const LADDER_FUNNEL_GAIN: f32 = 4.0;

// Y tolerance for mapping a world position to a discrete map level
// (`MapGeometryConfig::level_for_y`). This keeps brief jumps and small
// vertical wobble from changing render/filter level.
pub const LEVEL_CLASSIFICATION_TOLERANCE: f32 = 0.5;

// ============================================================================
// Game Tick
// ============================================================================

// Default simulation timing; runtime rates come from NetworkConfig.
pub const TICK_HZ: u32 = 30;
pub const TICK_SECS: f32 = 1.0 / TICK_HZ as f32;
pub const TICK_DURATION: Duration = Duration::from_nanos(1_000_000_000 / TICK_HZ as u64);

// ============================================================================
// Characters
// ============================================================================

// Characters whose Y falls below this are killed (players run through the
// normal death/respawn flow; actors are despawned outright).
pub const CHARACTER_FALL_DEATH_Y: f32 = -25.0;

// Hard cap on a falling character's downward speed. Prevents arbitrarily large
// velocities from very tall drops.
pub const CHARACTER_TERMINAL_VELOCITY: f32 = 25.0; // m/s

// How far the Rapier character controller may snap downward to stay attached to
// valid ground while walking over seams, ramps, and small frame-step gaps.
pub const CHARACTER_GROUND_SNAP_DISTANCE: f32 = 0.2;

// How far above a carrier's surface a body still rides it: ground snap
// leaves the feet slightly above the surface they ride.
pub const CHARACTER_CARRIER_RIDE_TOLERANCE: f32 = 0.05;

// Coincident static and carried surfaces must tolerate shape-cast depth noise.
pub const CHARACTER_CARRIER_TIE_EPSILON: f32 = 0.01;

// Gap the Rapier character controller keeps between a character and the
// geometry it touches; a body closer than this to a carrier is inside it.
pub const CHARACTER_CONTACT_OFFSET: f32 = 0.01;

// Maximum low ledge height the Rapier character controller may auto-step over.
pub const CHARACTER_STEP_HEIGHT: f32 = 0.2;

// Minimum forward clearance Rapier requires after an auto-step. This must be
// large enough to carry the character past thin slab/trim edges, not just onto
// the edge contact itself.
pub const CHARACTER_STEP_MIN_WIDTH: f32 = 0.2;

pub const CHARACTER_MAX_SLOPE: f32 = std::f32::consts::FRAC_PI_4;

// Overlapping blasts may stack up to this multiple of one blast's `knockback.max_speed`.
pub const KNOCKBACK_CLAMP_RATIO: f32 = 1.5;

// ============================================================================
// Explosions
// ============================================================================

// Inside this fraction of the blast radius the blast is at full strength;
// past it, strength falls off quadratically to zero at the rim (closer to
// real overpressure decay than a straight lerp — point blank is decisively
// worse than a rim graze).
pub const EXPLOSION_BLAST_CORE_FRACTION: f32 = 0.25;

// ============================================================================
// Portals
// ============================================================================

// Aperture half-extents along the frame's right/up axes — one oval for every
// surface orientation, sized to pass the player body whole.
pub const PORTAL_HALF_WIDTH: f32 = 0.7;
pub const PORTAL_HALF_HEIGHT: f32 = 1.3;
// Placement clearance follows the rendered rim so world geometry cannot
// visibly clip it at aperture edges.
pub const PORTAL_RIM_SCALE: f32 = 1.06;
// Normals pointing up at least this much are standable (a surface you can
// rest on); pressure-plate keep-outs apply only to portals on them.
pub const PORTAL_STANDABLE_NORMAL_Y: f32 = 0.5;
// Portal funneling (Portal-2 style): a body flying toward a vertical-normal
// aperture without steering is pulled toward its axis, so hand-placed
// floor/ceiling pairs loop indefinitely despite imperfect alignment. The
// pull ramps with the lateral offset (`GAIN` per second, capped at
// `MAX_SPEED`), engages within `CAPTURE_MARGIN` beyond the aperture rect
// once the approach exceeds `MIN_APPROACH`, and any deliberate lateral
// control above `RELEASE_SPEED` disengages it — escaping a fall chain
// stays exactly as easy as before.
pub const PORTAL_FUNNEL_GAIN: f32 = 6.0;
pub const PORTAL_FUNNEL_MAX_SPEED: f32 = 6.0;
pub const PORTAL_FUNNEL_CAPTURE_MARGIN: f32 = 0.6;
pub const PORTAL_FUNNEL_MIN_APPROACH: f32 = 2.0;
pub const PORTAL_FUNNEL_RELEASE_SPEED: f32 = 0.5;
// Fixture keep-outs the aperture must respect: margin around a wall light,
// and around a pressure plate's center (plates constrain only standable
// portals — they live on floors).
pub const PORTAL_LIGHT_CLEARANCE: f32 = 0.4;
pub const PORTAL_PLATE_CLEARANCE: f32 = 1.2;
// A fixture farther than this from the aperture plane cannot overlap it.
pub const PORTAL_FIXTURE_PLANE_DEPTH: f32 = 0.5;
// Blast knockback rotated through a portal keeps the explosion speed cap.
pub const PORTAL_KNOCKBACK_CARRY_FACTOR: f32 = 1.5;

// ============================================================================
// Console
// ============================================================================

// Character caps on a console line, applied by the client while typing and
// by the server on receipt. Commands get more room than chat: `/light`
// alone takes three arguments.
pub const CONSOLE_CHAT_MAX_CHARS: usize = 128;
pub const CONSOLE_COMMAND_MAX_CHARS: usize = 256;

// ============================================================================
// Fireworks
// ============================================================================

// How long the client's seeded firework show runs, from the launch cue to
// its last cue (`client/src/vfx/firework.rs::build_show`, whose finale pops
// land about 30 s in plus a rocket's flight). The server does not play the
// show; it spaces a fireworks switch's repeats by this plus the map's
// cooldown.
pub const FIREWORK_SHOW_SECS: f32 = 45.0;
