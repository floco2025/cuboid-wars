use bevy::prelude::*;

use super::BarrierAssets;
use crate::constants::*;

// Drive each color's shared emissive intensity by a sine wave. Per-color
// phase offsets keep the four colors visually out of lockstep. Because the
// material handle is shared across every barrier of that color, one write
// here updates every barrier instance.
pub fn barriers_pulsate_system(
    time: Res<Time>,
    barrier_assets: Option<Res<BarrierAssets>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(assets) = barrier_assets else { return };
    let t = time.elapsed_secs();
    let entries: [(&Handle<StandardMaterial>, Color, f32); 4] = [
        (&assets.red, BARRIER_RED_COLOR, 0.0),
        (&assets.blue, BARRIER_BLUE_COLOR, 0.5),
        (&assets.green, BARRIER_GREEN_COLOR, 1.0),
        (&assets.yellow, BARRIER_YELLOW_COLOR, 1.5),
    ];
    for (handle, color, phase) in entries {
        let Some(mat) = materials.get_mut(handle) else {
            continue;
        };
        let s = (t * BARRIER_PULSE_HZ * std::f32::consts::TAU + phase).sin() * 0.5 + 0.5;
        let intensity = BARRIER_EMISSIVE_MIN + (BARRIER_EMISSIVE_MAX - BARRIER_EMISSIVE_MIN) * s;
        mat.emissive = color.to_linear() * intensity;
    }
}
