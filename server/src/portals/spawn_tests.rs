use std::time::Duration;

use bevy::{ecs::system::SystemState, prelude::*};
use common::{
    map::Carriers,
    physics::{CollisionWorld, PortalSet},
    protocol::*,
};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use super::{PortalAssignments, PortalMap, handle_portal_shot_message};
use crate::{
    config::ServerGameplayConfig,
    map::MapConfig,
    network::{ServerToClient, SharedWorld},
    players::{PlayerInfo, PlayerMap, PowerUpState},
    test_geometry::geometry,
};

struct Fixture {
    world: World,
    players: PlayerMap,
    assignments: PortalAssignments,
    portals: PortalMap,
    set: PortalSet,
    time: Time,
    receivers: Vec<UnboundedReceiver<ServerToClient>>,
}

impl Fixture {
    fn new(mode: PortalMode) -> Self {
        let config = ServerGameplayConfig::load_default().expect("gameplay config is invalid");
        let settings = config.maps["hotel"].settings.clone();
        let layout = MapLayout {
            carriers: vec![Carrier {
                parent: CarrierId::WORLD,
                level: 0,
                levels: 1,
                from: Position::default(),
                to: Position {
                    x: 12.0,
                    y: 4.0,
                    z: 0.0,
                },
                travel_ticks: 30,
                pause_ticks: 5,
                phase_ticks: 0,
            }],
            ..default()
        };
        let mut world = World::new();
        world.insert_resource(CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default()));
        let mut carriers = Carriers::from_layout(&layout);
        carriers.advance(17);
        world.insert_resource(carriers);
        world.insert_resource(layout.clone());
        world.insert_resource(settings.clone());
        world.insert_resource(config.gameplay_config());
        world.insert_resource(MapConfig::for_grid(Vec::new(), geometry(1, 1)));
        world.insert_resource(WorldBootstrap {
            network: Default::default(),
            gameplay: config.gameplay_bootstrap(),
            map: MapBootstrap {
                missile_air_grids: Vec::new(),
                layout,
                settings,
                items: MapItems(Vec::new()),
            },
        });
        world.insert_resource(config);
        world.init_resource::<ServerTick>();
        let mut players = PlayerMap::default();
        let mut assignments = PortalAssignments::new(mode);
        let mut receivers = Vec::new();
        for id in [PlayerId(1), PlayerId(2)] {
            let entity = world.spawn_empty().id();
            let (sender, receiver) = unbounded_channel();
            let mut info = PlayerInfo::new(entity, sender);
            info.connection.logged_in = true;
            info.session.generation = PlayerGeneration(3);
            info.life.power_ups[PowerUpKind::PortalGun.index()] = PowerUpState::Permanent;
            players.insert(id, info);
            assignments.assign(id);
            receivers.push(receiver);
        }
        let mut time = Time::default();
        time.advance_by(Duration::from_secs(1));
        Self {
            world,
            players,
            assignments,
            portals: PortalMap::default(),
            set: PortalSet::default(),
            time,
            receivers,
        }
    }

    fn portal(&self, id: PlayerId, end: PortalEnd, x: f32) -> Portal {
        Portal {
            pair: self.assignments.get(&id).pair().expect("portal pair missing"),
            end,
            pos: Position { x, y: 1.5, z: 0.0 },
            nx: 0.0,
            ny: 0.0,
            nz: 1.0,
            yaw: 0.0,
            carrier: CarrierId(1),
        }
    }

    fn shoot(&mut self, id: PlayerId, generation: PlayerGeneration, result: PortalShotResult) {
        let mut params = SystemState::<SharedWorld>::new(&mut self.world);
        let world = params.get(&self.world).expect("portal handler parameters are invalid");
        handle_portal_shot_message(
            id,
            &CPortalShot { generation, result },
            &mut self.players,
            &self.time,
            &world,
            &self.assignments,
            &mut self.portals,
            &mut self.set,
        );
    }

    fn assert_opened(&mut self, shooter: PlayerId, portal: Portal) {
        for receiver in &mut self.receivers {
            let ServerToClient::Send(ServerMessage::PortalOpened(message)) =
                receiver.try_recv().expect("placement cue missing")
            else {
                panic!("placement sent a different cue");
            };
            assert_eq!(message.shooter, shooter);
            assert_eq!(message.portal, portal);
            assert!(receiver.try_recv().is_err(), "duplicate placement cue");
        }
    }
}

#[test]
fn placement_keeps_client_geometry_without_a_server_eye_ray_or_backing_surface() {
    let mut fixture = Fixture::new(PortalMode::Both);
    let id = PlayerId(1);
    let portal = fixture.portal(id, PortalEnd::A, 2.0);
    fixture.shoot(id, PlayerGeneration(3), PortalShotResult::Placed(portal));
    assert_eq!(fixture.portals.snapshot_portals(), vec![portal]);
    fixture.assert_opened(id, portal);
    fixture.time.advance_by(Duration::from_secs(1));
    let exit = fixture.portal(id, PortalEnd::B, 8.0);
    fixture.shoot(id, PlayerGeneration(3), PortalShotResult::Placed(exit));
    fixture.assert_opened(id, exit);
    assert!(!fixture.set.is_empty(), "accepted pair did not rebuild traversal");
}

#[test]
fn competing_placements_keep_the_first_portal_and_an_owner_can_replace_its_own_end() {
    let mut fixture = Fixture::new(PortalMode::Both);
    let first = fixture.portal(PlayerId(1), PortalEnd::A, 2.0);
    let second = fixture.portal(PlayerId(2), PortalEnd::A, 2.1);
    fixture.shoot(PlayerId(1), PlayerGeneration(3), PortalShotResult::Placed(first));
    fixture.assert_opened(PlayerId(1), first);
    fixture.shoot(PlayerId(2), PlayerGeneration(3), PortalShotResult::Placed(second));
    assert_eq!(fixture.portals.snapshot_portals(), vec![first]);
    assert!(
        fixture
            .receivers
            .iter_mut()
            .all(|receiver| receiver.try_recv().is_err())
    );
    fixture.time.advance_by(Duration::from_secs(1));
    let moved = Portal {
        pos: second.pos,
        ..first
    };
    fixture.shoot(PlayerId(1), PlayerGeneration(3), PortalShotResult::Placed(moved));
    fixture.assert_opened(PlayerId(1), moved);
    assert_eq!(fixture.portals.snapshot_portals(), vec![moved]);
}

#[test]
fn reported_fizzle_is_relayed_once_and_does_not_replace_a_portal() {
    let mut fixture = Fixture::new(PortalMode::Both);
    let id = PlayerId(1);
    let existing = fixture.portal(id, PortalEnd::A, 2.0);
    fixture.portals.set(existing);
    let impact = fixture.portal(id, PortalEnd::A, 4.0);
    for _ in 0..2 {
        fixture.shoot(id, PlayerGeneration(3), PortalShotResult::Fizzled(impact));
    }
    for receiver in &mut fixture.receivers {
        let ServerToClient::Send(ServerMessage::PortalFizzled(message)) = receiver.try_recv().expect("fizzle missing")
        else {
            panic!("fizzle sent a different cue");
        };
        assert_eq!(message.shooter, id);
        assert_eq!(message.impact, impact);
        assert!(receiver.try_recv().is_err(), "cooldown allowed a duplicate cue");
    }
    assert_eq!(fixture.portals.snapshot_portals(), vec![existing]);
}

#[test]
fn assignments_and_body_lifecycle_guard_placements() {
    let mut fixture = Fixture::new(PortalMode::Single);
    let id = PlayerId(1);
    let portal = fixture.portal(id, PortalEnd::A, 2.0);
    for invalid in [
        Portal {
            end: PortalEnd::B,
            ..portal
        },
        Portal {
            pair: PortalPairId(999),
            ..portal
        },
    ] {
        fixture.shoot(id, PlayerGeneration(3), PortalShotResult::Placed(invalid));
    }
    fixture.shoot(id, PlayerGeneration(2), PortalShotResult::Placed(portal));
    let player = fixture.players.get_mut(&id).expect("player missing");
    player.life.power_ups[PowerUpKind::PortalGun.index()] = PowerUpState::Inactive;
    fixture.shoot(id, PlayerGeneration(3), PortalShotResult::Placed(portal));
    let player = fixture.players.get_mut(&id).expect("player missing");
    player.life.power_ups[PowerUpKind::PortalGun.index()] = PowerUpState::Permanent;
    player.begin_respawn(1.0);
    fixture.shoot(id, PlayerGeneration(3), PortalShotResult::Placed(portal));
    assert!(fixture.portals.snapshot_portals().is_empty());
    assert!(
        fixture
            .receivers
            .iter_mut()
            .all(|receiver| receiver.try_recv().is_err())
    );
}
