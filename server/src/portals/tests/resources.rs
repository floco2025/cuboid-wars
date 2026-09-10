use super::*;
use common::protocol::CarrierId;

fn portal(pair: u32, end: PortalEnd, x: f32) -> Portal {
    Portal {
        pair: PortalPairId(pair),
        end,
        pos: Position { x, y: 0.0, z: 0.0 },
        nx: 0.0,
        ny: 0.0,
        nz: 1.0,
        yaw: 0.0,
        carrier: CarrierId::WORLD,
    }
}

use common::protocol::Position;

#[test]
fn reshooting_an_end_replaces_that_end_only() {
    let mut map = PortalMap::default();
    assert!(map.set(portal(1, PortalEnd::A, 1.0)));
    assert!(map.set(portal(1, PortalEnd::B, 2.0)));
    assert!(map.set(portal(1, PortalEnd::A, 9.0)));
    assert!(!map.set(portal(1, PortalEnd::A, 9.0)));

    let portals = map.snapshot_portals();
    assert_eq!(portals.len(), 2);
    assert_eq!(portals[0].pos.x, 9.0);
    assert_eq!(portals[1].pos.x, 2.0);
}

#[test]
fn remove_both_access_drops_both_ends() {
    let mut map = PortalMap::default();
    map.set(portal(1, PortalEnd::A, 1.0));
    map.set(portal(1, PortalEnd::B, 2.0));
    map.set(portal(2, PortalEnd::A, 3.0));

    assert!(map.remove_access(PortalAccess::Both { pair: PortalPairId(1) }));
    assert!(!map.remove_access(PortalAccess::Both { pair: PortalPairId(1) }));
    let portals = map.snapshot_portals();
    assert_eq!(portals.len(), 1);
    assert_eq!(portals[0].pair, PortalPairId(2));
}

#[test]
fn remove_single_access_preserves_the_partner_end() {
    let mut map = PortalMap::default();
    map.set(portal(1, PortalEnd::A, 1.0));
    map.set(portal(1, PortalEnd::B, 2.0));

    assert!(map.remove_access(PortalAccess::Single {
        pair: PortalPairId(1),
        end: PortalEnd::A,
    }));
    assert_eq!(map.snapshot_portals(), vec![portal(1, PortalEnd::B, 2.0)]);
}

#[test]
fn snapshot_portals_sorts_by_pair_then_end() {
    let mut map = PortalMap::default();
    map.set(portal(2, PortalEnd::B, 4.0));
    map.set(portal(2, PortalEnd::A, 3.0));
    map.set(portal(1, PortalEnd::B, 2.0));

    let portals = map.snapshot_portals();
    let keys: Vec<(u32, PortalEnd)> = portals.iter().map(|p| (p.pair.0, p.end)).collect();
    assert_eq!(keys, vec![(1, PortalEnd::B), (2, PortalEnd::A), (2, PortalEnd::B)]);
}

const fn single(pair: u32, end: PortalEnd) -> PortalAccess {
    PortalAccess::Single {
        pair: PortalPairId(pair),
        end,
    }
}

const fn both(pair: u32) -> PortalAccess {
    PortalAccess::Both {
        pair: PortalPairId(pair),
    }
}

#[test]
fn single_assignments_pair_adjacent_slots_and_reuse_vacancies() {
    let mut assignments = PortalAssignments::new(PortalMode::Single);
    assert_eq!(assignments.assign(PlayerId(10)), both(1));
    assert_eq!(assignments.assign(PlayerId(11)), single(1, PortalEnd::B));
    assert_eq!(assignments.get(&PlayerId(10)), single(1, PortalEnd::A));
    assert_eq!(assignments.assign(PlayerId(12)), single(2, PortalEnd::A));

    assert_eq!(assignments.release(&PlayerId(11)), single(1, PortalEnd::B));
    assert_eq!(assignments.assign(PlayerId(13)), single(1, PortalEnd::B));
    assert_eq!(assignments.get(&PlayerId(10)), single(1, PortalEnd::A));
    assert_eq!(assignments.get(&PlayerId(11)), PortalAccess::None);
}

#[test]
fn a_lone_single_mode_player_keeps_their_pair_id_in_either_slot() {
    let mut assignments = PortalAssignments::new(PortalMode::Single);
    assignments.assign(PlayerId(10));
    assignments.assign(PlayerId(11));
    assignments.release(&PlayerId(10));
    assert_eq!(assignments.get(&PlayerId(11)), both(1));

    assignments.assign(PlayerId(12));
    assert_eq!(assignments.get(&PlayerId(11)), single(1, PortalEnd::B));
    assert_eq!(assignments.get(&PlayerId(12)), single(1, PortalEnd::A));
}

#[test]
fn release_reports_the_access_held_before_the_slot_empties() {
    let mut assignments = PortalAssignments::new(PortalMode::Single);
    assignments.assign(PlayerId(10));
    assignments.assign(PlayerId(11));
    assert_eq!(assignments.release(&PlayerId(11)), single(1, PortalEnd::B));
    assert_eq!(assignments.release(&PlayerId(10)), both(1));
    assert_eq!(assignments.release(&PlayerId(10)), PortalAccess::None);
}

#[test]
fn both_assignments_give_every_slot_its_own_pair() {
    let mut assignments = PortalAssignments::new(PortalMode::Both);
    assert_eq!(assignments.assign(PlayerId(1)), both(1));
    assert_eq!(assignments.assign(PlayerId(2)), both(2));
    assert_eq!(assignments.assign(PlayerId(1)), both(1));
}
