// ============================================================================
// Camera Settings
// ============================================================================

// First-person view
pub const FPV_CAMERA_FOV_DEGREES: f32 = 90.0;

// Top-down view
pub const TOPDOWN_CAMERA_FOV_DEGREES: f32 = 45.0;
pub const TOPDOWN_CAMERA_HEIGHT: f32 = 68.0; // meters above ground
pub const TOPDOWN_CAMERA_Z_OFFSET: f32 = 55.0; // meters along Z axis from center
pub const TOPDOWN_LOOKAT_X: f32 = 0.0;
pub const TOPDOWN_LOOKAT_Y: f32 = 0.0;
pub const TOPDOWN_LOOKAT_Z: f32 = 8.7;
pub const TOPDOWN_WALL_ALPHA: f32 = 1.0;
pub const TOPDOWN_ROOF_ALPHA: f32 = 1.0;

// Rearview mirror
pub const REARVIEW_WIDTH_RATIO: f32 = 0.25; // Width as ratio of screen width
pub const REARVIEW_HEIGHT_RATIO: f32 = 0.25; // Height as ratio of screen height
pub const REARVIEW_MARGIN: f32 = 0.02; // Margin from edge as ratio of screen size
pub const REARVIEW_FOV_DEGREES: f32 = 90.0;

// ============================================================================
// Input
// ============================================================================

pub const MOUSE_SENSITIVITY: f32 = 0.002; // radians per pixel

// ============================================================================
// Player Model
// ============================================================================

pub const PLAYER_MODEL: &str = "models/player.glb#Scene0";
pub const PLAYER_MODEL_SCALE: f32 = 0.53;
pub const PLAYER_MODEL_HEIGHT_OFFSET: f32 = 0.0;
pub const PLAYER_MODEL_ANIMATION_SPEED: f32 = 2.0;
pub const PLAYER_BOUNDING_BOX: bool = false; // Debug visualization

// ============================================================================
// Lighting
// ============================================================================

// Wall lights
pub const WALL_LIGHT_MODEL: &str = "models/wall_light.glb";
pub const WALL_LIGHT_SCALE: f32 = 1.5;
pub const WALL_LIGHT_BRIGHTNESS: f32 = 100_000.0;
pub const WALL_LIGHT_RANGE: f32 = 10.0; // meters
pub const WALL_LIGHT_INWARD_OFFSET: f32 = 0.2; // meters
pub const WALL_LIGHT_RADIUS: f32 = 0.1; // Larger radius = softer specular highlights, less shimmer
pub const WALL_LIGHT_EMISSIVE_LUMINANCE: f32 = 2.0;

// Scene lighting
pub const LIGHT_AMBIENT_BRIGHTNESS: f32 = 100.0;
pub const LIGHT_DIRECTIONAL_BRIGHTNESS: f32 = 8000.0;

// ============================================================================
// Network Throttling
// ============================================================================

// Speed updates
pub const SPEED_MAX_SEND_INTERVAL: f32 = 0.05; // seconds
pub const SPEED_DIR_CHANGE_THRESHOLD: f32 = 1.0; // degrees

// Face direction updates
pub const FACE_MAX_SEND_INTERVAL: f32 = 0.1; // seconds
pub const FACE_CHANGE_THRESHOLD: f32 = 2.0; // degrees

// Round-trip time
pub const ECHO_INTERVAL: f32 = 10.0; // seconds

// ============================================================================
// Player Labels
// ============================================================================

pub const LABEL_HEIGHT_ABOVE_PLAYER: f32 = 0.5; // meters
pub const LABEL_WIDTH: f32 = 1.0; // world units
pub const LABEL_TEXTURE_WIDTH: u32 = 256; // pixels
pub const LABEL_TEXTURE_HEIGHT: u32 = 64; // pixels
pub const LABEL_TEXT_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 1.0]; // RGBA
pub const LABEL_BACKGROUND_COLOR: [f32; 4] = [0.0, 0.0, 0.0, 0.2]; // RGBA
pub const LABEL_FONT_SIZE: f32 = 40.0; // pixels

// ============================================================================
// Power-Up Items
// ============================================================================

pub const ITEM_SIZE: f32 = 0.3;
pub const ITEM_HEIGHT_ABOVE_FLOOR: f32 = 1.2;
pub const ITEM_ANIMATION_HEIGHT: f32 = 0.4;
pub const ITEM_ANIMATION_SPEED: f32 = 0.8;
pub const ITEM_EMISSIVE_STRENGTH: f32 = 0.1; // Multiplier for emissive glow
pub const ITEM_SPEED_COLOR: [f32; 3] = [0.2, 0.7, 1.0]; // Light blue
pub const ITEM_MULTISHOT_COLOR: [f32; 3] = [1.0, 0.2, 0.2]; // Red
pub const ITEM_PHASING_COLOR: [f32; 3] = [0.2, 1.0, 0.2]; // Green
pub const ITEM_SENTRY_HUNT_COLOR: [f32; 3] = [0.973, 0.973, 1.0]; // Pale blue

// ============================================================================
// Cookies
// ============================================================================

pub const COOKIE_SIZE: f32 = 0.15; // sphere radius
pub const COOKIE_HEIGHT: f32 = 0.16; // meters above floor

// ============================================================================
// Sentries
// ============================================================================

pub const SENTRY_MODEL: &str = "models/sentry.glb#Scene0";
pub const SENTRY_MODEL_SCALE: f32 = 0.012;
pub const SENTRY_MODEL_HEIGHT_OFFSET: f32 = 0.0;
pub const SENTRY_MODEL_DEPTH_OFFSET: f32 = 0.1;
pub const SENTRY_MODEL_ANIMATION_SPEED: f32 = 1.0; // Walk animation speed
pub const SENTRY_WALK_ANIMATION_INDEX: usize = 20; // Index of walk animation in glb
pub const SENTRY_BOUNDING_BOX: bool = false; // Debug visualization

// ============================================================================
// Textures & Materials
// ============================================================================

// Tile sizes
pub const TEXTURE_WALL_TILE_SIZE: f32 = 6.0;
pub const TEXTURE_ROOF_TILE_SIZE: f32 = 6.0;
pub const TEXTURE_FLOOR_TILE_SIZE: f32 = 8.0;

// Material properties - Walls
pub const TEXTURE_WALL_METALLIC: f32 = 0.5;
pub const TEXTURE_WALL_ROUGHNESS: f32 = 0.5;

// Material properties - Roofs
pub const TEXTURE_ROOF_METALLIC: f32 = 0.5;
pub const TEXTURE_ROOF_ROUGHNESS: f32 = 0.5;

// Material properties - Floor
pub const TEXTURE_FLOOR_METALLIC: f32 = 0.5;
pub const TEXTURE_FLOOR_ROUGHNESS: f32 = 0.5;

// Material properties - Cookies
pub const TEXTURE_COOKIE_METALLIC: f32 = 1.0;
pub const TEXTURE_COOKIE_ROUGHNESS: f32 = 1.0;

// Material properties - Power-ups
pub const TEXTURE_ITEM_METALLIC: f32 = 1.0;
pub const TEXTURE_ITEM_ROUGHNESS: f32 = 1.0;

// ============================================================================
// Debug Visualization
// ============================================================================

pub const GRID_LINES: bool = false;
