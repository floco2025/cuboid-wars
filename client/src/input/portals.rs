use bevy::{ecs::system::SystemParam, input::mouse::MouseButton, prelude::*};

use super::WeaponMode;
use crate::{
    audio::play_sound,
    cameras::{CameraAim, CameraInputState},
    config::AssetSet,
    network::{ClientToServer, ClientToServerChannel},
    players::{LocalPlayerInfo, LocalPlayerMarker, PlayerMap},
    portals::PortalMap,
};
use common::{
    config::GameplayConfig,
    map::Carriers,
    physics::{
        CollisionWorld, PortalPlacement, PortalPlacementFailure, compute_portal_placement, portal_placement_overlaps,
    },
    protocol::{
        CPortalShot, ClientMessage, MapLayout, MapSettings, PlateState, PlayerId, Portal, PortalAccess, PortalEnd,
        PortalPairId, PortalShotResult,
    },
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
    players: Res<'w, PlayerMap>,
    gameplay_config: Res<'w, GameplayConfig>,
}

pub fn input_portal_system(
    mut commands: Commands,
    mode: Res<WeaponMode>,
    mouse: Res<ButtonInput<MouseButton>>,
    input: Res<CameraInputState>,
    aim: Res<CameraAim>,
    local_player_query: Query<&PlayerId, With<LocalPlayerMarker>>,
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
    let Ok(id) = local_player_query.single() else {
        return;
    };
    let Some(player) = world.players.get(id) else {
        return;
    };
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
    let Some(result) = portal_shot_result(placement, pair, end, &existing, &world.carriers) else {
        play_sound(&mut commands, &asset_server, asset_set.player_sound("dry_fire"));
        return;
    };

    local_player_info.last_shot_time = now;
    to_server.send(ClientToServer::Send(ClientMessage::PortalShot(CPortalShot {
        generation: player.generation,
        result,
    })));
}

fn portal_shot_result(
    placement: Result<PortalPlacement, PortalPlacementFailure>,
    pair: PortalPairId,
    end: PortalEnd,
    existing: &[Portal],
    carriers: &Carriers,
) -> Option<PortalShotResult> {
    match placement {
        Ok(placement) => {
            let portal = placement.portal(pair, end, carriers);
            (!portal_placement_overlaps(&portal, existing, carriers)).then_some(PortalShotResult::Placed(portal))
        }
        Err(PortalPlacementFailure::IncompatibleMaterial(impact)) => {
            Some(PortalShotResult::Fizzled(impact.portal(pair, end, carriers)))
        }
        Err(PortalPlacementFailure::InvalidPlacement) => None,
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::PI;

    use super::*;
    use crate::{players::PlayerInfo, test_fixtures};
    use common::protocol::{
        BarrierKindTable, CarrierId, FaceMaterials, Health, Player, PlayerGeneration, PlayerMoveIntent, Position,
        TextureSettings, Wall,
    };
    use tokio::sync::mpsc::unbounded_channel;

    #[test]
    fn firing_sends_the_client_resolved_geometry_and_current_body_generation() {
        for portalable in [true, false] {
            let mut app = App::new();
            app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()));
            let layout = MapLayout {
                walls: vec![Wall {
                    x1: -5.0,
                    x2: 5.0,
                    z1: 0.0,
                    z2: 0.0,
                    y: 0.0,
                    height: 5.0,
                    width: 0.3,
                    level: 0,
                    carrier: CarrierId::WORLD,
                }],
                wall_materials: vec![FaceMaterials::uniform("surface")],
                ..default()
            };
            let mut settings = test_fixtures::map_settings();
            settings.textures = [("surface".to_owned(), TextureSettings { portalable })].into();
            let collision = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
            let id = PlayerId(7);
            let entity = app.world_mut().spawn((id, LocalPlayerMarker)).id();
            let mut player = Player::new(
                "Player".into(),
                Position::default(),
                PlayerMoveIntent::Idle,
                0.0,
                0,
                Health(100.0),
            );
            player.generation = PlayerGeneration(4);
            let mut players = PlayerMap::default();
            players.insert(id, PlayerInfo::from_snapshot(entity, &player, 0));
            let (sender, mut receiver) = unbounded_channel();
            app.insert_resource(players)
                .insert_resource(ClientToServerChannel::new(sender))
                .insert_resource(WeaponMode::Portal)
                .insert_resource(PortalAccess::Both { pair: PortalPairId(3) })
                .insert_resource(CameraAim {
                    origin: Vec3::new(2.0, 1.6, 3.0),
                    direction: Vec3::NEG_Z,
                    yaw: PI,
                    ..default()
                })
                .insert_resource(AssetSet::load_default().expect("asset config is invalid"))
                .insert_resource(test_fixtures::gameplay_config())
                .insert_resource(collision)
                .insert_resource(layout)
                .insert_resource(settings)
                .init_resource::<Time>()
                .init_resource::<CameraInputState>()
                .init_resource::<Carriers>()
                .init_resource::<PortalMap>()
                .init_resource::<PlateState>()
                .init_resource::<LocalPlayerInfo>()
                .init_resource::<ButtonInput<MouseButton>>()
                .add_systems(Update, input_portal_system);
            app.world_mut()
                .resource_mut::<ButtonInput<MouseButton>>()
                .press(MouseButton::Left);
            app.update();
            let ClientToServer::Send(ClientMessage::PortalShot(shot)) =
                receiver.try_recv().expect("portal shot missing")
            else {
                panic!("portal input sent a different message");
            };
            assert_eq!(shot.generation, PlayerGeneration(4));
            assert_eq!(matches!(shot.result, PortalShotResult::Placed(_)), portalable);
            let portal = shot.result.portal();
            assert_eq!(portal.pair, PortalPairId(3));
            assert_eq!(portal.end, PortalEnd::A);
            assert_eq!(portal.carrier, CarrierId::WORLD);
            assert!((Vec3::from(portal.pos) - Vec3::new(2.0, 1.6, 0.15)).length() < 1e-4);
            assert!(receiver.try_recv().is_err(), "shot sent twice");
        }
    }
}
