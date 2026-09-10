use bevy::{ecs::system::SystemParam, input::mouse::MouseButton, prelude::*};

use crate::{
    audio::play_sound,
    cameras::{CameraAim, CameraInputState},
    config::AssetSet,
    constants::{MISSILE_RADIUS, MISSILE_SPAWN_OFFSET},
    missiles::{LockOnTarget, clear_launch_direction},
    network::{ClientToServer, ClientToServerChannel},
    players::{LocalPlayerInfo, LocalPlayerMarker, MyPlayerId, PlayerMap},
};
use common::{
    config::GameplayConfig,
    physics::CollisionWorld,
    protocol::{CMissileShot, ClientMessage, MapSettings, MissileMovementState, PlateState},
};

use super::WeaponMode;

#[derive(SystemParam)]
pub struct MissileInputWorld<'w> {
    gameplay_config: Res<'w, GameplayConfig>,
    collision_world: Res<'w, CollisionWorld>,
    plates: Res<'w, PlateState>,
    map_settings: Res<'w, MapSettings>,
}

pub fn input_missile_system(
    mut commands: Commands,
    mode: Res<WeaponMode>,
    mouse: Res<ButtonInput<MouseButton>>,
    input: Res<CameraInputState>,
    aim: Res<CameraAim>,
    local_players: Query<(), With<LocalPlayerMarker>>,
    to_server: Res<ClientToServerChannel>,
    asset_server: Res<AssetServer>,
    asset_set: Res<AssetSet>,
    lock: Res<LockOnTarget>,
    my_player_id: Res<MyPlayerId>,
    mut players: ResMut<PlayerMap>,
    local_player_info: Res<LocalPlayerInfo>,
    world: MissileInputWorld,
) {
    let MissileInputWorld {
        gameplay_config,
        collision_world,
        plates,
        map_settings,
    } = world;
    if local_player_info.is_dead || *mode != WeaponMode::Missile {
        return;
    }
    if input.released || input.suppress_fire || !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    if local_players.is_empty() {
        return;
    }

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

    let Some(info) = players.get_mut(&my_player_id.0) else {
        return;
    };
    let generation = info.generation;
    info.missiles = info.missiles.saturating_sub(1);
    let config = gameplay_config.missiles;
    let muzzle = aim.origin + aim.direction * MISSILE_SPAWN_OFFSET;
    let speed = map_settings.movement.missile_speed;
    let spread = if target.is_some() {
        config.launch_spread_degrees.to_radians()
    } else {
        0.0
    };
    let direction = clear_launch_direction(
        aim.direction,
        spread,
        muzzle,
        speed * 0.5,
        MISSILE_RADIUS,
        &collision_world,
        &plates.open_barrier_kinds,
        &mut rand::rng(),
    );
    to_server.send(ClientToServer::Send(ClientMessage::MissileShot(CMissileShot {
        generation,
        target,
        movement: MissileMovementState::from_velocity(muzzle.into(), direction * speed),
    })));
}
