use bevy::{
    ecs::system::SystemParam,
    input::mouse::MouseButton,
    prelude::*,
    window::{CursorGrabMode, CursorOptions},
};

use crate::{
    barriers::OpenBarrierKinds,
    cameras::{CameraViewMode, MainCameraMarker},
    config::AssetSet,
    network::{ClientToServer, ClientToServerChannel},
    players::{LocalPlayerInfo, LocalPlayerMarker, MyPlayerId, PlayerMap},
    projectiles::{ProjectileAssets, spawn_projectiles},
};
use common::{
    config::GameplayConfig,
    constants::{ALWAYS_MULTI_SHOT, PROJECTILE_COOLDOWN_TIME},
    physics::CollisionWorld,
    protocol::*,
};

// Bundles the shooter-identity resources so `input_shooting_system` stays
// under Bevy's 16-parameter system tuple limit.
#[derive(SystemParam)]
pub struct ShooterContext<'w> {
    pub my_player_id: Option<Res<'w, MyPlayerId>>,
    pub players: Res<'w, PlayerMap>,
    pub open_barrier_kinds: Res<'w, OpenBarrierKinds>,
}

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
    asset_set: Res<AssetSet>,
    projectile_assets: Res<ProjectileAssets>,
    shooter: ShooterContext,
    collision_world: Option<Res<CollisionWorld>>,
    gameplay_config: Res<GameplayConfig>,
    view_mode: Res<CameraViewMode>,
    time: Res<Time>,
    mut local_player_info: ResMut<LocalPlayerInfo>,
) {
    // Dead players can't shoot.
    if local_player_info.is_dead {
        return;
    }
    // Only allow shooting when cursor is locked
    let cursor_locked = cursor_options.grab_mode != CursorGrabMode::None;

    if cursor_locked
        && mouse.just_pressed(MouseButton::Left)
        && let Some((pos, face_dir)) = local_player_query.iter().next()
    {
        let now = time.elapsed_secs();

        let pitch = if view_mode.is_first_person() {
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
                AudioPlayer::new(asset_server.load(asset_set.player_sound("dry_fire").to_owned())),
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
            || shooter
                .my_player_id
                .as_ref()
                .and_then(|id| shooter.players.get(&id.0))
                .is_some_and(|info| info.power_up(PowerUpKind::MultiShot));

        if let Some(my_id) = shooter.my_player_id.as_ref()
            && let Some(collision_world) = collision_world.as_ref()
        {
            if spawn_projectiles(
                &mut commands,
                &projectile_assets,
                pos,
                face_dir.0,
                pitch,
                has_multi_shot,
                gameplay_config.player.eye_height(),
                collision_world,
                &shooter.open_barrier_kinds.0,
                my_id.0,
            ) > 0
            {
                commands.spawn((
                    AudioPlayer::new(asset_server.load(asset_set.player_sound("fire").to_owned())),
                    PlaybackSettings::DESPAWN,
                ));
            } else {
                commands.spawn((
                    AudioPlayer::new(asset_server.load(asset_set.player_sound("dry_fire").to_owned())),
                    PlaybackSettings::DESPAWN,
                ));
            }
        }
    }
}
