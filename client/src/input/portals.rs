use bevy::{
    ecs::system::SystemParam,
    input::mouse::MouseButton,
    prelude::*,
    window::{CursorGrabMode, CursorOptions},
};

use crate::{
    audio::play_sound,
    cameras::{CameraViewMode, MainCameraMarker},
    config::AssetSet,
    network::{ClientToServer, ClientToServerChannel},
    players::{LocalPlayerInfo, LocalPlayerMarker, MyPlayerId, PlayerMap},
    portals::PortalMap,
};
use common::{
    config::GameplayConfig,
    math::direction_from_yaw_pitch,
    physics::{CollisionWorld, compute_portal_placement, portal_placement_overlaps},
    protocol::*,
};

// Which weapon the mouse buttons drive. Client-only presentation state, like
// `CameraViewMode`; the server just receives whichever shot message results.
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub enum WeaponMode {
    #[default]
    Projectile,
    MultiShot(String),
    Missile,
    Portal,
}

fn available_weapon_modes(
    map_settings: &MapSettings,
    portal_access: PortalAccess,
    has_multi_shot: bool,
    gameplay_config: &GameplayConfig,
) -> Vec<WeaponMode> {
    let mut modes = Vec::new();
    if map_settings.weapons.projectiles {
        if has_multi_shot {
            modes.extend(
                gameplay_config
                    .projectiles
                    .multi_shot
                    .allowed_patterns()
                    .iter()
                    .cloned()
                    .map(WeaponMode::MultiShot),
            );
        } else {
            modes.push(WeaponMode::Projectile);
        }
    }
    if map_settings.weapons.missiles {
        modes.push(WeaponMode::Missile);
    }
    if portal_access != PortalAccess::None {
        modes.push(WeaponMode::Portal);
    }
    modes
}

fn updated_weapon_mode(current: &WeaponMode, available: &[WeaponMode], advance: bool) -> Option<WeaponMode> {
    if available.is_empty() {
        return None;
    }
    let current_index = available.iter().position(|candidate| candidate == current);
    let index = if advance {
        current_index.map_or(0, |index| (index + 1) % available.len())
    } else {
        current_index.unwrap_or(0)
    };
    Some(available[index].clone())
}

#[derive(SystemParam)]
pub struct PortalInputWorld<'w> {
    time: Res<'w, Time>,
    collision_world: Option<Res<'w, CollisionWorld>>,
    map_layout: Option<Res<'w, MapLayout>>,
    portals: Res<'w, PortalMap>,
    gameplay_config: Res<'w, GameplayConfig>,
}

pub fn input_weapon_toggle_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    map_settings: Option<Res<MapSettings>>,
    portal_access: Option<Res<PortalAccess>>,
    my_player_id: Option<Res<MyPlayerId>>,
    players: Res<PlayerMap>,
    gameplay_config: Res<GameplayConfig>,
    mut mode: ResMut<WeaponMode>,
) {
    let (Some(map_settings), Some(portal_access)) = (map_settings, portal_access) else {
        return;
    };
    let has_multi_shot = my_player_id
        .as_deref()
        .and_then(|id| players.get(&id.0))
        .is_some_and(|info| info.power_up(PowerUpKind::MultiShot));
    let available = available_weapon_modes(&map_settings, *portal_access, has_multi_shot, &gameplay_config);
    if let Some(updated) = updated_weapon_mode(&mode, &available, keyboard.just_pressed(KeyCode::KeyQ))
        && *mode != updated
    {
        *mode = updated;
    }
}

// Portal-gun fire: both-access uses left=A and right=B; single-access uses
// left for its assigned end. Placement is predicted with the same shared check the server
// runs on the same static map data, so the feedback is immediate and never
// wrong: a valid aperture plays the fire sound and sends the shot, an
// invalid one (miss, doesn't fit, covers a fixture) dry-fires and sends
// nothing. The portal itself still only appears with `SPortalOpened`.
pub fn input_portal_system(
    mut commands: Commands,
    mode: Res<WeaponMode>,
    mouse: Res<ButtonInput<MouseButton>>,
    cursor_options: Single<&CursorOptions>,
    camera_query: Query<&Transform, (With<Camera3d>, With<MainCameraMarker>)>,
    local_player_query: Query<(&Position, &FaceYaw), With<LocalPlayerMarker>>,
    to_server: Res<ClientToServerChannel>,
    asset_server: Res<AssetServer>,
    asset_set: Res<AssetSet>,
    view_mode: Res<CameraViewMode>,
    mut local_player_info: ResMut<LocalPlayerInfo>,
    world: PortalInputWorld,
    portal_access: Option<Res<PortalAccess>>,
) {
    if *mode != WeaponMode::Portal || local_player_info.is_dead {
        return;
    }
    if cursor_options.grab_mode == CursorGrabMode::None {
        return;
    }
    let Some(access) = portal_access.as_deref().copied() else {
        return;
    };
    let end = match access {
        PortalAccess::None => return,
        PortalAccess::Single { end, .. } if mouse.just_pressed(MouseButton::Left) => end,
        PortalAccess::Single { .. } => return,
        PortalAccess::Both { .. } if mouse.just_pressed(MouseButton::Left) => PortalEnd::A,
        PortalAccess::Both { .. } if mouse.just_pressed(MouseButton::Right) => PortalEnd::B,
        PortalAccess::Both { .. } => return,
    };
    let Some(pair) = access.pair() else {
        return;
    };
    let now = world.time.elapsed_secs();
    if now - local_player_info.last_shot_time < world.gameplay_config.projectiles.cooldown_secs {
        play_sound(&mut commands, &asset_server, asset_set.player_sound("dry_fire"));
        return;
    }
    let Some((pos, face_yaw)) = local_player_query.iter().next() else {
        return;
    };
    let (Some(collision_world), Some(map_layout)) = (world.collision_world.as_deref(), world.map_layout.as_deref())
    else {
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

    let origin = Vec3::new(pos.x, pos.y + world.gameplay_config.player.eye_height(), pos.z);
    let direction = direction_from_yaw_pitch(face_yaw.0, pitch);
    let placement = compute_portal_placement(
        origin,
        direction,
        face_yaw.0,
        world.gameplay_config.portals.range,
        collision_world,
        map_layout,
    );
    let existing = world.portals.wire_portals();
    if placement.is_none_or(|placement| portal_placement_overlaps(&placement, pair, end, &existing)) {
        play_sound(&mut commands, &asset_server, asset_set.player_sound("dry_fire"));
        return;
    }

    local_player_info.last_shot_time = now;
    play_sound(&mut commands, &asset_server, asset_set.player_sound("fire"));
    let _ = to_server.send(ClientToServer::Send(ClientMessage::PortalShot(CPortalShot {
        end,
        face_yaw: face_yaw.0,
        face_pitch: pitch,
    })));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(projectiles: bool, missiles: bool, portals: PortalMode) -> MapSettings {
        MapSettings {
            skybox: "test".to_owned(),
            gravity: 25.0,
            low_gravity: 5.0,
            weapons: MapWeaponSettings {
                projectiles,
                missiles,
                portals,
            },
        }
    }

    #[test]
    fn available_weapons_follow_map_and_power_up_in_cycle_order() {
        let gameplay = GameplayConfig::load_default().expect("default gameplay config failed to load");
        let both = PortalAccess::Both { pair: PortalPairId(1) };
        assert_eq!(
            available_weapon_modes(&settings(true, true, PortalMode::Both), both, false, &gameplay),
            [WeaponMode::Projectile, WeaponMode::Missile, WeaponMode::Portal]
        );
        assert_eq!(
            available_weapon_modes(&settings(true, true, PortalMode::Both), both, true, &gameplay),
            [
                WeaponMode::MultiShot("star_4".to_owned()),
                WeaponMode::MultiShot("line_5".to_owned()),
                WeaponMode::Missile,
                WeaponMode::Portal,
            ]
        );
        assert!(
            available_weapon_modes(
                &settings(false, false, PortalMode::None),
                PortalAccess::None,
                false,
                &gameplay,
            )
            .is_empty()
        );
    }

    #[test]
    fn weapon_selection_wraps_and_recovers_from_power_up_changes() {
        let available = [WeaponMode::Projectile, WeaponMode::Missile, WeaponMode::Portal];
        assert_eq!(
            updated_weapon_mode(&WeaponMode::Projectile, &available, true),
            Some(WeaponMode::Missile)
        );
        assert_eq!(
            updated_weapon_mode(&WeaponMode::Portal, &available, true),
            Some(WeaponMode::Projectile)
        );
        assert_eq!(
            updated_weapon_mode(&WeaponMode::MultiShot("star_4".to_owned()), &available, false),
            Some(WeaponMode::Projectile)
        );
        assert_eq!(updated_weapon_mode(&WeaponMode::Portal, &[], true), None);
    }
}
