use bevy::{audio::SpatialListener, prelude::*};
use common::physics::CollisionWorld;

use super::LowPassCutoff;
use crate::constants::{
    AUDIO_OCCLUSION_CUTOFF_HZ, AUDIO_OCCLUSION_END_MARGIN, AUDIO_OCCLUSION_FADE_RATE,
    AUDIO_OCCLUSION_LAYER_CUTOFF_RATIO, AUDIO_OCCLUSION_LAYER_GAIN, AUDIO_OCCLUSION_LAYER_GAP,
    AUDIO_OCCLUSION_MAX_LAYERS, AUDIO_OCCLUSION_OPEN_CUTOFF_HZ, AUDIO_OCCLUSION_PROBE_HZ,
};

const FACTOR_SNAP: f32 = 1e-3;

// How much geometry stands between a spatial sound and the camera: the probe
// counts the layers, the factor eases toward that count, and the gain and
// filter cutoff follow.
#[derive(Component)]
pub(crate) struct AudioOcclusion {
    cutoff: LowPassCutoff,
    layers: f32,
    // None until the first probe, which sets the factor outright.
    factor: Option<f32>,
    // Paused only until the first probe and volume have reached the sink.
    held: bool,
}

impl Default for AudioOcclusion {
    fn default() -> Self {
        Self {
            cutoff: LowPassCutoff::open(),
            layers: 0.0,
            factor: None,
            held: false,
        }
    }
}

impl AudioOcclusion {
    pub(super) fn holding() -> Self {
        Self {
            held: true,
            ..Self::default()
        }
    }

    pub(super) fn release(&mut self) -> bool {
        std::mem::take(&mut self.held)
    }

    pub(super) fn cutoff(&self) -> &LowPassCutoff {
        &self.cutoff
    }

    pub(super) fn gain(&self) -> f32 {
        occlusion_gain(self.factor.unwrap_or(0.0))
    }

    pub(super) fn probe(&mut self, layers: f32) {
        self.layers = layers;
    }

    pub(super) fn ease(&mut self, delta_secs: f32) {
        let target = self.layers;
        let factor = match self.factor {
            None => target,
            Some(mut factor) => {
                factor.smooth_nudge(&target, AUDIO_OCCLUSION_FADE_RATE, delta_secs);
                if (factor - target).abs() < FACTOR_SNAP {
                    target
                } else {
                    factor
                }
            }
        };
        self.factor = Some(factor);
        self.cutoff.set(occlusion_cutoff(factor));
    }
}

#[derive(Resource)]
pub(super) struct OcclusionClock(Timer);

impl Default for OcclusionClock {
    fn default() -> Self {
        Self(Timer::from_seconds(
            1.0 / AUDIO_OCCLUSION_PROBE_HZ,
            TimerMode::Repeating,
        ))
    }
}

fn occlusion_gain(factor: f32) -> f32 {
    AUDIO_OCCLUSION_LAYER_GAIN.powf(factor)
}

// The first layer sweeps the cutoff down from open; each further one
// multiplies it.
fn occlusion_cutoff(factor: f32) -> f32 {
    if factor <= 0.0 {
        return f32::INFINITY;
    }
    AUDIO_OCCLUSION_OPEN_CUTOFF_HZ
        * (AUDIO_OCCLUSION_CUTOFF_HZ / AUDIO_OCCLUSION_OPEN_CUTOFF_HZ).powf(factor.min(1.0))
        * AUDIO_OCCLUSION_LAYER_CUTOFF_RATIO.powf((factor - 1.0).max(0.0))
}

// One layer per surface entered, whatever its thickness; entries within
// AUDIO_OCCLUSION_LAYER_GAP of the last counted one are the seams where wall
// and slab segments overlap. Fields lie outside the query's groups, and the
// last stretch is ignored because an impact sound sits on the surface it
// struck.
fn sound_path_layers(world: &CollisionWorld, listener: Vec3, emitter: Vec3) -> f32 {
    let path = emitter - listener;
    let reach = path.length() - AUDIO_OCCLUSION_END_MARGIN;
    if reach <= 0.0 {
        return 0.0;
    }
    let mut layers = 0.0_f32;
    let mut last_entry = f32::NEG_INFINITY;
    for hit in world.world_surfaces_along_ray(listener, path, reach) {
        let entry = hit.point.distance(listener);
        if entry - last_entry >= AUDIO_OCCLUSION_LAYER_GAP {
            layers += 1.0;
            last_entry = entry;
        }
    }
    layers.min(AUDIO_OCCLUSION_MAX_LAYERS)
}

pub(super) fn audio_occlusion_system(
    time: Res<Time>,
    mut clock: ResMut<OcclusionClock>,
    world: Res<CollisionWorld>,
    listeners: Query<&GlobalTransform, With<SpatialListener>>,
    mut emitters: Query<(&GlobalTransform, &mut AudioOcclusion)>,
) {
    let Some(listener) = listeners.iter().next().map(GlobalTransform::translation) else {
        return;
    };
    let probe_all = clock.0.tick(time.delta()).just_finished();
    for (transform, mut occlusion) in &mut emitters {
        if probe_all || occlusion.factor.is_none() {
            occlusion.probe(sound_path_layers(&world, listener, transform.translation()));
        }
        occlusion.ease(time.delta_secs());
    }
}

#[cfg(test)]
#[path = "tests/occlusion.rs"]
mod tests;
