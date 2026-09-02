use std::collections::HashMap;

use bevy::prelude::*;

use common::{
    physics::{CollisionWorld, PortalSet},
    protocol::{PlayerId, Portal, PortalAccess, PortalEnd, PortalMode, PortalPairId},
};

#[derive(Default, Clone, Copy)]
struct PortalPair {
    a: Option<Portal>,
    b: Option<Portal>,
}

#[derive(Resource, Default)]
pub struct PortalMap(HashMap<PortalPairId, PortalPair>);

impl PortalMap {
    pub fn set(&mut self, portal: Portal) -> bool {
        let pair = self.0.entry(portal.pair).or_default();
        let slot = match portal.end {
            PortalEnd::A => &mut pair.a,
            PortalEnd::B => &mut pair.b,
        };
        if *slot == Some(portal) {
            return false;
        }
        *slot = Some(portal);
        true
    }

    pub fn remove_access(&mut self, access: PortalAccess) -> bool {
        let Some(pair_id) = access.pair() else {
            return false;
        };
        if matches!(access, PortalAccess::Both { .. }) {
            return self.0.remove(&pair_id).is_some();
        }
        let Some(pair) = self.0.get_mut(&pair_id) else {
            return false;
        };
        let slot = match access {
            PortalAccess::Single { end: PortalEnd::A, .. } => &mut pair.a,
            PortalAccess::Single { end: PortalEnd::B, .. } => &mut pair.b,
            PortalAccess::None | PortalAccess::Both { .. } => unreachable!(),
        };
        let changed = slot.take().is_some();
        if pair.a.is_none() && pair.b.is_none() {
            self.0.remove(&pair_id);
        }
        changed
    }

    #[must_use]
    pub fn snapshot_portals(&self) -> Vec<Portal> {
        let mut portals: Vec<Portal> = self.0.values().flat_map(|pair| [pair.a, pair.b]).flatten().collect();
        portals.sort_by_key(|portal| (portal.pair.0, portal.end == PortalEnd::B));
        portals
    }

    #[must_use]
    pub fn rebuild_set(&self, collision_world: &CollisionWorld) -> PortalSet {
        PortalSet::rebuild(&self.snapshot_portals(), collision_world)
    }
}

#[derive(Clone, Copy)]
struct Assignment {
    slot: usize,
    access: PortalAccess,
}

#[derive(Resource, Default)]
pub struct PortalAssignments {
    slots: Vec<Option<PlayerId>>,
    assignments: HashMap<PlayerId, Assignment>,
}

impl PortalAssignments {
    pub fn assign(&mut self, player: PlayerId, mode: PortalMode) -> PortalAccess {
        if let Some(assignment) = self.assignments.get(&player) {
            return assignment.access;
        }
        if mode == PortalMode::None {
            return PortalAccess::None;
        }
        let slot = self.slots.iter().position(Option::is_none).unwrap_or_else(|| {
            self.slots.push(None);
            self.slots.len() - 1
        });
        self.slots[slot] = Some(player);
        let access = match mode {
            PortalMode::None => unreachable!(),
            PortalMode::Single => PortalAccess::Single {
                pair: PortalPairId(u32::try_from(slot / 2 + 1).expect("portal pair slot exceeds u32")),
                end: if slot % 2 == 0 { PortalEnd::A } else { PortalEnd::B },
            },
            PortalMode::Both => PortalAccess::Both {
                pair: PortalPairId(u32::try_from(slot + 1).expect("portal pair slot exceeds u32")),
            },
        };
        self.assignments.insert(player, Assignment { slot, access });
        access
    }

    #[must_use]
    pub fn get(&self, player: &PlayerId) -> PortalAccess {
        self.assignments
            .get(player)
            .map_or(PortalAccess::None, |assignment| assignment.access)
    }

    pub fn release(&mut self, player: &PlayerId) -> PortalAccess {
        let Some(assignment) = self.assignments.remove(player) else {
            return PortalAccess::None;
        };
        self.slots[assignment.slot] = None;
        assignment.access
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn portal(pair: u32, end: PortalEnd, x: f32) -> Portal {
        Portal {
            pair: PortalPairId(pair),
            end,
            pos: Position { x, y: 0.0, z: 0.0 },
            nx: 0.0,
            ny: 0.0,
            nz: 1.0,
            yaw: 0.0,
        }
    }

    use common::protocol::{BarrierKindTable, MapLayout, Position};

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

    #[test]
    fn rebuild_set_pairs_only_complete_pairs() {
        let world = CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default());
        let mut map = PortalMap::default();
        map.set(portal(1, PortalEnd::A, 1.0));
        assert!(map.rebuild_set(&world).is_empty());

        map.set(portal(1, PortalEnd::B, 2.0));
        assert!(!map.rebuild_set(&world).is_empty());
    }

    #[test]
    fn single_assignments_pair_adjacent_slots_and_reuse_vacancies() {
        let mut assignments = PortalAssignments::default();
        let first = assignments.assign(PlayerId(10), PortalMode::Single);
        let second = assignments.assign(PlayerId(11), PortalMode::Single);
        let third = assignments.assign(PlayerId(12), PortalMode::Single);
        assert_eq!(
            first,
            PortalAccess::Single {
                pair: PortalPairId(1),
                end: PortalEnd::A
            }
        );
        assert_eq!(
            second,
            PortalAccess::Single {
                pair: PortalPairId(1),
                end: PortalEnd::B
            }
        );
        assert_eq!(
            third,
            PortalAccess::Single {
                pair: PortalPairId(2),
                end: PortalEnd::A
            }
        );

        assert_eq!(assignments.release(&PlayerId(11)), second);
        assert_eq!(assignments.assign(PlayerId(13), PortalMode::Single), second);
        assert_eq!(assignments.get(&PlayerId(10)), first);
    }
}
