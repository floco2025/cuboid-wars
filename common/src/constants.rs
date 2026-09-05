// ============================================================================
// Map Geometry
// ============================================================================

// The grid cell size, storey height, and floor and wall thicknesses are per
// map (`MapGeometryConfig` in the map settings); every size that follows them
// is a fraction here, computed there.

// Barriers. Force-field segments authored on grid edges; same shape as walls
// (wall height, this fraction of the wall thickness) but rendered as
// translucent pulsating geometry on the client. Each kind gets its own
// collision group (`barrier_collision_group`) so held keys and open pressure
// plates gate pass-through per color (`passable_barrier_kinds`).
pub const BARRIER_THICKNESS_FRACTION: f32 = 1.0 / 6.0;

// Slab thickness of a light bridge, as a fraction of the floor thickness.
pub const BRIDGE_THICKNESS_FRACTION: f32 = 0.25;

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
// Gap between a character's leading face and the rail plane while held at
// the ladder. Face-based (half extent along the plane normal + this), not
// center-based: the collider is wider than it is deep.
pub const LADDER_STANDOFF_CLEARANCE: f32 = 0.05;
// How far the blocking band reaches from the plane, on both sides.
// Generous enough to cover any character's hold distance, so the fence
// clamp can't oscillate at the band's outer boundary.
pub const LADDER_BAND_DEPTH: f32 = 1.0;
// A move must point mostly INTO the ladder face to start a climb: its
// into-face component must be at least this fraction of the whole horizontal
// move (0.5 ≈ within 60° of straight-in). Keeps a grazing walk past a ladder
// from lifting off.
pub const LADDER_CLIMB_FACING_FRACTION: f32 = 0.5;
// ...and carry at least this much speed into the face (m/s), so micro drift
// (reconciliation nudges, knockback tails) never reads as climbing.
pub const LADDER_CLIMB_MIN_SPEED: f32 = 1.0;
// While ascending or descending, a character gets an additive pull along
// the ladder face toward its center axis; idle latch does not pull. Pull
// speed is lateral offset in meters × `GAIN`. Player movement is applied
// alongside the pull, allowing enough sideways input to overpower it.
pub const LADDER_FUNNEL_GAIN: f32 = 4.0;

// Y tolerance for mapping a world position to a discrete map level
// (`MapGeometryConfig::level_for_y`). This keeps brief jumps and small
// vertical prediction differences from changing render/filter level.
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
pub const CHARACTER_TERMINAL_VELOCITY: f32 = 25.0; // m/s

// How far the Rapier character controller may snap downward to stay attached to
// valid ground while walking over seams, ramps, and small frame-step gaps.
pub const CHARACTER_GROUND_SNAP_DISTANCE: f32 = 0.5;

// Inside this fraction of the blast radius the blast is at full strength;
// past it, strength falls off quadratically to zero at the rim (closer to
// real overpressure decay than a straight lerp — point blank is decisively
// worse than a rim graze).
pub const EXPLOSION_BLAST_CORE_FRACTION: f32 = 0.25;

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
// Missiles
// ============================================================================

// Fixed missile geometry. The server sweeps this ball for collision, the
// proximity fuse, and launch clearance. The client renders a separately
// tuned, smaller mesh (`client::constants::MISSILE_BODY_RADIUS`) — feel and
// looks are deliberately independent — but its widest radial extent must
// fit inside this ball or missiles visibly clip walls they fly along; a
// client test (`rendered_missile_fits_inside_the_collision_ball`) pins
// that. Flight/guidance tuning stays in `config/server/gameplay.json`.
pub const MISSILE_RADIUS: f32 = 0.3;
// Launch distance in front of the shooter's eye along the aim.
pub const MISSILE_SPAWN_OFFSET: f32 = 1.0;

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
pub const PORTAL_PROJECTILE_EXIT_STANDOFF: f32 = 0.02;
// Above this |normal.y| world-up has no usable in-plane projection and the
// placement yaw orients the aperture frame instead.
pub const PORTAL_UP_DEGENERACY_LIMIT: f32 = 0.99;
// How far a collider may poke past a portal's surface plane and still count
// as flush backing; coplanar grid faces differ by float noise only.
pub const PORTAL_BACKING_FLUSH_EPSILON: f32 = 0.02;
// A projectile's portal crossing and its bounce off the portal's own surface
// register at nearly the same time of impact; within this fraction-of-tick
// tie the portal wins.
pub const PORTAL_SURFACE_TIE_EPSILON: f32 = 0.01;
// Bounds chained bounces and portal hops inside one projectile tick. When the
// budget is exhausted, the projectile stays at its last validated position.
pub const PROJECTILE_EVENT_LIMIT: usize = 8;
// Blast knockback rotated through a portal keeps the explosion speed cap.
pub const PORTAL_KNOCKBACK_CARRY_FACTOR: f32 = 1.5;

// ============================================================================
// Console
// ============================================================================

// Character caps on a console line, applied by the client while typing and
// by the server on receipt. Commands get more room than chat: `/light`
// alone takes three arguments.
pub const CHAT_MAX_CHARS: usize = 128;
pub const COMMAND_MAX_CHARS: usize = 256;
