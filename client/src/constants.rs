use bevy::color::Color;

// ============================================================================
// Camera Settings
// ============================================================================

// Top-down view
pub const TOPDOWN_CAMERA_MARGIN: f32 = 1.15;
pub const TOPDOWN_CAMERA_TILT_DEGREES: f32 = 35.0;

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
// Player Labels
// ============================================================================

pub const LABEL_HEIGHT_ABOVE_PLAYER: f32 = 0.5;
pub const LABEL_WIDTH: f32 = 1.0;
pub const LABEL_TEXTURE_WIDTH: u32 = 256;
pub const LABEL_TEXTURE_HEIGHT: u32 = 64;
pub const LABEL_TEXT_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 1.0);
pub const LABEL_BACKGROUND_COLOR: Color = Color::srgba(0.0, 0.0, 0.0, 0.2);
pub const LABEL_FONT_SIZE: f32 = 40.0;

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
