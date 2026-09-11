use std::f32::consts::PI;

use super::*;
use crate::{players::PlayerInfo, test_fixtures};
use common::protocol::{
    CarrierId, FaceMaterials, Health, Player, PlayerGeneration, PlayerMoveIntent, Position, TextureSettings, Wall,
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
        let collision = CollisionWorld::from_map_layout(&layout);
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
            .insert_resource(test_fixtures::asset_set())
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
        let ClientMessage::PortalShot(shot) = receiver.try_recv().expect("portal shot missing") else {
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
