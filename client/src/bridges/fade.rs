use bevy::prelude::*;

use super::BridgeAssets;
use crate::{
    constants::{BRIDGE_ALPHA_OFF, BRIDGE_ALPHA_ON, BRIDGE_FADE_SECS},
    vfx::{color_with_alpha, ease_blend},
};
use common::protocol::{BridgeKindId, PlateState};

const BRIDGE_FADE_SNAP: f32 = 0.002;

// Ease each kind's shared material alpha toward its powered/unpowered level.
// One write per kind reaches every bridge of that kind; the material's own
// `base_color.alpha` is the fade state.
pub fn bridges_fade_system(
    time: Res<Time>,
    plates: Res<PlateState>,
    bridge_assets: Res<BridgeAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (idx, handle) in bridge_assets.material_handles().iter().enumerate() {
        let kind = BridgeKindId(u16::try_from(idx).expect("bridge kind index exceeds u16"));
        let Some(alpha) = materials.get(handle).map(|material| material.base_color.alpha()) else {
            continue;
        };
        let Some(next) = fade_step(alpha, fade_target(&plates, kind), time.delta_secs()) else {
            continue;
        };
        // `get_mut` marks the asset modified and re-extracts it to the GPU,
        // so a settled kind is left untouched.
        if let Some(mut material) = materials.get_mut(handle) {
            material.base_color = color_with_alpha(bridge_assets.base_color(kind), next);
        }
    }
}

fn fade_target(plates: &PlateState, kind: BridgeKindId) -> f32 {
    if plates.powered_bridge_kinds.contains(&kind) {
        BRIDGE_ALPHA_ON
    } else {
        BRIDGE_ALPHA_OFF
    }
}

// Frame-rate independent easing; `None` once settled on the target.
fn fade_step(alpha: f32, target: f32, delta_secs: f32) -> Option<f32> {
    if (alpha - target).abs() <= f32::EPSILON {
        return None;
    }
    let next = alpha + (target - alpha) * ease_blend(delta_secs, BRIDGE_FADE_SECS);
    Some(if (next - target).abs() < BRIDGE_FADE_SNAP {
        target
    } else {
        next
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_targets_follow_the_powered_kinds() {
        let plates = PlateState {
            open_barrier_kinds: Vec::new(),
            powered_bridge_kinds: vec![BridgeKindId(1)],
        };
        assert_eq!(fade_target(&plates, BridgeKindId(1)), BRIDGE_ALPHA_ON);
        assert_eq!(fade_target(&plates, BridgeKindId(0)), BRIDGE_ALPHA_OFF);
    }

    #[test]
    fn fade_step_approaches_and_settles_then_stops_writing() {
        let mut alpha = BRIDGE_ALPHA_OFF;
        let first = fade_step(alpha, BRIDGE_ALPHA_ON, 0.05).expect("first step reports settled");
        assert!(first > alpha && first < BRIDGE_ALPHA_ON);
        alpha = first;
        for _ in 0..200 {
            match fade_step(alpha, BRIDGE_ALPHA_ON, 0.05) {
                Some(next) => alpha = next,
                None => break,
            }
        }
        assert_eq!(alpha, BRIDGE_ALPHA_ON);
        assert_eq!(fade_step(alpha, BRIDGE_ALPHA_ON, 0.05), None);
    }

    #[test]
    fn material_alpha_round_trips_through_color_with_alpha() {
        let stored = color_with_alpha(Color::srgb(0.2, 0.6, 0.9), BRIDGE_ALPHA_ON);
        assert_eq!(stored.alpha(), BRIDGE_ALPHA_ON);
        assert_eq!(fade_step(stored.alpha(), BRIDGE_ALPHA_ON, 0.05), None);
    }
}
