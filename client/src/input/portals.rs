use bevy::{
    input::mouse::MouseButton,
    prelude::*,
    window::{CursorGrabMode, CursorOptions},
};

use crate::{
    audio::play_sound,
    cameras::{CameraViewMode, MainCameraMarker},
    config::AssetSet,
    network::{ClientToServer, ClientToServerChannel},
    players::{LocalPlayerInfo, LocalPlayerMarker},
};
use common::protocol::*;

// Which weapon the mouse buttons drive. Client-only presentation state, like
// `CameraViewMode`; the server just receives whichever shot message results.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeaponMode {
    #[default]
    Gun,
    PortalGun,
}

impl WeaponMode {
    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            Self::Gun => Self::PortalGun,
            Self::PortalGun => Self::Gun,
        }
    }
}

pub fn input_weapon_toggle_system(keyboard: Res<ButtonInput<KeyCode>>, mut mode: ResMut<WeaponMode>) {
    if keyboard.just_pressed(KeyCode::KeyQ) {
        *mode = mode.toggled();
    }
}

// Portal-gun fire: left click places end A (blue), right click end B
// (orange). The gun always fires — the immediate sound is the trigger
// feedback — but the outcome is the server's: a miss fizzles silently and
// the portal appears with `SPortalOpened`. Nothing is spawned locally.
pub fn input_portal_system(
    mut commands: Commands,
    mode: Res<WeaponMode>,
    mouse: Res<ButtonInput<MouseButton>>,
    cursor_options: Single<&CursorOptions>,
    camera_query: Query<&Transform, (With<Camera3d>, With<MainCameraMarker>)>,
    local_player_query: Query<&FaceYaw, With<LocalPlayerMarker>>,
    to_server: Res<ClientToServerChannel>,
    asset_server: Res<AssetServer>,
    asset_set: Res<AssetSet>,
    view_mode: Res<CameraViewMode>,
    local_player_info: Res<LocalPlayerInfo>,
) {
    if *mode != WeaponMode::PortalGun || local_player_info.is_dead {
        return;
    }
    if cursor_options.grab_mode == CursorGrabMode::None {
        return;
    }
    let end = if mouse.just_pressed(MouseButton::Left) {
        PortalEnd::A
    } else if mouse.just_pressed(MouseButton::Right) {
        PortalEnd::B
    } else {
        return;
    };
    let Some(face_yaw) = local_player_query.iter().next() else {
        return;
    };
    let pitch = if view_mode.is_first_person() {
        camera_query
            .iter()
            .next()
            .map_or(0.0, |transform| transform.rotation.to_euler(EulerRot::YXZ).1)
    } else {
        0.0
    };

    play_sound(&mut commands, &asset_server, asset_set.player_sound("fire"));
    let _ = to_server.send(ClientToServer::Send(ClientMessage::PortalShot(CPortalShot {
        end,
        face_yaw: face_yaw.0,
        face_pitch: pitch,
    })));
}
