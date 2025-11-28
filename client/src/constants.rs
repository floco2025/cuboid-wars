// ============================================================================
// Camera Settings
// ============================================================================

// First-person view camera settings
pub const FPV_CAMERA_HEIGHT_RATIO: f32 = 0.9; // Camera height as ratio of player height (0.9 = 90% = eye level)

// Top-down view camera settings
pub const TOPDOWN_CAMERA_HEIGHT: f32 = 50.0; // Height above ground (meters)
pub const TOPDOWN_CAMERA_Z_OFFSET: f32 = 50.0; // How far along Z axis from center (positive = south side)
pub const TOPDOWN_LOOKAT_X: f32 = 0.0; // X coordinate camera looks at
pub const TOPDOWN_LOOKAT_Y: f32 = 0.0; // Y coordinate camera looks at
pub const TOPDOWN_LOOKAT_Z: f32 = 8.5; // Z coordinate camera looks at

// ============================================================================
// Input Settings
// ============================================================================

pub const MOUSE_SENSITIVITY: f32 = 0.002; // radians per pixel
pub const ROTATION_CHANGE_THRESHOLD: f32 = 0.05; // ~3 degrees

// ============================================================================
// Network Settings
// ============================================================================

pub const SPEED_MAX_SEND_INTERVAL: f32 = 0.1; // Send speed updates at most every 100ms
pub const ECHO_INTERVAL: f32 = 10.0; // Send echo requests every 10 seconds
