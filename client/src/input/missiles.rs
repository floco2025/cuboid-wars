use bevy::{input::mouse::MouseButton, prelude::*};

use crate::{
    audio::play_sound,
    cameras::{CameraAim, CameraInputState},
    config::AssetSet,
    missiles::LockOnTarget,
    network::{ClientToServer, ClientToServerChannel},
    players::{LocalPlayerInfo, LocalPlayerMarker, MyPlayerId, PlayerMap},
};
use common::{
    config::GameplayConfig,
    constants::{MISSILE_RADIUS, MISSILE_SPAWN_OFFSET},
    physics::CollisionWorld,
    protocol::*,
};

use super::WeaponMode;

// Selected-weapon fire: a seeking missile at the locked target. With
// `missiles.require_lock` off, an unlocked shot launches unguided along the
// aim; with it on, no lock dry-fires. No ammo always dry-fires. There is no
// fire cooldown: the ammo cap is the rate limit. Launch feedback (sound +
// the missile itself) arrives with `SMissileLaunch`, ~half an RTT later.
// The missile itself is NOT spawned locally: the server owns the whole
// flight and answers with `SMissileLaunch`; the immediate fire sound and the
// predicted ammo decrement mask the round trip.
pub fn input_missile_system(
    mut commands: Commands,
    mode: Res<WeaponMode>,
    mouse: Res<ButtonInput<MouseButton>>,
    input: Res<CameraInputState>,
    aim: Res<CameraAim>,
    local_player_query: Query<&FaceYaw, With<LocalPlayerMarker>>,
    to_server: Res<ClientToServerChannel>,
    asset_server: Res<AssetServer>,
    asset_set: Res<AssetSet>,
    lock: Res<LockOnTarget>,
    my_player_id: Res<MyPlayerId>,
    mut players: ResMut<PlayerMap>,
    local_player_info: Res<LocalPlayerInfo>,
    gameplay_config: Res<GameplayConfig>,
    collision_world: Res<CollisionWorld>,
    plates: Res<PlateState>,
) {
    if local_player_info.is_dead || *mode != WeaponMode::Missile {
        return;
    }
    if input.released || input.suppress_fire || !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let Some(_) = local_player_query.iter().next() else {
        return;
    };

    let has_ammo = players.get(&my_player_id.0).is_some_and(|info| info.missiles > 0);
    if !has_ammo
        || (gameplay_config.missiles.require_lock && lock.0.is_none())
        || !collision_world.projectile_path_clear(
            aim.origin,
            aim.direction * MISSILE_SPAWN_OFFSET,
            MISSILE_RADIUS,
            &plates.open_barrier_kinds,
        )
    {
        play_sound(&mut commands, &asset_server, asset_set.player_sound("dry_fire"));
        return;
    }
    let target = lock.0;

    let pitch = aim.pitch;

    // Predicted decrement; the snapshot's `Player.missiles` self-heals it.
    if let Some(info) = players.get_mut(&my_player_id.0) {
        info.missiles = info.missiles.saturating_sub(1);
    }

    let _ = to_server.send(ClientToServer::Send(ClientMessage::MissileShot(CMissileShot {
        target,
        face_yaw: aim.yaw,
        face_pitch: pitch,
    })));
    // No launch sound here: the server may still reject the shot (target
    // died / left range mid-flight of the message). The sound plays when
    // `SMissileLaunch` arrives, so it can never orphan.
}
