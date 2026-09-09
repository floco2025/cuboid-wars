use bevy::{ecs::system::SystemParam, input::mouse::MouseButton, prelude::*};

use super::WeaponMode;
use crate::{
    audio::play_sound,
    cameras::{CameraAim, CameraInputState},
    config::AssetSet,
    network::{ClientToServer, ClientToServerChannel},
    players::{LocalPlayerInfo, LocalPlayerMarker},
    portals::PortalMap,
};
use common::{
    config::GameplayConfig,
    map::Carriers,
    physics::{CollisionWorld, PortalPlacementFailure, compute_portal_placement, portal_placement_overlaps},
    protocol::{CPortalShot, ClientMessage, MapLayout, MapSettings, PlateState, PortalAccess, PortalEnd, Position},
};

// The mouse button that places each portal end: a `single` slot places its
// assigned end with the left button, a `both` slot places A left and B right.
pub(super) fn portal_end_for_button(access: PortalAccess, button: MouseButton) -> Option<PortalEnd> {
    match (access, button) {
        (PortalAccess::Single { end, .. }, MouseButton::Left) => Some(end),
        (PortalAccess::Both { .. }, MouseButton::Left) => Some(PortalEnd::A),
        (PortalAccess::Both { .. }, MouseButton::Right) => Some(PortalEnd::B),
        _ => None,
    }
}

#[derive(SystemParam)]
pub struct PortalInputWorld<'w> {
    time: Res<'w, Time>,
    collision_world: Res<'w, CollisionWorld>,
    carriers: Res<'w, Carriers>,
    map_layout: Res<'w, MapLayout>,
    map_settings: Res<'w, MapSettings>,
    plates: Res<'w, PlateState>,
    portals: Res<'w, PortalMap>,
    gameplay_config: Res<'w, GameplayConfig>,
}

pub fn input_portal_system(
    mut commands: Commands,
    mode: Res<WeaponMode>,
    mouse: Res<ButtonInput<MouseButton>>,
    input: Res<CameraInputState>,
    aim: Res<CameraAim>,
    local_player_query: Query<&Position, With<LocalPlayerMarker>>,
    to_server: Res<ClientToServerChannel>,
    asset_server: Res<AssetServer>,
    asset_set: Res<AssetSet>,
    mut local_player_info: ResMut<LocalPlayerInfo>,
    world: PortalInputWorld,
    portal_access: Res<PortalAccess>,
) {
    if *mode != WeaponMode::Portal || local_player_info.is_dead {
        return;
    }
    if input.released || input.suppress_fire {
        return;
    }
    let access = *portal_access;
    let Some(end) = [MouseButton::Left, MouseButton::Right]
        .into_iter()
        .find(|button| mouse.just_pressed(*button))
        .and_then(|button| portal_end_for_button(access, button))
    else {
        return;
    };
    let Some(pair) = access.pair() else {
        return;
    };
    let now = world.time.elapsed_secs();
    if now - local_player_info.last_shot_time < world.gameplay_config.projectiles.cooldown_secs {
        play_sound(&mut commands, &asset_server, asset_set.player_sound("dry_fire"));
        return;
    }
    if local_player_query.is_empty() {
        return;
    }
    let pitch = aim.pitch;
    let origin = aim.origin;
    let direction = aim.direction;
    let placement = compute_portal_placement(
        origin,
        direction,
        aim.yaw,
        world.gameplay_config.portals.range,
        &world.collision_world,
        &world.map_layout,
        &world.carriers,
        &world.plates.open_barrier_kinds,
        &world.map_settings.textures,
    );
    let existing = world.portals.wire_portals();
    let dry_fire = match placement {
        Ok(placement) => portal_placement_overlaps(&placement, pair, end, &existing, &world.carriers),
        Err(PortalPlacementFailure::IncompatibleMaterial(_)) => false,
        Err(PortalPlacementFailure::InvalidPlacement) => true,
    };
    if dry_fire {
        play_sound(&mut commands, &asset_server, asset_set.player_sound("dry_fire"));
        return;
    }

    local_player_info.last_shot_time = now;
    let _ = to_server.send(ClientToServer::Send(ClientMessage::PortalShot(CPortalShot {
        end,
        face_yaw: aim.yaw,
        face_pitch: pitch,
    })));
}
