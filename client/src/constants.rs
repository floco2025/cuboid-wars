use bevy::color::Color;

// ============================================================================
// Input
// ============================================================================

// Player input is sampled at render rate (smooth camera) and committed once
// per game tick (`common::constants::TICK_HZ`) in `commit_player_input_system`.
// The tick rate alone gates send frequency; this threshold filters mouse-
// sensor / hand jitter below a meaningful direction change.
//
// 1° at 10 m is ~17 cm of visual offset — well below human aim resolution.
// State transitions (idle ↔ moving, walk ↔ run, jump) bypass the threshold
// and always commit.
pub const ANGLE_COMMIT_THRESHOLD_DEGREES: f32 = 1.0;

// ============================================================================
// RTT measurement
// ============================================================================

// Round-trip time — interval between ping requests sent to the server.
pub const PING_INTERVAL: f32 = 1.0;

// ============================================================================
// Server Reconciliation
// ============================================================================
//
// Server snapshots blend into the client's predicted position over a
// per-tick correction window. If the gap is too big to smooth, the client
// snaps to the server pos instead — large divergence usually means a
// teleport or a desync that won't close from gradual correction.

// --- Shared (players + actors) ---

// Correction-window length scales with RTT so smoothing stays proportional
// to typical drift size.
pub const RECON_CORRECTION_TIME_RTT_MULTIPLIER: f32 = 4.0;

// --- Player only ---

// Per-axis snap distance, lerped by a high-water-mark "recently running"
// speed that decays over `RECON_PLAYER_SNAP_DECAY_SECS` after a stop. The
// decay keeps the threshold from tightening abruptly on stop and tripping
// the snap branch on drift that's still being smoothed out.
pub const RECON_PLAYER_SNAP_DISTANCE_IDLE: f32 = 1.0;
pub const RECON_PLAYER_SNAP_DISTANCE_RUNNING: f32 = 5.0;
pub const RECON_PLAYER_SNAP_DECAY_SECS: f32 = 1.0;

// Idle endpoint of the correction-window lerp (running endpoint is
// `rtt * RECON_CORRECTION_TIME_RTT_MULTIPLIER`). Stationary players see
// corrections more clearly than moving ones, so smooth them slowly.
pub const RECON_PLAYER_IDLE_CORRECTION_SECS: f32 = 8.0;

// --- Actor only ---

// Per-axis snap distance. Fixed — actor speeds are simple enough that
// lerping doesn't earn the complexity.
pub const RECON_ACTOR_SNAP_DISTANCE: f32 = 3.0;

// ============================================================================
// Character Visuals
// ============================================================================

// Max angular speed (rad/s) at which the rendered character yaw slews toward
// the gameplay `FaceDirection`. The cap is what makes the visual robust: no
// matter how fast the server changes facing, the model turns at most this much
// per second and can never spin. ~12 rad/s ⇒ a 180° turn takes ~0.26 s.
pub const CHARACTER_VISUAL_TURN_MAX_SPEED: f32 = 12.0;

// ============================================================================
// Floating Labels (layout math; sizes live in `client.json::hud`)
// ============================================================================

// Player floating label: name text above a health bar; needs the larger
// texture for legible text. The mesh aspect ratio is driven by the texture
// dimensions, so these stay paired as constants.
pub const LABEL_PLAYER_MESH_WIDTH: f32 = 1.0;
pub const LABEL_PLAYER_TEXTURE_WIDTH: u32 = 256;
pub const LABEL_PLAYER_TEXTURE_HEIGHT: u32 = 64;
// Player health bar geometry below the name label. Width matches the name mesh
// today but is tracked separately so either can change alone.
pub const LABEL_PLAYER_BAR_WIDTH: f32 = 1.0;
// Vertical gap (m) between the health bar's top edge and the name label.
pub const LABEL_PLAYER_NAME_GAP: f32 = 0.03;
// Actor floating label: just the health bar, no text. Texture matches the
// visible bar 1:1 (no padding) — texture size is derived at the spawn
// call site from the runtime health-bar dims, not stored here.
pub const LABEL_ACTOR_MESH_WIDTH: f32 = 0.85;
// Frames a label's render-target camera stays active after a health change.
// A multi-frame window (not a single frame) so the render reliably lands —
// a one-frame-only activation intermittently failed to redraw the bar.
pub const LABEL_RENDER_FRAMES: u8 = 3;
pub const LABEL_TEXT_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 1.0);
pub const LABEL_BACKGROUND_COLOR: Color = Color::srgba(0.0, 0.0, 0.0, 0.2);
// Padding inside the name label's translucent background, in texture px.
pub const LABEL_TEXT_PADDING_X: f32 = 12.0;
pub const LABEL_TEXT_PADDING_Y: f32 = 2.0;

// ============================================================================
// HUD
// ============================================================================

// Gap (logical px) between a corner-anchored HUD element and the window edge.
// Shared by the player list, quest panel, and rear-view mirror so they all
// sit the same distance in from the sides. The mirror is a render viewport in
// physical pixels, so it multiplies this by the window scale factor.
pub const HUD_EDGE_MARGIN_PX: f32 = 10.0;
// Bottom strip reserved for the admin console's input line; the message
// feed starts above it so the two never overlap (classic chat layout:
// input at the very bottom, messages stacking upward above it).
pub const CONSOLE_LINE_RESERVED_PX: f32 = 30.0;

// Safety floor for the HUD scale (window width / `hud.reference_width`) so a
// tiny window can't collapse the UI toward zero.
pub const HUD_MIN_SCALE: f32 = 0.5;

// ============================================================================
// Health Bars (colors + fill layering; pixel dimensions live in
// `client.json::hud.health_bars`)
// ============================================================================

pub const HEALTH_BAR_TRACK_COLOR: Color = Color::srgba(0.0, 0.0, 0.0, 0.65);
pub const HEALTH_BAR_FILL_COLOR: Color = Color::srgb(0.0, 0.85, 0.2);
// Local +Z of a world-space bar's opaque fill quad over its translucent track,
// so the fill always layers in front.
pub const HEALTH_BAR_FILL_Z_OFFSET: f32 = 0.005;

// ============================================================================
// Quest Panel (colors only — dimensions live in `client.json::hud.quest_panel`)
// ============================================================================

pub const QUEST_BAR_TRACK_COLOR: Color = Color::srgba(0.0, 0.0, 0.0, 0.65);
pub const QUEST_BAR_FILL_COLOR: Color = Color::srgb(0.85, 0.7, 0.0); // gold, in progress
pub const QUEST_BAR_COMPLETE_COLOR: Color = Color::srgb(0.0, 0.85, 0.2); // green, done

// ============================================================================
// Power-Up Items
// ============================================================================

pub const ITEM_SIZE: f32 = 0.3;
pub const ITEM_HEIGHT_ABOVE_FLOOR: f32 = 1.1;
pub const ITEM_ANIMATION_HEIGHT: f32 = 0.2; // bob amplitude (m); peak-to-peak swing is 2×
pub const ITEM_ANIMATION_SPEED: f32 = 0.8;
pub const ITEM_SPIN_HZ: f32 = 0.4; // slow coin-spin around Y for keys + power-ups
pub const ITEM_EMISSIVE_STRENGTH: f32 = 0.2; // Multiplier for emissive glow
// The map editor mirrors these colors (tools/map_editor/constants.py
// `ITEM_TYPE_COLORS`) — keep the two in sync.
pub const ITEM_SPEED_COLOR: Color = Color::srgb(1.00, 0.85, 0.15); // Yellow (lightning)
pub const ITEM_MULTISHOT_COLOR: Color = Color::srgb(1.00, 0.25, 0.25); // Red
pub const ITEM_PHASING_COLOR: Color = Color::srgb(0.2, 1.0, 0.2); // Green — unchanged; phasing is dormant
pub const ITEM_LOW_GRAVITY_COLOR: Color = Color::srgb(0.30, 0.85, 1.00); // Cyan
pub const ITEM_HEALTH_COLOR: Color = Color::srgb(0.20, 0.95, 0.30); // Green (heal / potion)

// ============================================================================
// Cookies
// ============================================================================

pub const COOKIE_SIZE: f32 = 0.15; // sphere radius
pub const COOKIE_HEIGHT: f32 = 0.16; // above floor

// ============================================================================
// Keys
// ============================================================================

// Visual: a small rotating cuboid that reuses the matching barrier material
// (translucent, pulsating). Width = height = ~face plate; depth = thin slab.
pub const KEY_WIDTH: f32 = 0.8;
pub const KEY_HEIGHT: f32 = 0.8;
pub const KEY_DEPTH: f32 = 0.1;
pub const KEY_HEIGHT_ABOVE_FLOOR: f32 = 0.6;

// HUD: thin vertical bars next to the player name, after the power-up icons.
// Shape (narrow vertical) is intentionally different from the 12×12 power-up
// squares so the two categories read as different.
pub const KEY_HUD_ICON_WIDTH_PX: f32 = 4.0;
pub const KEY_HUD_ICON_HEIGHT_PX: f32 = 12.0;
pub const KEY_HUD_GAP_PX: f32 = 8.0;

// ============================================================================
// Projectile VFX
// ============================================================================

// Configured spark count and speed are calibrated at this impact speed.
pub const PROJECTILE_SPARK_REFERENCE_SPEED: f32 = 70.0;
pub const PROJECTILE_SPARK_GRAVITY: f32 = 20.0;
pub const PROJECTILE_SPARK_SPREAD_DEGREES: f32 = 45.0;

// ============================================================================
// Actor Beam-In VFX
// ============================================================================

pub const BEAM_IN_COLOR: Color = Color::srgb(1.0, 0.85, 0.3);
// Keeps the minimum emission rate proportional when configured density changes.
pub const BEAM_IN_REFERENCE_SPARKLES_PER_M3_PER_SECOND: f32 = 200.0;
pub const BEAM_IN_MIN_SPARKLES_PER_SECOND: f32 = 20.0;
pub const BEAM_IN_MAX_SPARKLES_PER_FRAME: usize = 32;
pub const BEAM_IN_SPARKLE_RISE_SPEED: f32 = 1.0;
pub const BEAM_IN_SPARKLE_DRIFT_SPEED: f32 = 0.25;
pub const BEAM_IN_MATERIALIZATION_PARTICLE_COUNT: usize = 32;
pub const BEAM_IN_MATERIALIZATION_SPEED: f32 = 3.5;
pub const BEAM_IN_MATERIALIZATION_LIFETIME_SECS: f32 = 0.55;
pub const BEAM_IN_LIGHT_MIN_INTENSITY: f32 = 5_000.0;
pub const BEAM_IN_LIGHT_RANGE: f32 = 8.0;

// ============================================================================
// Explosion VFX
// ============================================================================

pub const EXPLOSION_FALLBACK_FIREBALL_DIAMETER: f32 = 6.0;
pub const EXPLOSION_FIREBALL_LIFETIME_FACTOR: f32 = 0.6;
pub const EXPLOSION_FIREBALL_START_ALPHA: f32 = 0.9;

pub const EXPLOSION_SHOCKWAVE_SURFACE_OFFSET: f32 = 0.05;
pub const EXPLOSION_SHOCKWAVE_DIAMETER_FACTOR: f32 = 1.0;
pub const EXPLOSION_SHOCKWAVE_LIFETIME_FACTOR: f32 = 0.7;
pub const EXPLOSION_SHOCKWAVE_THICKNESS_RATIO: f32 = 0.16;
pub const EXPLOSION_SHOCKWAVE_START_ALPHA: f32 = 0.7;

pub const EXPLOSION_SCORCH_SURFACE_OFFSET: f32 = 0.015;
pub const EXPLOSION_SCORCH_FADE_FRACTION: f32 = 1.0 / 3.0;
pub const EXPLOSION_SCORCH_MAX_ACTIVE: usize = 128;
pub const EXPLOSION_SCORCH_MESH_VARIANT_COUNT: usize = 12;
// Stay inside the irregular floor outline so wall and floor coverage agree at corners.
pub const EXPLOSION_SCORCH_WALL_REACH_FACTOR: f32 = 0.6;
pub const EXPLOSION_SCORCH_RING_RADII: [f32; 3] = [0.22, 0.39, 0.5];
pub const EXPLOSION_SCORCH_RING_ALPHA: [f32; 3] = [0.84, 0.60, 0.0];
pub const EXPLOSION_SCORCH_WALL_SEAM_OVERSCAN_FACTOR: f32 = 0.35;

pub const EXPLOSION_GRASS_BURN_VERTICAL_TOLERANCE: f32 = 0.1;
pub const EXPLOSION_GRASS_BURN_CORE_RADIUS_FACTOR: f32 = 0.5;
pub const EXPLOSION_GRASS_BURN_CENTER_HEIGHT_FACTOR: f32 = 0.6;
pub const EXPLOSION_GRASS_BURN_CENTER_WIDTH_FACTOR: f32 = 0.55;
pub const EXPLOSION_GRASS_BURN_CENTER_SWAY_FACTOR: f32 = 0.4;
pub const EXPLOSION_GRASS_BURN_MAX_COLOR_BLEND: f32 = 0.97;
pub const EXPLOSION_GRASS_BURN_COLOR: Color = Color::srgb(0.18, 0.17, 0.16);
pub const EXPLOSION_GRASS_BURN_ROOT_BRIGHTNESS_FACTOR: f32 = 0.7;
pub const EXPLOSION_GRASS_BURN_MID_BRIGHTNESS_FACTOR: f32 = 1.0;
pub const EXPLOSION_GRASS_BURN_TIP_BRIGHTNESS_FACTOR: f32 = 1.35;
pub const EXPLOSION_GRASS_BURN_FADE_STEPS: u32 = 60;

// Particle count limits scale around the densities shipped in client.json.
pub const EXPLOSION_REFERENCE_SHARDS_PER_METER: f32 = 40.0;
pub const EXPLOSION_SHARD_MIN_COUNT: usize = 200;
pub const EXPLOSION_SHARD_MAX_COUNT: usize = 600;
pub const EXPLOSION_SHARD_GLOBAL_MAX_COUNT: usize = 1_200;
pub const EXPLOSION_SHARD_LIFETIME_FACTOR: f32 = 1.3;
pub const EXPLOSION_SHARD_SPEED_FACTOR: f32 = 2.0;
pub const EXPLOSION_SHARD_GRAVITY: f32 = 15.0;
pub const EXPLOSION_SHARD_UP_BIAS: f32 = 0.35;
pub const EXPLOSION_SHARD_BOUNCE_DAMPING: f32 = 0.4;
pub const EXPLOSION_SHARD_FRICTION: f32 = 0.7;

pub const EXPLOSION_REFERENCE_SMOKE_PARTICLES_PER_METER: f32 = 1.5;
pub const EXPLOSION_SMOKE_MIN_COUNT: usize = 10;
pub const EXPLOSION_SMOKE_MAX_COUNT: usize = 30;
pub const EXPLOSION_SMOKE_GLOBAL_MAX_COUNT: usize = 160;
pub const EXPLOSION_SMOKE_START_SIZE: f32 = 0.42;
pub const EXPLOSION_SMOKE_FADE_IN_SECS: f32 = 0.5;
pub const EXPLOSION_SMOKE_FADE_OUT_START_FRACTION: f32 = 0.58;

pub const EXPLOSION_LIGHT_COLOR: Color = Color::srgb(1.0, 0.55, 0.2);
pub const EXPLOSION_LIGHT_RANGE_PER_RADIUS: f32 = 1.5;
pub const EXPLOSION_LIGHT_MIN_RANGE: f32 = 7.0;
pub const EXPLOSION_LIGHT_MAX_ACTIVE: usize = 4;

// ============================================================================
// Wall Light Flicker
// ============================================================================

// Two incommensurate sines; their product exceeds the threshold only in
// short, irregular windows — mostly-steady lights with occasional dips.
pub const WALL_LIGHT_FLICKER_HZ_A: f32 = 5.0;
pub const WALL_LIGHT_FLICKER_HZ_B: f32 = 1.3;
pub const WALL_LIGHT_FLICKER_THRESHOLD: f32 = 0.9;
pub const WALL_LIGHT_FLICKER_DEPTH: f32 = 0.65; // max fraction of brightness lost in a dip
