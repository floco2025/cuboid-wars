use bevy::prelude::*;

use super::settings_menu::SettingsMenuState;
use crate::{
    cameras::CameraViewMode,
    constants::{CROSSHAIR_COLOR, CROSSHAIR_LOCK_COLOR},
    missiles::LockOnTarget,
};

// Marker for the crosshair UI node (visible in first-person view only).
#[derive(Component)]
pub struct CrosshairMarker;

// Marker for the two crosshair bars; their color flips on missile lock.
#[derive(Component)]
pub struct CrosshairBarMarker;

pub fn ui_crosshair_lock_system(
    lock: Res<LockOnTarget>,
    mut bars: Query<&mut BackgroundColor, With<CrosshairBarMarker>>,
) {
    if !lock.is_changed() {
        return;
    }
    let want = if lock.0.is_some() {
        CROSSHAIR_LOCK_COLOR
    } else {
        CROSSHAIR_COLOR
    };
    for mut bar in &mut bars {
        if bar.0 != want {
            bar.0 = want;
        }
    }
}

pub fn ui_crosshair_visibility_system(
    view_mode: Res<CameraViewMode>,
    menu: Res<SettingsMenuState>,
    mut query: Query<&mut Visibility, With<CrosshairMarker>>,
) {
    if !view_mode.is_changed() && !menu.is_changed() {
        return;
    }

    // Hidden while the settings menu is open — it would show through the
    // translucent panel.
    for mut visibility in &mut query {
        *visibility = if view_mode.is_first_person() && !menu.open {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}
