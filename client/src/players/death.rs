use crate::constants::{DEATH_OVERLAY_FADE_SECS, DEATH_OVERLAY_SECS};
use bevy::prelude::*;

use super::resources::LocalPlayerInfo;
use crate::{
    config::ClientSettings,
    ui::{DeathOverlayMarker, fade_out_alpha},
};

// Peak alpha of the red death tint. Hardcoded — exposing in
// `DeathOverlayConfig` is a one-line add if it ever needs tuning.
const DEATH_OVERLAY_MAX_ALPHA: f32 = 0.3;

// Drive the red death tint with the same timer-driven shape the HUD
// banner uses: on the `is_dead` false→true transition, arm a
// `duration_secs` timer; hold the peak alpha until the final
// `fade_duration_secs`, then linearly fade out. No fade in — the snap
// gives a sharper "you died" feedback than easing in.
//
// `Local<bool>` remembers last frame's `is_dead` so the system can
// detect the transition. `Local<f32>` holds the countdown timer.
pub fn death_overlay_visibility_system(
    time: Res<Time>,
    _client_settings: Res<ClientSettings>,
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
        if color.0.alpha() != 0.0 {
            color.0.set_alpha(0.0);
        }
        if *visibility != Visibility::Hidden {
            *visibility = Visibility::Hidden;
        }
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
