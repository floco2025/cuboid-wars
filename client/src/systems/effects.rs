use bevy::prelude::*;
use std::time::Duration;

// ============================================================================
// Components
// ============================================================================

// Camera shake effect - tracks duration and intensity
#[derive(Component)]
pub struct CameraShake {
    pub timer: Timer,
    pub intensity: f32,
    pub dir_x: f32,   // Direction of impact
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
    pub dir_x: f32,   // Direction of impact
    pub dir_z: f32,
    pub offset_x: f32, // Current shake offset
    pub offset_z: f32,
}

// ============================================================================
// Visual Effects Systems
// ============================================================================

// Apply camera shake effect - updates shake offset
pub fn apply_camera_shake_system(
    mut commands: Commands,
    time: Res<Time>,
    mut camera_query: Query<(Entity, &mut CameraShake), With<Camera3d>>,
) {
    for (entity, mut shake) in camera_query.iter_mut() {
        update_camera_shake(&mut commands, entity, time.delta(), &mut shake);
    }
}

// Apply cuboid shake effect - updates shake offset
pub fn apply_cuboid_shake_system(
    mut commands: Commands,
    time: Res<Time>,
    mut cuboid_query: Query<(Entity, &mut CuboidShake)>,
) {
    for (entity, mut shake) in cuboid_query.iter_mut() {
        update_cuboid_shake(&mut commands, entity, time.delta(), &mut shake);
    }
}

fn update_camera_shake(commands: &mut Commands, entity: Entity, delta: Duration, shake: &mut CameraShake) {
    shake.timer.tick(delta);
    if shake.timer.is_finished() {
        commands.entity(entity).remove::<CameraShake>();
        return;
    }

    let progress = shake.timer.fraction();
    let amplitude = shake.intensity * (1.0 - progress);
    let oscillation = (progress * 30.0).sin();

    shake.offset_x = shake.dir_x * amplitude * oscillation;
    shake.offset_z = shake.dir_z * amplitude * oscillation;
    shake.offset_y = amplitude * oscillation * 0.2;
}

fn update_cuboid_shake(commands: &mut Commands, entity: Entity, delta: Duration, shake: &mut CuboidShake) {
    shake.timer.tick(delta);
    if shake.timer.is_finished() {
        commands.entity(entity).remove::<CuboidShake>();
        return;
    }

    let progress = shake.timer.fraction();
    let amplitude = shake.intensity * (1.0 - progress);
    let bounce = (progress * 20.0).sin();

    shake.offset_x = shake.dir_x * amplitude * bounce;
    shake.offset_z = shake.dir_z * amplitude * bounce;
}
