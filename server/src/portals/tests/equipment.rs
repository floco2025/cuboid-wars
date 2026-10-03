use super::*;
use crate::{
    config::RespawnConfig,
    players::{PlayerInfo, PowerUpState, erase_equipment_system},
};
use common::protocol::{CarrierId, PlayerId, Portal, PortalEnd, PortalMode, Position};
use crossbeam_channel::unbounded;

#[test]
fn gun_loss_removes_controlled_ends_and_preserves_assignments_and_equipped_partners() {
    for mode in [PortalMode::Auto, PortalMode::Both] {
        for count in [1, 2] {
            for loss in ["expiry", "death", "eraser"] {
                let mut app = App::new();
                let mut assignments = PortalAssignments::new(mode);
                let mut players = PlayerMap::default();
                for id in 1..=count {
                    let (tx, _) = unbounded();
                    let mut info = PlayerInfo::new(Entity::PLACEHOLDER, tx);
                    info.life.power_ups[PowerUpKind::PortalGun.index()] = PowerUpState::Permanent;
                    players.insert(PlayerId(id), info);
                    assignments.assign(PlayerId(id));
                }
                let access = assignments.get(&PlayerId(1));
                let mut portals = PortalMap::default();
                for id in 1..=count {
                    let access = assignments.get(&PlayerId(id));
                    for end in [PortalEnd::A, PortalEnd::B] {
                        if access.allows(end) {
                            portals.set(Portal {
                                pair: access.pair().expect("portal pair missing"),
                                end,
                                pos: Position::default(),
                                nx: 0.0,
                                ny: 0.0,
                                nz: 1.0,
                                yaw: 0.0,
                                carrier: CarrierId::WORLD,
                            });
                        }
                    }
                }
                let expected: Vec<_> = portals
                    .snapshot_portals()
                    .into_iter()
                    .filter(|portal| Some(portal.pair) != access.pair() || !access.allows(portal.end))
                    .collect();
                let info = players.get_mut(&PlayerId(1)).expect("player missing");
                match loss {
                    "expiry" => {
                        info.life.power_ups[PowerUpKind::PortalGun.index()] = PowerUpState::Timed(1.0);
                        info.tick_timers(1.0);
                    }
                    "death" => info.begin_respawn(2.0),
                    _ => {
                        info.erase_equipment();
                    }
                }
                app.insert_resource(players)
                    .insert_resource(assignments)
                    .insert_resource(portals)
                    .add_systems(Update, unequipped_portals_cleanup_system);
                app.update();
                assert_eq!(
                    app.world().resource::<PortalMap>().snapshot_portals(),
                    expected,
                    "{mode:?}, {count}, {loss}"
                );
                assert_eq!(app.world().resource::<PortalAssignments>().get(&PlayerId(1)), access);
            }
        }
    }
}

#[test]
fn an_eraser_closes_its_players_portals_keeps_an_always_held_gun_and_cues_once() {
    for carries_a_pickup in [false, true] {
        let mut always_active = [false; PowerUpKind::COUNT];
        always_active[PowerUpKind::PortalGun.index()] = true;
        let mut players = PlayerMap::new(RespawnConfig::default(), always_active);
        let (tx, rx) = unbounded();
        let mut info = PlayerInfo::new(Entity::PLACEHOLDER, tx);
        info.connection.logged_in = true;
        if carries_a_pickup {
            info.life.power_ups[PowerUpKind::Speed.index()] = PowerUpState::Permanent;
        }
        info.life.outcomes.erase_equipment = true;
        players.insert(PlayerId(1), info);
        let mut assignments = PortalAssignments::new(PortalMode::Both);
        assignments.assign(PlayerId(1));
        let pair = assignments.get(&PlayerId(1)).pair().expect("portal pair missing");
        let mut portals = PortalMap::default();
        for end in [PortalEnd::A, PortalEnd::B] {
            portals.set(Portal {
                pair,
                end,
                pos: Position::default(),
                nx: 0.0,
                ny: 0.0,
                nz: 1.0,
                yaw: 0.0,
                carrier: CarrierId::WORLD,
            });
        }
        let mut app = App::new();
        app.insert_resource(players)
            .insert_resource(assignments)
            .insert_resource(portals)
            .add_systems(
                Update,
                (
                    erased_portals_system,
                    erase_equipment_system,
                    unequipped_portals_cleanup_system,
                )
                    .chain(),
            );
        app.update();

        assert!(app.world().resource::<PortalMap>().snapshot_portals().is_empty());
        let info = app
            .world()
            .resource::<PlayerMap>()
            .get(&PlayerId(1))
            .expect("player missing");
        assert!(info.has(PowerUpKind::PortalGun), "the gun stays");
        assert!(!info.has(PowerUpKind::Speed), "the pickup goes");
        let cues = rx
            .try_iter()
            .filter(|message| matches!(message, ServerMessage::EquipmentErased(_)))
            .count();
        assert_eq!(cues, 1, "carries a pickup: {carries_a_pickup}");
    }
}
