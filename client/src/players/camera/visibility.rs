use bevy::prelude::*;

use crate::{
    cameras::CameraViewMode,
    players::{LocalPlayerInfo, LocalPlayerMarker},
};

// Update local player visibility based on camera view mode.
pub fn local_player_visibility_sync_system(
    view_mode: Res<CameraViewMode>,
    local_player_info: Res<LocalPlayerInfo>,
    mut local_player_query: Query<&mut Visibility, With<LocalPlayerMarker>>,
) {
    // A dead local player stays hidden in any view mode: the death path hides
    // the entity, but this system would otherwise re-show it every frame in
    // top-down view, leaving the corpse on screen for the whole death window.
    let desired_visibility = if local_player_info.is_dead {
        Visibility::Hidden
    } else {
        match *view_mode {
            CameraViewMode::FirstPerson => Visibility::Hidden,
            CameraViewMode::TopDown => Visibility::Visible,
        }
    };

    // Always check and update, not just when changed, to ensure it's correct.
    for mut visibility in &mut local_player_query {
        if *visibility != desired_visibility {
            *visibility = desired_visibility;
        }
    }
}
