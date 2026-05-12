use bevy::prelude::*;

use super::resources::LocalPlayerInfo;
use crate::ui::DeathOverlayMarker;

// Drive the death overlay's visibility from `LocalPlayerInfo.is_dead`.
// Snapshot diff in `sync_players` sets / clears that flag; this system
// mirrors it to the UI overlay each frame.
pub fn death_overlay_visibility_system(
    local_player_info: Res<LocalPlayerInfo>,
    mut overlay: Query<(&mut Visibility, &mut BackgroundColor), With<DeathOverlayMarker>>,
) {
    let Ok((mut visibility, mut color)) = overlay.single_mut() else {
        return;
    };
    if local_player_info.is_dead {
        if *visibility != Visibility::Visible {
            *visibility = Visibility::Visible;
            color.0 = Color::srgba(1.0, 0.0, 0.0, 0.3);
        }
    } else if *visibility != Visibility::Hidden {
        *visibility = Visibility::Hidden;
        color.0 = Color::srgba(1.0, 0.0, 0.0, 0.0);
    }
}
