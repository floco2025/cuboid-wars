use super::particles::ExplosionVfxBudget;
use bevy::prelude::*;

// Fireball flash and shockwave ring share one animation: ease-out scale
// growth plus alpha + emissive fade on a per-instance material clone.
#[derive(Component)]
pub struct ExplosionPulse {
    pub(super) elapsed: f32,
    pub(super) lifetime: f32,
    pub(super) max_scale: f32,
    pub(super) start_alpha: f32,
    // Template emissive at spawn; the fade rescales from this each frame.
    pub(super) base_emissive: LinearRgba,
    pub(super) material: Handle<StandardMaterial>,
}

#[derive(Component)]
pub struct ExplosionLight {
    pub(super) elapsed: f32,
    pub(super) lifetime: f32,
    pub(super) intensity: f32,
    pub(super) range: f32,
}

pub fn explosion_pulse_system(
    mut commands: Commands,
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut pulses: Query<(Entity, &mut ExplosionPulse, &mut Transform)>,
) {
    let delta = time.delta_secs();
    for (entity, mut pulse, mut transform) in &mut pulses {
        pulse.elapsed += delta;
        // The per-instance material asset frees itself when the entity drops
        // the last handle to it.
        if pulse.elapsed >= pulse.lifetime {
            commands.entity(entity).despawn();
            continue;
        }
        let progress = (pulse.elapsed / pulse.lifetime).clamp(0.0, 1.0);
        let grow = 1.0 - (1.0 - progress).powi(3);
        transform.scale = Vec3::splat(pulse.max_scale * grow);
        if let Some(mut material) = materials.get_mut(&pulse.material) {
            material.base_color.set_alpha(pulse.start_alpha * (1.0 - progress));
            // Emissive has no ceiling, so the alpha fade alone can't pull
            // extreme brightness values under the bloom threshold before
            // despawn — square-fade the emissive too so the glow dies
            // smoothly instead of blinking out.
            material.emissive = pulse.base_emissive * (1.0 - progress).powi(2);
        }
    }
}

pub fn explosion_lights_system(
    mut commands: Commands,
    time: Res<Time>,
    mut budget: ResMut<ExplosionVfxBudget>,
    mut lights: Query<(Entity, &mut ExplosionLight, &mut PointLight)>,
) {
    let delta = time.delta_secs();
    for (entity, mut state, mut light) in &mut lights {
        state.elapsed += delta;
        if state.elapsed >= state.lifetime {
            budget.release_light();
            commands.entity(entity).despawn();
            continue;
        }
        let progress = (state.elapsed / state.lifetime).clamp(0.0, 1.0);
        let fade = (1.0 - progress).powi(2);
        light.intensity = state.intensity * fade;
        light.range = state.range * fade.max(0.25);
    }
}
