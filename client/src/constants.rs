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

pub const CHARACTER_VISUAL_TURN_MIN_DURATION: f32 = 0.10; // Seconds for tiny visual turns.
pub const CHARACTER_VISUAL_TURN_MAX_DURATION: f32 = 0.25; // Seconds for large visual turns.
pub const CHARACTER_VISUAL_TURN_MAX_ANGLE: f32 = 180.0; // degrees

// ============================================================================
// Floating Labels (layout math; sizes live in `client.json::hud`)
// ============================================================================

// Player floating label: name text above a health bar; needs the larger
// texture for legible text. The mesh aspect ratio is driven by the texture
// dimensions, so these stay paired as constants.
pub const LABEL_PLAYER_MESH_WIDTH: f32 = 1.0;
pub const LABEL_PLAYER_TEXTURE_WIDTH: u32 = 256;
pub const LABEL_PLAYER_TEXTURE_HEIGHT: u32 = 64;
// Actor floating label: just the health bar, no text. Texture matches the
// visible bar 1:1 (no padding) — texture size is derived at the spawn
// call site from the runtime health-bar dims, not stored here.
pub const LABEL_ACTOR_MESH_WIDTH: f32 = 0.85;
pub const LABEL_TEXT_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 1.0);
pub const LABEL_BACKGROUND_COLOR: Color = Color::srgba(0.0, 0.0, 0.0, 0.2);

// ============================================================================
// Health Bars (colors only — dimensions live in `client.json::hud.health_bars`)
// ============================================================================

pub const HEALTH_BAR_TRACK_COLOR: Color = Color::srgba(0.0, 0.0, 0.0, 0.65);
pub const HEALTH_BAR_FILL_COLOR: Color = Color::srgb(0.0, 0.85, 0.2);

// ============================================================================
// Power-Up Items
// ============================================================================

pub const ITEM_SIZE: f32 = 0.3;
pub const ITEM_HEIGHT_ABOVE_FLOOR: f32 = 1.2;
pub const ITEM_ANIMATION_HEIGHT: f32 = 0.4;
pub const ITEM_ANIMATION_SPEED: f32 = 0.8;
pub const ITEM_EMISSIVE_STRENGTH: f32 = 0.1; // Multiplier for emissive glow
pub const ITEM_SPEED_COLOR: Color = Color::srgb(0.2, 0.7, 1.0); // Light blue
pub const ITEM_MULTISHOT_COLOR: Color = Color::srgb(1.0, 0.2, 0.2); // Red
pub const ITEM_PHASING_COLOR: Color = Color::srgb(0.2, 1.0, 0.2); // Green
pub const ITEM_ANTI_GRAVITY_COLOR: Color = Color::srgb(0.7, 0.3, 1.0); // Purple

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
pub const KEY_ROTATION_HZ: f32 = 0.4; // slow coin-spin around Y

// HUD: thin vertical bars next to the player name, after the power-up icons.
// Shape (narrow vertical) is intentionally different from the 12×12 power-up
// squares so the two categories read as different.
pub const HUD_KEY_ICON_WIDTH_PX: f32 = 4.0;
pub const HUD_KEY_ICON_HEIGHT_PX: f32 = 12.0;
pub const HUD_KEY_GAP_PX: f32 = 8.0;

// ============================================================================
// Projectiles
// ============================================================================

pub const PROJECTILE_MIN_BOUNCE_SOUND_SPEED: f32 = 10.0; // minimum speed to play bounce sound
pub const PROJECTILE_MAX_BOUNCE_SOUNDS_PER_SECOND: f32 = 30.0; // rate limit for bounce sounds
