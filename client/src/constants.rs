// ============================================================================
// Client Game Constants
// ============================================================================

// First-person view camera settings
pub const FPV_CAMERA_HEIGHT_RATIO: f32 = 0.9; // Camera height as ratio of player height (0.9 = 90% = eye level)

// Top-down view camera settings
pub const TOPDOWN_CAMERA_HEIGHT: f32 = 50.0; // Height above ground (meters)
pub const TOPDOWN_CAMERA_Z_OFFSET: f32 = 50.0; // How far along Z axis from center (positive = south side)
pub const TOPDOWN_LOOKAT_X: f32 = 0.0; // X coordinate camera looks at
pub const TOPDOWN_LOOKAT_Y: f32 = 0.0; // Y coordinate camera looks at
pub const TOPDOWN_LOOKAT_Z: f32 = 8.5; // Z coordinate camera looks at

// Mouse sensitivity as radians per pixel
pub const MOUSE_SENSITIVITY: f32 = 0.002;

// For throtteling speed updates to the server
pub const SPEED_MAX_SEND_INTERVAL: f32 = 0.05; // seconds
pub const SPEED_DIR_CHANGE_THRESHOLD: f32 = 1.0; // degrees

// For throtteling face updates to the server
pub const FACE_MAX_SEND_INTERVAL: f32 = 0.1; // seconds
pub const FACE_CHANGE_THRESHOLD: f32 = 2.0; // degrees

// Echo request for RTT calculations in seconds
pub const ECHO_INTERVAL: f32 = 10.0;

// Player ID label settings
pub const LABEL_HEIGHT_ABOVE_PLAYER: f32 = 0.5; // How high above player (in meters)
pub const LABEL_WIDTH: f32 = 1.0; // Width of the label plane (in world units)
pub const LABEL_TEXTURE_WIDTH: u32 = 256; // Texture width in pixels
pub const LABEL_TEXTURE_HEIGHT: u32 = 64; // Texture height in pixels
pub const LABEL_TEXT_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 1.0]; // White (RGBA)
pub const LABEL_BACKGROUND_COLOR: [f32; 4] = [0.0, 0.0, 0.0, 0.2]; // Transparent (RGBA)
pub const LABEL_FONT_SIZE: f32 = 40.0; // Font size in pixels
