use bevy::color::Color;

// ============================================================================
// Input
// ============================================================================

// Player input is sampled at render rate (smooth camera) and committed once
// per game tick (`common::constants::TICK_HZ`) in `commit_player_input_system`,
// changed or not.

// Slots in the local player's ring of committed positions, one per tick;
// 64 is two seconds, more than any round trip worth playing over.
pub const COMMITTED_POSITION_RING_LEN: usize = 64;

// ============================================================================
// RTT measurement
// ============================================================================

// Round-trip time — interval between ping requests sent to the server.
pub const PING_INTERVAL: f32 = 1.0;

// ============================================================================
// Server tick
// ============================================================================

// Consecutive echoes that must all report a clock error in the same
// direction before `TickSync` shifts the clock; half a second outlasts any
// delivery jitter.
pub const TICK_SYNC_WINDOW_TICKS: usize = 15;

// ============================================================================
// Server Reconciliation
// ============================================================================
//
// Server samples — snapshots, and for players the per-tick move stream —
// blend into the client's predicted position over a correction window. If
// the gap is too big to smooth, the client snaps to the server pos instead —
// large divergence usually means a teleport or a desync that won't close
// from gradual correction.

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

// A portal crossing the client has predicted shows up in the server's
// states about one-way latency later. One still missing a round trip plus
// this after it was mispredicted, and the server's side stands
// (`PlayerInfo::disputed_echoes`).
pub const HOP_DISPUTE_SLACK_SECS: f32 = 0.3;

// --- Actor only ---

// Per-axis snap distance. Fixed — actor speeds are simple enough that
// lerping doesn't earn the complexity.
pub const RECON_ACTOR_SNAP_DISTANCE: f32 = 3.0;

// Missile course changes are broadcast promptly, so clients barely drift.
// Actors need the larger threshold because they can reverse direction instantly
// between updates.
pub const RECON_MISSILE_SNAP_DISTANCE: f32 = 1.5;

// ============================================================================
// Character Visuals
// ============================================================================

// Max angular speed (rad/s) at which the rendered character yaw slews toward
// the gameplay `FaceYaw`. The cap is what makes the visual robust: no
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
// Vertical gap between stacked rows in every HUD column.
pub const HUD_ROW_GAP_PX: f32 = 4.0;
// Every timed HUD line (banner and feed) fades out over its final seconds.
pub const HUD_LINE_FADE_SECS: f32 = 0.8;

// Safety floor for the HUD scale (window width / `hud.reference_width`) so a
// tiny window can't collapse the UI toward zero.
pub const HUD_MIN_SCALE: f32 = 0.5;

// ============================================================================
// Crosshair
// ============================================================================

pub const CROSSHAIR_SIZE_PX: f32 = 30.0;
pub const CROSSHAIR_THICKNESS_PX: f32 = 2.0;
pub const CROSSHAIR_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 0.8);
// Lock-on tint: lit crosshair = a missile fired now will track this target.
pub const CROSSHAIR_LOCK_COLOR: Color = Color::srgba(1.0, 0.25, 0.2, 0.9);
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
pub const QUEST_NOTE_COLOR: Color = Color::srgba(0.75, 0.75, 0.75, 1.0); // scope line under the bar
pub const QUEST_ENTRY_BG_COLOR: Color = Color::srgba(0.0, 0.0, 0.0, 0.35); // card behind each quest
// Scope line relative to the quest font.
pub const QUEST_NOTE_FONT_SCALE: f32 = 0.8;

// ============================================================================
// Message Feed & Console (colors only — durations live in
// `client.json::hud.message_feed`)
// ============================================================================

pub const FEED_TEXT_COLOR: Color = Color::srgba(0.85, 0.85, 0.85, 1.0);
pub const FEED_DIM_TEXT_COLOR: Color = Color::srgba(0.6, 0.6, 0.6, 1.0);
// Full white so player speech stands apart from system lines.
pub const FEED_CHAT_TEXT_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 1.0);
// A command in the console prompt and the admin lines it produces share one
// color so they read as one unit (chat in the prompt uses the chat color).
pub const CONSOLE_TEXT_COLOR: Color = Color::srgba(1.0, 0.85, 0.4, 1.0);

// ============================================================================
// Power-Up Items
// ============================================================================

pub const ITEM_SIZE: f32 = 0.3;
pub const ITEM_HEIGHT_ABOVE_FLOOR: f32 = 1.1;
pub const ITEM_ANIMATION_HEIGHT: f32 = 0.2; // bob amplitude (m); peak-to-peak swing is 2×
pub const ITEM_ANIMATION_SPEED: f32 = 0.8;
pub const ITEM_SPIN_HZ: f32 = 0.4; // slow coin-spin around Y for keys + power-ups
// The map editor mirrors these colors (tools/map_editor/constants.py
// `ITEM_TYPE_COLORS`) — keep the two in sync.
pub const ITEM_SPEED_COLOR: Color = Color::srgb(1.00, 0.85, 0.15); // Yellow (lightning)
pub const ITEM_MULTISHOT_COLOR: Color = Color::srgb(1.00, 0.25, 0.25); // Red
pub const ITEM_LOW_GRAVITY_COLOR: Color = Color::srgb(0.30, 0.85, 1.00); // Cyan
pub const ITEM_HEALTH_COLOR: Color = Color::srgb(0.20, 0.95, 0.30); // Green (heal / potion)
pub const ITEM_MISSILE_COLOR: Color = Color::srgb(0.95, 0.45, 0.10); // Orange (missile pack)

// Missile mesh dimensions (m): Y-up cylinder body, cone nose, 4 fins at the tail.
pub const MISSILE_BODY_RADIUS: f32 = 0.08;
pub const MISSILE_BODY_LENGTH: f32 = 0.5;
pub const MISSILE_NOSE_LENGTH: f32 = 0.2;
pub const MISSILE_FIN_SPAN: f32 = 0.14; // outward reach beyond the body surface
pub const MISSILE_FIN_LENGTH: f32 = 0.18; // along the body axis

// ============================================================================
// Portals
// ============================================================================
// Looks only — the aperture size and traversal geometry are shared constants
// (`common::constants` portal block), and the oval is drawn at exactly the
// aperture extents so the visual IS the trigger area.

pub const PORTAL_A_COLOR: Color = Color::srgb(0.20, 0.55, 1.00); // blue — end A (left click)
pub const PORTAL_B_COLOR: Color = Color::srgb(1.00, 0.55, 0.10); // orange — end B (right click)
pub const PORTAL_EMISSIVE: f32 = 8.0;
// Offset off the surface so the decal wins the depth test against its wall.
pub const PORTAL_SURFACE_OFFSET: f32 = 0.01;
pub const PORTAL_RIM_OFFSET: f32 = 0.002;
// Near plane sits this far past the exit plane, clipping the exit's own surface.
pub const PORTAL_VIEW_CLIP_OFFSET: f32 = 0.02;
// A view stays valid until the eye is this close to the entry plane, so the
// last frame before a crossing still sees through.
pub const PORTAL_VIEW_MIN_EYE_DISTANCE: f32 = 0.001;
// Rendered only by the main camera: the shared portal surfaces and its sky disc.
pub const MAIN_VIEW_RENDER_LAYER: usize = 1;
pub const LOCAL_PLAYER_RENDER_LAYER: usize = 2;
// Rendered only by the rearview mirror: its portal replicas and sky disc.
pub const REARVIEW_RENDER_LAYER: usize = 3;
// Camera-facing labels only make sense from the main view that orients them.
pub const CHARACTER_LABEL_RENDER_LAYER: usize = 4;
// Portal-style exit reorientation: the camera is seeded with the fully
// mapped (possibly tilted) view and blended back to the upright aim over
// this window.
pub const PORTAL_VIEW_BLEND_SECS: f32 = 0.25;

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
pub const KEY_HUD_ICON_SIZE_PX: f32 = 10.0;
// Icon strip spacing: tight within a category; the power-up / missile / key
// groups spread across the entry, and this is the least they get when packed.
pub const HUD_ICON_GAP_PX: f32 = 3.0;
pub const HUD_ICON_CATEGORY_GAP_PX: f32 = 6.0;
pub const MISSILE_HUD_ICON_WIDTH_PX: f32 = 3.0;
pub const MISSILE_HUD_ICON_HEIGHT_PX: f32 = 12.0;
// Unfilled slot in the player-list strips (power-up not active, key not
// held, missile bay empty). Every slot always renders so the row width
// never changes on pickup.
pub const HUD_SLOT_EMPTY_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 0.12);

// ============================================================================
// Projectile VFX
// ============================================================================

// Configured spark count and speed are calibrated at this impact speed.
// Glow of the projectile body itself.
pub const PROJECTILE_BODY_EMISSIVE: f32 = 5.0;
// Impact sparks: base burst size (scaled by impact speed), particle size,
// launch speed, lifetime, and brightness.
pub const PROJECTILE_SPARK_BASE_COUNT: usize = 6;
pub const PROJECTILE_SPARK_SIZE: f32 = 0.05;
pub const PROJECTILE_SPARK_SPEED: f32 = 8.0;
pub const PROJECTILE_SPARK_LIFETIME_SECS: f32 = 0.25;
pub const PROJECTILE_SPARK_EMISSIVE: f32 = 25.0;
pub const PROJECTILE_SPARK_REFERENCE_SPEED: f32 = 70.0;
pub const PROJECTILE_SPARK_GRAVITY: f32 = 20.0;
pub const PROJECTILE_SPARK_SPREAD_DEGREES: f32 = 45.0;

// ============================================================================
// Actor Beam-In VFX
// ============================================================================

// Sparkle density/size/lifetime/brightness of the beam-in volume, and the
// glow light's intensity per cubic meter of that volume.
pub const BEAM_IN_SPARKLES_PER_M3_PER_SECOND: f32 = 200.0;
pub const BEAM_IN_SPARKLE_SIZE: f32 = 0.05;
pub const BEAM_IN_SPARKLE_LIFETIME_SECS: f32 = 1.5;
pub const BEAM_IN_SPARKLE_EMISSIVE: f32 = 25.0;
pub const BEAM_IN_LIGHT_INTENSITY_LUMENS_PER_M3: f32 = 500_000.0;
// The one-shot materialization ring at beam-in completion; currently off.
pub const BEAM_IN_MATERIALIZATION_RING_ENABLED: bool = false;
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

pub const EXPLOSION_BASE_DURATION_SECS: f32 = 0.8;
pub const EXPLOSION_FIREBALL_BLAST_DIAMETER_FACTOR: f32 = 0.5;
pub const EXPLOSION_FIREBALL_EMISSIVE: f32 = 3000.0;
pub const EXPLOSION_SHOCKWAVE_EMISSIVE: f32 = 2000.0;
pub const EXPLOSION_LIGHT_INTENSITY_LUMENS: f32 = 1_000_000.0;
pub const EXPLOSION_SHARDS_PER_RADIUS_METER: f32 = 40.0;
pub const EXPLOSION_SHARD_SIZE: f32 = 0.2;
pub const EXPLOSION_SHARD_EMISSIVE: f32 = 2500.0;
pub const EXPLOSION_SMOKE_PER_RADIUS_METER: f32 = 1.5;
pub const EXPLOSION_SMOKE_END_SIZE: f32 = 1.45;
pub const EXPLOSION_SMOKE_LIFETIME_SECS: f32 = 4.0;
pub const EXPLOSION_SMOKE_MAX_OPACITY: f32 = 0.32;
pub const EXPLOSION_SCORCH_BLAST_DIAMETER_FACTOR: f32 = 0.4;
pub const EXPLOSION_SCORCH_FULL_OPACITY_SECS: f32 = 30.0;
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

// ============================================================================
// Barriers
// ============================================================================

// Pulse alpha swings between min and max at the pulse rate. Below ~0.1 the
// barrier almost disappears (good off-phase look); above ~0.7 it reads as
// solid.
pub const BARRIER_ALPHA_MIN: f32 = 0.007;
pub const BARRIER_ALPHA_MAX: f32 = 0.015;
pub const BARRIER_PULSE_HZ: f32 = 0.5;
// Constant emissive brightness multiplier on the kind color — set once on
// the material, never pulsed. Translucency still attenuates what the surface
// contributes, so useful values are well above the bloom threshold.
pub const BARRIER_EMISSIVE: f32 = 2000.0;

// ============================================================================
// Light bridges
// ============================================================================

// Surface alpha of a kind's material: a ghost while unpowered, near-solid
// while powered. Alpha also scales the emissive contribution, so the
// emissive stays far below the barrier value.
pub const BRIDGE_ALPHA_OFF: f32 = 0.15;
pub const BRIDGE_ALPHA_ON: f32 = 0.80;
pub const BRIDGE_EMISSIVE: f32 = 6.0;
// Time constant of the alpha ease between the two levels.
pub const BRIDGE_FADE_SECS: f32 = 0.25;
// Visible gap between a slab's free side and a floor's edge (a floor slab
// reaches half the wall thickness past its grid line, so the render inset is
// that plus this); a side that meets another bridge stays flush. The
// collider keeps the full rectangle.
pub const BRIDGE_EDGE_GAP: f32 = 0.3;

// ============================================================================
// Rain
// ============================================================================
// Presentation of the server-scheduled rain: how a given intensity looks.
// Tuning knobs only; structural values (probe lengths, epsilons) live in
// `vfx/rain.rs`, and density/size/radius are user-facing in
// `client.json::weather`.

// How far above the camera drops spawn (m).
pub const RAIN_SPAWN_HEIGHT: f32 = 10.0;
pub const RAIN_FALL_SPEED: f32 = 14.0;
pub const RAIN_DROP_COLOR: Color = Color::srgb(0.55, 0.6, 0.7);
// Splash on impact: droplet color (slightly brighter than the drops so
// impacts sparkle), droplet size, horizontal scatter (m), and bounce height
// (m); velocities and airtime are derived from these. Height must stay
// positive — the airtime is derived from it.
pub const RAIN_SPLASH_COLOR: Color = Color::srgb(0.7, 0.75, 0.85);
pub const RAIN_SPLASH_SIZE: f32 = 0.01;
pub const RAIN_SPLASH_RADIUS: f32 = 0.15;
pub const RAIN_SPLASH_HEIGHT: f32 = 0.2;

// ============================================================================
// Sun / Moon Disc
// ============================================================================
// Emissive tint of the celestial disc: golden sunlight at bright, cool
// blue moonlight below. Deliberately strong — the disc is bright (tonemap
// pulls it toward white) and the level's `saturation` grading mutes color
// further, so subtle tints read as plain white in game.

pub const SUN_DISC_COLOR: Color = Color::linear_rgb(1.0, 0.85, 0.6);
pub const MOON_DISC_COLOR: Color = Color::linear_rgb(0.5, 0.72, 1.0);

// ============================================================================
// Grass Wind
// ============================================================================

// Horizontal sway amplitude at the blade tip (m), oscillation speed (rad/s),
// and direction.
pub const GRASS_WIND_STRENGTH: f32 = 0.05;
pub const GRASS_WIND_SPEED: f32 = 2.5;
pub const GRASS_WIND_DIRECTION_DEGREES: f32 = 30.0;

// ============================================================================
// Projectile Impact Audio
// ============================================================================

pub const PROJECTILE_IMPACT_WALL_BOUNCE_GAIN: f32 = 0.2;
// Bounces slower than this stay silent.
pub const PROJECTILE_IMPACT_MIN_BOUNCE_SPEED: f32 = 10.0;
// Rate limit, plus the loudness ratio at which a new bounce may preempt it.
pub const PROJECTILE_IMPACT_MAX_SOUNDS_PER_SECOND: f32 = 30.0;
pub const PROJECTILE_IMPACT_PREEMPTION_LOUDNESS_RATIO: f32 = 2.0;

// ============================================================================
// Banner & Death Overlay
// ============================================================================

// Band top as a fraction of the window height: below the crosshair, above
// the console.
pub const BANNER_BAND_TOP_PERCENT: f32 = 60.0;
// Translucent black band behind the lines; the longest-lived line's fade
// modulates it so the band and its last text disappear together.
pub const BANNER_BAND_ALPHA: f32 = 0.45;
pub const DEATH_OVERLAY_SECS: f32 = 3.0;
pub const DEATH_OVERLAY_FADE_SECS: f32 = 0.8;

// ============================================================================
// Laser Beam
// ============================================================================

pub const LASER_BEAM_RADIUS: f32 = 0.008;
pub const LASER_EMISSIVE: f32 = 40.0;
// Endpoint wander as fractions of the target collider's width/height, and
// where on the target's height the beam aims.
pub const LASER_ENDPOINT_WANDER_WIDTH_FRACTION: f32 = 0.4;
pub const LASER_ENDPOINT_WANDER_HEIGHT_FRACTION: f32 = 0.2;
pub const LASER_AIM_HEIGHT_FRACTION: f32 = 0.6;

// ============================================================================
// Map Rendering
// ============================================================================

// Visual overlap into adjacent floors and walls to win the depth test where
// surfaces would otherwise be coplanar. The barrier mesh grows by this amount
// at each end (along the segment) and at top/bottom (in Y).
pub const BARRIER_OVERLAP_EPS: f32 = 0.01;

// Ladder rail and rung bar half-extents (full bar = 2x) and vertical rung
// spacing. Cosmetic only — the physics climbs the shared volume and rail
// plane (`common::constants::LADDER_*`), never the rungs.
pub const LADDER_RAIL_HALF_THICKNESS: f32 = 0.055;
pub const LADDER_RUNG_HALF_THICKNESS: f32 = 0.04;
pub const LADDER_RUNG_SPACING: f32 = 0.5;

// Settings-menu colors only — dimensions live in `client.json::hud.settings_menu`.
pub const SETTINGS_BACKDROP_COLOR: Color = QUEST_ENTRY_BG_COLOR;
// Denser than the HUD panels: it must read over the whole 3D scene.
pub const SETTINGS_PANEL_BG_COLOR: Color = Color::srgba(0.0, 0.0, 0.0, 0.75);
pub const SETTINGS_SLIDER_TRACK_COLOR: Color = QUEST_BAR_TRACK_COLOR;
// The quest-bar gold on thumbs, check marks, and pressed buttons.
pub const SETTINGS_ACCENT_COLOR: Color = QUEST_BAR_FILL_COLOR;
pub const SETTINGS_OUTLINE_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 0.35);
