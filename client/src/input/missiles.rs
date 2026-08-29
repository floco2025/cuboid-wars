use bevy::{
    prelude::*,
    window::{CursorGrabMode, CursorOptions},
};

use crate::{
    audio::play_sound,
    cameras::MainCameraMarker,
    config::AssetSet,
    missiles::LockOnTarget,
    network::{ClientToServer, ClientToServerChannel},
    players::{LocalPlayerInfo, LocalPlayerMarker, MyPlayerId, PlayerMap},
};
use common::{config::GameplayConfig, protocol::*};

// Alternative fire (F): a seeking missile at the locked target. With
// `missiles.require_lock` off, an unlocked shot launches unguided along the
// aim; with it on, no lock dry-fires. No ammo always dry-fires. There is no
// fire cooldown: the ammo cap is the rate limit. Launch feedback (sound +
// the missile itself) arrives with `SMissileLaunch`, ~half an RTT later.
// The missile itself is NOT spawned locally: the server owns the whole
// flight and answers with `SMissileLaunch`; the immediate fire sound and the
// predicted ammo decrement mask the round trip.
pub fn input_missile_system(
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    cursor_options: Single<&CursorOptions>,
    camera_query: Query<&Transform, (With<Camera3d>, With<MainCameraMarker>)>,
    local_player_query: Query<&FaceYaw, With<LocalPlayerMarker>>,
    to_server: Res<ClientToServerChannel>,
    asset_server: Res<AssetServer>,
    asset_set: Res<AssetSet>,
    lock: Res<LockOnTarget>,
    my_player_id: Option<Res<MyPlayerId>>,
    mut players: ResMut<PlayerMap>,
    local_player_info: Res<LocalPlayerInfo>,
    gameplay_config: Res<GameplayConfig>,
) {
    if local_player_info.is_dead {
        return;
    }
    let cursor_locked = cursor_options.grab_mode != CursorGrabMode::None;
    // Bare F only — Cmd/Ctrl+F is the fullscreen toggle chord.
    let modifier_held = keyboard.pressed(KeyCode::SuperLeft)
        || keyboard.pressed(KeyCode::SuperRight)
        || keyboard.pressed(KeyCode::ControlLeft)
        || keyboard.pressed(KeyCode::ControlRight);
    if !(cursor_locked && keyboard.just_pressed(KeyCode::KeyF) && !modifier_held) {
        return;
    }
    let Some(my_id) = my_player_id else {
        return;
    };
    let Some(face_yaw) = local_player_query.iter().next() else {
        return;
    };

    let has_ammo = players.get(&my_id.0).is_some_and(|info| info.missiles > 0);
    if !has_ammo || (gameplay_config.missiles.require_lock && lock.0.is_none()) {
        play_sound(&mut commands, &asset_server, asset_set.player_sound("dry_fire"));
        return;
    }
    let target = lock.0;

    // Lock exists only in first person, so the camera pitch is the aim pitch.
    let pitch = camera_query
        .iter()
        .next()
        .map_or(0.0, |transform| transform.rotation.to_euler(EulerRot::YXZ).1);

    // Predicted decrement; the snapshot's `Player.missiles` self-heals it.
    if let Some(info) = players.get_mut(&my_id.0) {
        info.missiles = info.missiles.saturating_sub(1);
    }

    let _ = to_server.send(ClientToServer::Send(ClientMessage::MissileShot(CMissileShot {
        target,
        face_yaw: face_yaw.0,
        face_pitch: pitch,
    })));
    // No launch sound here: the server may still reject the shot (target
    // died / left range mid-flight of the message). The sound plays when
    // `SMissileLaunch` arrives, so it can never orphan.
}
