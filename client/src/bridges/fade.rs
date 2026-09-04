use bevy::prelude::*;

use super::{BridgeAssets, assets::bridge_base_color};
use crate::constants::{BRIDGE_ALPHA_OFF, BRIDGE_ALPHA_ON, BRIDGE_FADE_SECS};
use common::protocol::{BridgeKindId, PlateState};

const BRIDGE_FADE_SNAP: f32 = 0.002;

// Ease each kind's shared material alpha toward its powered/unpowered level.
// One write per kind reaches every bridge of that kind.
pub fn bridges_fade_system(
    time: Res<Time>,
    plates: Res<PlateState>,
    bridge_assets: Res<BridgeAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut alphas: Local<Vec<f32>>,
) {
    alphas.resize(bridge_assets.material_handles().len(), BRIDGE_ALPHA_OFF);
    for (idx, handle) in bridge_assets.material_handles().iter().enumerate() {
        let kind = BridgeKindId(u16::try_from(idx).expect("bridge kind index exceeds u16"));
        let target = fade_target(&plates, kind);
        let Some(next) = fade_step(alphas[idx], target, time.delta_secs()) else {
            continue;
        };
        alphas[idx] = next;
        // A material write marks the asset modified and re-extracts it to the
        // GPU every frame, so a settled kind is left untouched.
        if let Some(mut material) = materials.get_mut(handle) {
            material.base_color = bridge_base_color(bridge_assets.base_color(kind), next);
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
    let next = alpha + (target - alpha) * (1.0 - (-delta_secs / BRIDGE_FADE_SECS).exp());
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
        let first = fade_step(alpha, BRIDGE_ALPHA_ON, 0.05).expect("first step moves");
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
}
