use bevy::{
    input::mouse::MouseButton,
    prelude::*,
    window::{CursorGrabMode, CursorOptions},
};

use crate::{
    markers::{LocalPlayerMarker, MainCameraMarker},
    net::ClientToServer,
    resources::{CameraViewMode, ClientToServerChannel, LocalPlayerInfo, MyPlayerId, PlayerMap},
    spawning::spawn_projectiles,
};
use common::{
    constants::{ALWAYS_MULTI_SHOT, PROJECTILE_COOLDOWN_TIME},
    protocol::*,
};

// ============================================================================
// Input Shooting System
// ============================================================================

pub fn input_shooting_system(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    cursor_options: Single<&CursorOptions>,
    local_player_query: Query<(&Position, &FaceDirection), With<LocalPlayerMarker>>,
    camera_query: Query<&Transform, (With<Camera3d>, With<MainCameraMarker>)>,
    to_server: Res<ClientToServerChannel>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    my_player_id: Option<Res<MyPlayerId>>,
    players: Res<PlayerMap>,
    map_layout: Option<Res<MapLayout>>,
    view_mode: Res<CameraViewMode>,
    time: Res<Time>,
    mut local_player_info: ResMut<LocalPlayerInfo>,
) {
    // Only allow shooting when cursor is locked
    let cursor_locked = cursor_options.grab_mode != CursorGrabMode::None;

    if cursor_locked
        && mouse.just_pressed(MouseButton::Left)
        && let Some((pos, face_dir)) = local_player_query.iter().next()
    {
        let now = time.elapsed_secs();

        let pitch = if *view_mode == CameraViewMode::FirstPerson {
            camera_query
                .iter()
                .next()
                .map_or(0.0, |transform| transform.rotation.to_euler(EulerRot::YXZ).1)
        } else {
            0.0
        };

        // Client-side cooldown guard (server still authoritative)
        if now - local_player_info.last_shot_time < PROJECTILE_COOLDOWN_TIME {
            commands.spawn((
                AudioPlayer::new(asset_server.load("sounds/player_dry_click.ogg")),
                PlaybackSettings::DESPAWN,
            ));
            return;
        }

        local_player_info.last_shot_time = now;

        // Send shot message with current face direction to server
        let shot_msg = ClientMessage::Shot(CShot {
            face_dir: face_dir.0,
            face_pitch: pitch,
        });
        let _ = to_server.send(ClientToServer::Send(shot_msg));

        // Check if player has multi-shot power-up
        let has_multi_shot = ALWAYS_MULTI_SHOT
            || my_player_id
                .as_ref()
                .and_then(|id| players.0.get(&id.0))
                .is_some_and(|info| info.multi_shot_power_up);

        if let Some(my_id) = my_player_id.as_ref()
            && let Some(map_layout) = map_layout.as_ref()
        {
            if spawn_projectiles(
                &mut commands,
                &mut meshes,
                &mut materials,
                pos,
                face_dir.0,
                pitch,
                has_multi_shot,
                map_layout.lower_walls.as_slice(),
                map_layout.ramps.as_slice(),
                map_layout.roofs.as_slice(),
                my_id.0,
            ) > 0
            {
                commands.spawn((
                    AudioPlayer::new(asset_server.load("sounds/player_fires.ogg")),
                    PlaybackSettings::DESPAWN,
                ));
            } else {
                commands.spawn((
                    AudioPlayer::new(asset_server.load("sounds/player_dry_click.ogg")),
                    PlaybackSettings::DESPAWN,
                ));
            }
        }
    }
}
