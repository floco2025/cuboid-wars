use bevy::prelude::*;
use common::{
    map::Carriers,
    physics::{CollisionWorld, PortalSet},
    protocol::PowerUpKind,
};

use super::{PortalAssignments, PortalMap};
use crate::players::PlayerMap;

pub fn unequipped_portals_cleanup_system(
    players: Res<PlayerMap>,
    assignments: Res<PortalAssignments>,
    mut portals: ResMut<PortalMap>,
    mut portal_set: ResMut<PortalSet>,
    collision_world: Res<CollisionWorld>,
    carriers: Res<Carriers>,
) {
    let mut changed = false;
    for (id, info) in players.iter() {
        if info.is_dead() || !info.has(PowerUpKind::PortalGun) {
            changed |= portals.remove_access(assignments.get(id));
        }
    }
    if changed {
        *portal_set = portals.rebuild_set(&collision_world, &carriers);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::players::{PlayerInfo, PowerUpState};
    use common::protocol::{BarrierKindTable, CarrierId, MapLayout, PlayerId, Portal, PortalEnd, PortalMode, Position};
    use tokio::sync::mpsc::unbounded_channel;

    #[test]
    fn gun_loss_removes_controlled_ends_and_preserves_assignments_and_equipped_partners() {
        for mode in [PortalMode::Single, PortalMode::Both] {
            for count in [1, 2] {
                for loss in ["expiry", "death", "eraser"] {
                    let mut app = App::new();
                    let mut assignments = PortalAssignments::new(mode);
                    let mut players = PlayerMap::default();
                    for id in 1..=count {
                        let (tx, _) = unbounded_channel();
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
                        .insert_resource(PortalSet::default())
                        .insert_resource(Carriers::default())
                        .insert_resource(CollisionWorld::from_map_layout(
                            &MapLayout::default(),
                            &BarrierKindTable::default(),
                        ))
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
}
