use bevy::prelude::*;

// ============================================================================
// Components
// ============================================================================

// The local player's run-up for the bump sound: horizontal distance moved
// since the body last stood still or hit something. Every hit spends it, so
// holding or sliding against a wall never rebuilds one.
#[derive(Component, Default)]
pub struct BumpFeedbackState {
    pub run_up: f32,
}

// ============================================================================
// Camera and Visual Effects
// ============================================================================

// Camera shake effect - tracks duration and intensity. `dir_{x,y,z}` is the
// shake "direction" each axis is modulated against, allowing the same
// component to serve both horizontal hits (small `dir_y` for a slight
// vertical jolt) and vertical impacts like fall damage (`dir_x = dir_z =
// 0`, larger `dir_y`).
#[derive(Component)]
pub struct CameraShake {
    pub timer: Timer,
    pub intensity: f32,
    pub dir_x: f32,
    pub dir_y: f32,
    pub dir_z: f32,
    pub offset_x: f32, // Current shake offset
    pub offset_y: f32,
    pub offset_z: f32,
}

// Cuboid shake effect - tracks duration and intensity
#[derive(Component)]
pub struct CuboidShake {
    pub timer: Timer,
    pub intensity: f32,
    pub dir_x: f32, // Direction of impact
    pub dir_z: f32,
    pub offset_x: f32, // Current shake offset
    pub offset_z: f32,
}

// Portal-transit view transient: on teleport the camera is seeded with the
// fully mapped (possibly tilted) exit view; this decays that tilt back onto
// the upright aim. Pure presentation — the aim underneath already IS the
// target, so mouse input keeps working during the blend.
#[derive(Component)]
pub struct PortalTransitBlend {
    pub delta: Quat,
    pub timer: Timer,
}
