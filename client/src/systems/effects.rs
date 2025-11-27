use bevy::prelude::*;

// ============================================================================
// Components
// ============================================================================

// Camera shake effect - tracks duration and intensity
#[derive(Component)]
pub struct CameraShake {
    pub timer: Timer,
    pub intensity: f32,
    pub direction_x: f32, // Direction of impact
    pub direction_z: f32,
    pub offset_x: f32,    // Current shake offset
    pub offset_y: f32,
    pub offset_z: f32,
}

// Cuboid shake effect - tracks duration and intensity
#[derive(Component)]
pub struct CuboidShake {
    pub timer: Timer,
    pub intensity: f32,
    pub direction_x: f32, // Direction of impact
    pub direction_z: f32,
    pub offset_x: f32,    // Current shake offset
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
        shake.timer.tick(time.delta());
        
        if shake.timer.is_finished() {
            // Remove shake component when done
            commands.entity(entity).remove::<CameraShake>();
        } else {
            // Calculate oscillating offset in the hit direction
            let progress = shake.timer.fraction();
            let amplitude = shake.intensity * (1.0 - progress); // Decay over time
            let oscillation = (progress * 30.0).sin(); // Fast oscillation
            
            shake.offset_x = shake.direction_x * amplitude * oscillation;
            shake.offset_z = shake.direction_z * amplitude * oscillation;
            shake.offset_y = amplitude * oscillation * 0.2; // Slight vertical shake
        }
    }
}

// Apply cuboid shake effect - updates shake offset
pub fn apply_cuboid_shake_system(
    mut commands: Commands,
    time: Res<Time>,
    mut cuboid_query: Query<(Entity, &mut CuboidShake)>,
) {
    for (entity, mut shake) in cuboid_query.iter_mut() {
        shake.timer.tick(time.delta());
        
        if shake.timer.is_finished() {
            // Remove shake component when done
            commands.entity(entity).remove::<CuboidShake>();
        } else {
            // Calculate bouncing back effect in the hit direction
            let progress = shake.timer.fraction();
            let amplitude = shake.intensity * (1.0 - progress); // Decay over time
            let bounce = (progress * 20.0).sin(); // Bounce oscillation
            
            shake.offset_x = shake.direction_x * amplitude * bounce;
            shake.offset_z = shake.direction_z * amplitude * bounce;
        }
    }
}
