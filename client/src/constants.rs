use bevy::color::Color;

// ============================================================================
// Camera Settings
// ============================================================================

// Top-down view
pub const TOPDOWN_MARGIN: f32 = 1.15;
pub const TOPDOWN_TILT_DEGREES: f32 = 35.0;

// Rearview mirror
pub const REARVIEW_WIDTH_RATIO: f32 = 0.25; // Width as ratio of screen width
pub const REARVIEW_HEIGHT_RATIO: f32 = 0.25; // Height as ratio of screen height
pub const REARVIEW_MARGIN: f32 = 0.02; // Margin from edge as ratio of screen size

// ============================================================================
// Input
// ============================================================================

pub const MOUSE_SENSITIVITY: f32 = 0.002; // radians per pixel

// ============================================================================
// Network Throttling
// ============================================================================

// Move-input updates
pub const MOVE_INPUT_SEND_COOLDOWN: f32 = 0.05;
pub const MOVE_INPUT_DIR_CHANGE_THRESHOLD: f32 = 1.0; // degrees

// Face direction updates
pub const FACE_SEND_COOLDOWN: f32 = 0.1;
pub const FACE_CHANGE_THRESHOLD: f32 = 2.0; // degrees

// Round-trip time
pub const ECHO_INTERVAL: f32 = 10.0;

// ============================================================================
// Character Visuals
// ============================================================================

pub const CHARACTER_VISUAL_TURN_MIN_DURATION: f32 = 0.10; // Seconds for tiny visual turns.
pub const CHARACTER_VISUAL_TURN_MAX_DURATION: f32 = 0.25; // Seconds for large visual turns.
pub const CHARACTER_VISUAL_TURN_MAX_ANGLE: f32 = 180.0; // degrees

// ============================================================================
// Floating Labels
// ============================================================================

pub const LABEL_HEIGHT_ABOVE_CHARACTER: f32 = 0.5;
// Player floating label: name text above a health bar; needs the larger
// texture for legible text.
pub const LABEL_PLAYER_MESH_WIDTH: f32 = 1.0;
pub const LABEL_PLAYER_TEXTURE_WIDTH: u32 = 256;
pub const LABEL_PLAYER_TEXTURE_HEIGHT: u32 = 64;
// Actor floating label: just the health bar, no text. Texture matches the
// visible bar 1:1 (no padding).
pub const LABEL_ACTOR_MESH_WIDTH: f32 = 0.85;
pub const LABEL_ACTOR_TEXTURE_WIDTH: u32 = HEALTH_BAR_FLOATING_ACTOR_WIDTH as u32;
pub const LABEL_ACTOR_TEXTURE_HEIGHT: u32 = HEALTH_BAR_FLOATING_ACTOR_HEIGHT as u32;
// Beyond this distance from the main camera, character labels (player and
// actor) stop rendering. Combined with `Changed<Health>` gating, this is
// the second axis of the per-frame render-pass cost reduction for labels.
pub const LABEL_CULL_DISTANCE: f32 = 30.0; // meters
pub const LABEL_TEXT_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 1.0);
pub const LABEL_BACKGROUND_COLOR: Color = Color::srgba(0.0, 0.0, 0.0, 0.2);
pub const LABEL_FONT_SIZE: f32 = 40.0;

// ============================================================================
// Health Bars
// ============================================================================

pub const HEALTH_BAR_FLOATING_PLAYER_WIDTH: f32 = 180.0;
pub const HEALTH_BAR_FLOATING_PLAYER_HEIGHT: f32 = 10.0;
pub const HEALTH_BAR_FLOATING_ACTOR_WIDTH: f32 = 170.0;
pub const HEALTH_BAR_FLOATING_ACTOR_HEIGHT: f32 = 22.0;
pub const HEALTH_BAR_PLAYER_LIST_WIDTH: f32 = 160.0;
pub const HEALTH_BAR_PLAYER_LIST_HEIGHT: f32 = 8.0;
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
// Projectiles
// ============================================================================

pub const PROJECTILE_MIN_BOUNCE_SOUND_SPEED: f32 = 10.0; // minimum speed to play bounce sound
pub const PROJECTILE_MAX_BOUNCE_SOUNDS_PER_SECOND: f32 = 30.0; // rate limit for bounce sounds
