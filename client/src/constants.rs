// ============================================================================
// Client Game Constants
// ============================================================================

// First-person view camera settings
pub const FPV_CAMERA_FOV_DEGREES: f32 = 90.0; // Field of view in degrees

// Top-down view camera settings
pub const TOPDOWN_CAMERA_FOV_DEGREES: f32 = 45.0; // Field of view in degrees
pub const TOPDOWN_CAMERA_HEIGHT: f32 = 68.0; // Height above ground (meters)
pub const TOPDOWN_CAMERA_Z_OFFSET: f32 = 55.0; // How far along Z axis from center (positive = south side)
pub const TOPDOWN_LOOKAT_X: f32 = 0.0; // X coordinate camera looks at
pub const TOPDOWN_LOOKAT_Y: f32 = 0.0; // Y coordinate camera looks at
pub const TOPDOWN_LOOKAT_Z: f32 = 8.7; // Z coordinate camera looks at
pub const TOPDOWN_WALL_ALPHA: f32 = 1.0; //0.7; // Opacity for walls in top-down view
pub const TOPDOWN_ROOF_ALPHA: f32 = 1.0; //0.3; // Opacity for roofs in top-down view

// Mouse sensitivity as radians per pixel
pub const MOUSE_SENSITIVITY: f32 = 0.002;

// Wall light asset
pub const WALL_LIGHT_MODEL: &str = "models/wall_light.glb";
pub const WALL_LIGHT_SCALE: f32 = 1.5; // uniform scale applied to light model
pub const WALL_LIGHT_BRIGHTNESS: f32 = 100000.0; // point light intensity
pub const WALL_LIGHT_RANGE: f32 = 10.0; // meters
pub const WALL_LIGHT_INWARD_OFFSET: f32 = 0.2; // push the point light further into the cell to avoid wall overlap
pub const WALL_LIGHT_RADIUS: f32 = 0.01;
pub const WALL_LIGHT_EMISSIVE_LUMINANCE: f32 = 2.0;

// Ambient light settings
pub const LIGHT_AMBIENT_BRIGHTNESS: f32 = 100.0;
pub const LIGHT_DIRECTIONAL_BRIGHTNESS: f32 = 8000.0;

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

// Item visual settings
pub const ITEM_SIZE: f32 = 0.2;
pub const ITEM_HEIGHT_ABOVE_FLOOR: f32 = 1.0;
pub const ITEM_ANIMATION_HEIGHT: f32 = 0.5;
pub const ITEM_ANIMATION_SPEED: f32 = 1.5;
pub const ITEM_SPEED_COLOR: [f32; 4] = [0.2, 0.7, 1.0, 1.0]; // Light blue
pub const ITEM_MULTISHOT_COLOR: [f32; 4] = [1.0, 0.2, 0.2, 1.0]; // Red
pub const ITEM_PHASING_COLOR: [f32; 4] = [0.2, 1.0, 0.2, 1.0]; // Green
pub const ITEM_GHOST_HUNT_COLOR: [f32; 3] = [0.973, 0.973, 1.0]; // Pale blue (same as ghost color)

// Textures
pub const TEXTURE_WALL_TILE_SIZE: f32 = 6.0;
pub const TEXTURE_WALL_ALBEDO: &str = "textures/wall-albedo.png";
pub const TEXTURE_WALL_NORMAL: &str = "textures/wall-normal-dx.png";
pub const TEXTURE_WALL_AO: &str = "textures/wall-ao.png";

pub const TEXTURE_ROOF_TILE_SIZE: f32 = 6.0;
pub const TEXTURE_ROOF_ALBEDO: &str = "textures/roof-albedo.png";
pub const TEXTURE_ROOF_NORMAL: &str = "textures/roof-normal-dx.png";
pub const TEXTURE_ROOF_AO: &str = "textures/roof-ao.png";

pub const TEXTURE_FLOOR_TILE_SIZE: f32 = 8.0;
pub const TEXTURE_FLOOR_ALBEDO: &str = "textures/floor-albedo.png";
pub const TEXTURE_FLOOR_NORMAL: &str = "textures/floor-normal-dx.png";
pub const TEXTURE_FLOOR_AO: &str = "textures/floor-ao.png";

// Visual settings for debugging
pub const RANDOM_WALL_COLORS: bool = false;
pub const RANDOM_ROOF_COLORS: bool = false;
pub const RANDOM_ROOF_WALL_COLORS: bool = false;
pub const GRID_LINES: bool = false;

// Cookie visual settings
pub const COOKIE_SIZE: f32 = 0.15; // Small sphere radius
pub const COOKIE_HEIGHT: f32 = 0.1; // Slightly above floor to avoid z-fighting
pub const COOKIE_COLOR: [f32; 4] = [0.55, 0.35, 0.2, 1.0]; // Brown

// Ghost visual settings
pub const GHOST_COLOR: [f32; 4] = [0.973, 0.973, 1.0, 0.5];

// Rearview mirror settings
pub const REARVIEW_WIDTH_RATIO: f32 = 0.25; // Width as ratio of screen width (25%)
pub const REARVIEW_HEIGHT_RATIO: f32 = 0.25; // Height as ratio of screen height (25%)
pub const REARVIEW_MARGIN: f32 = 0.02; // Margin from edge as ratio of screen size (2%)
pub const REARVIEW_FOV_DEGREES: f32 = 90.0; // Field of view for rearview mirror
