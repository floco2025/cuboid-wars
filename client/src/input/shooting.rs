use bevy::{ecs::system::SystemParam, input::mouse::MouseButton, prelude::*};

use crate::{
    audio::play_sound,
    cameras::{CameraAim, CameraInputState},
    config::AssetSet,
    network::{ClientToServer, ClientToServerChannel},
    players::{LocalPlayerInfo, LocalPlayerMarker, MyPlayerId},
    projectiles::{ProjectileAssets, spawn_projectiles},
};
use common::{config::GameplayConfig, physics::CollisionWorld, protocol::*};

use super::WeaponMode;

// Bundles the shooter-identity resources so `input_shooting_system` stays
// under Bevy's 16-parameter system tuple limit.
#[derive(SystemParam)]
pub struct ShooterContext<'w> {
    pub my_player_id: Res<'w, MyPlayerId>,
    pub plates: Res<'w, PlateState>,
    pub map_settings: Res<'w, MapSettings>,
}

// ============================================================================
// Input Shooting System
// ============================================================================

pub fn input_shooting_system(
    mut commands: Commands,
    mode: Res<WeaponMode>,
    mouse: Res<ButtonInput<MouseButton>>,
    input: Res<CameraInputState>,
    aim: Res<CameraAim>,
    local_player_query: Query<&Position, With<LocalPlayerMarker>>,
    to_server: Res<ClientToServerChannel>,
    asset_server: Res<AssetServer>,
    asset_set: Res<AssetSet>,
    projectile_assets: Res<ProjectileAssets>,
    shooter: ShooterContext,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    time: Res<Time>,
    mut local_player_info: ResMut<LocalPlayerInfo>,
) {
    if local_player_info.is_dead {
        return;
    }
    let pattern = match *mode {
        WeaponMode::Projectile => None,
        WeaponMode::MultiShot(index) => match gameplay_config.projectiles.multi_shot.allowed_pattern(index) {
            Some((name, _)) => Some(name),
            None => return,
        },
        _ => return,
    };
    if !input.released
        && !input.suppress_fire
        && mouse.just_pressed(MouseButton::Left)
        && let Some(pos) = local_player_query.iter().next()
    {
        let now = time.elapsed_secs();
        let pitch = aim.pitch;

        // Client-side cooldown guard (server still authoritative)
        if now - local_player_info.last_shot_time < gameplay_config.projectiles.cooldown_secs {
            play_sound(&mut commands, &asset_server, asset_set.player_sound("dry_fire"));
            return;
        }

        local_player_info.last_shot_time = now;

        let shot_msg = ClientMessage::ProjectileShot(CProjectileShot {
            face_yaw: aim.yaw,
            face_pitch: pitch,
            pattern: pattern.map(str::to_owned),
        });
        let _ = to_server.send(ClientToServer::Send(shot_msg));

        if spawn_projectiles(
            &mut commands,
            &projectile_assets,
            pos,
            aim.yaw,
            pitch,
            pattern,
            gameplay_config.player.eye_height(),
            &gameplay_config,
            shooter.map_settings.movement.projectile_speed,
            &collision_world,
            &shooter.plates.open_barrier_kinds,
            shooter.my_player_id.0,
        ) > 0
        {
            play_sound(&mut commands, &asset_server, asset_set.player_sound("fire"));
        } else {
            play_sound(&mut commands, &asset_server, asset_set.player_sound("dry_fire"));
        }
    }
}
