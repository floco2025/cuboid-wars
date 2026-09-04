use crate::constants::{DEATH_OVERLAY_FADE_SECS, DEATH_OVERLAY_SECS};
use bevy::prelude::*;

use super::resources::LocalPlayerInfo;
use crate::ui::{DeathOverlayMarker, fade_out_alpha};

const DEATH_OVERLAY_MAX_ALPHA: f32 = 0.3;

// Drive the red death tint with the same timer-driven shape the HUD
// banner uses: on the `is_dead` false→true transition, arm a
// `DEATH_OVERLAY_SECS` timer; hold the peak alpha until the final
// `DEATH_OVERLAY_FADE_SECS`, then linearly fade out. No fade in — the snap
// gives a sharper "you died" feedback than easing in.
//
// `Local<bool>` remembers last frame's `is_dead` so the system can
// detect the transition. `Local<f32>` holds the countdown timer.
pub fn death_overlay_visibility_system(
    time: Res<Time>,
    local_player_info: Res<LocalPlayerInfo>,
    mut prev_is_dead: Local<bool>,
    mut timer: Local<f32>,
    mut overlay: Query<(&mut Visibility, &mut BackgroundColor), With<DeathOverlayMarker>>,
) {
    let Ok((mut visibility, mut color)) = overlay.single_mut() else {
        return;
    };

    if local_player_info.is_dead && !*prev_is_dead {
        *timer = DEATH_OVERLAY_SECS;
    }
    *prev_is_dead = local_player_info.is_dead;

    if *timer <= 0.0 {
        color.0.set_alpha(0.0);
        // Once faded out, an equal `Visibility` write would rerun propagation every frame.
        visibility.set_if_neq(Visibility::Hidden);
        return;
    }

    *timer = (*timer - time.delta_secs()).max(0.0);
    let fade = fade_out_alpha(*timer, DEATH_OVERLAY_FADE_SECS);
    let alpha = DEATH_OVERLAY_MAX_ALPHA * fade;
    color.0 = Color::srgba(1.0, 0.0, 0.0, alpha);
    *visibility = if alpha > 0.0 {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
}
