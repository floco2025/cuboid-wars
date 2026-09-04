use std::collections::HashMap;

use bevy::prelude::*;

use common::{
    physics::{CollisionWorld, PortalSet},
    protocol::{PlayerId, Portal, PortalAccess, PortalEnd, PortalMode, PortalPairId},
};

// Both ends of one pair. Re-shooting an end replaces just that end.
#[derive(Default, Clone, Copy)]
struct PortalPair {
    a: Option<Portal>,
    b: Option<Portal>,
}

impl PortalPair {
    const fn end_mut(&mut self, end: PortalEnd) -> &mut Option<Portal> {
        match end {
            PortalEnd::A => &mut self.a,
            PortalEnd::B => &mut self.b,
        }
    }
}

// Every placed portal end, keyed by pair. The authoritative store — the
// snapshot list and the traversal `PortalSet` are derived views. Portals
// survive their controller's death and leave with their disconnect.
#[derive(Resource, Default)]
pub struct PortalMap(HashMap<PortalPairId, PortalPair>);

impl PortalMap {
    pub fn set(&mut self, portal: Portal) -> bool {
        let slot = self.0.entry(portal.pair).or_default().end_mut(portal.end);
        if *slot == Some(portal) {
            return false;
        }
        *slot = Some(portal);
        true
    }

    // Returns true when the access controlled any portal to remove.
    pub fn remove_access(&mut self, access: PortalAccess) -> bool {
        match access {
            PortalAccess::None => false,
            PortalAccess::Both { pair } => self.0.remove(&pair).is_some(),
            PortalAccess::Single { pair, end } => {
                let Some(ends) = self.0.get_mut(&pair) else {
                    return false;
                };
                let changed = ends.end_mut(end).take().is_some();
                if ends.a.is_none() && ends.b.is_none() {
                    self.0.remove(&pair);
                }
                changed
            }
        }
    }

    // Sorted by (pair, end) so the encoded snapshot bytes are deterministic.
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

// Portal slots handed out at login under the map's fixed portal mode:
// `both` gives each slot its own pair, `single` pairs adjacent slots as ends
// A/B so two players share one pair. A freed slot is reused first, so a
// partner's replacement joins the same pair. A lone `single` player holds
// both ends of their pair — same pair id, so nothing they placed moves — and
// the ends split A/B again once the adjacent slot fills.
#[derive(Resource)]
pub struct PortalAssignments {
    mode: PortalMode,
    slots: Vec<Option<PlayerId>>,
}

impl PortalAssignments {
    #[must_use]
    pub const fn new(mode: PortalMode) -> Self {
        Self {
            mode,
            slots: Vec::new(),
        }
    }

    pub fn assign(&mut self, player: PlayerId) -> PortalAccess {
        if self.mode == PortalMode::None {
            return PortalAccess::None;
        }
        let slot = self.slot_of(&player).unwrap_or_else(|| {
            let slot = self.slots.iter().position(Option::is_none).unwrap_or_else(|| {
                self.slots.push(None);
                self.slots.len() - 1
            });
            self.slots[slot] = Some(player);
            slot
        });
        self.access(slot)
    }

    #[must_use]
    pub fn get(&self, player: &PlayerId) -> PortalAccess {
        self.slot_of(player)
            .map_or(PortalAccess::None, |slot| self.access(slot))
    }

    pub fn release(&mut self, player: &PlayerId) -> PortalAccess {
        let Some(slot) = self.slot_of(player) else {
            return PortalAccess::None;
        };
        // Read before the slot empties: that can make the partner solo, and
        // the leaver must report only what they held.
        let access = self.access(slot);
        self.slots[slot] = None;
        access
    }

    fn slot_of(&self, player: &PlayerId) -> Option<usize> {
        self.slots.iter().position(|slot| slot.as_ref() == Some(player))
    }

    // Occupied slots are exactly the logged-in players: assigned at login,
    // released at disconnect.
    fn is_solo(&self) -> bool {
        self.slots.iter().flatten().count() == 1
    }

    fn access(&self, slot: usize) -> PortalAccess {
        let pair = |index: usize| PortalPairId(u32::try_from(index + 1).expect("portal pair slot exceeds u32"));
        match self.mode {
            PortalMode::None => PortalAccess::None,
            PortalMode::Single if self.is_solo() => PortalAccess::Both { pair: pair(slot / 2) },
            PortalMode::Single => PortalAccess::Single {
                pair: pair(slot / 2),
                end: if slot.is_multiple_of(2) {
                    PortalEnd::A
                } else {
                    PortalEnd::B
                },
            },
            PortalMode::Both => PortalAccess::Both { pair: pair(slot) },
        }
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
    fn both_assignments_give_every_slot_its_own_pair_and_none_mode_gives_nothing() {
        let mut assignments = PortalAssignments::new(PortalMode::Both);
        assert_eq!(assignments.assign(PlayerId(1)), both(1));
        assert_eq!(assignments.assign(PlayerId(2)), both(2));
        assert_eq!(assignments.assign(PlayerId(1)), both(1));

        let mut none = PortalAssignments::new(PortalMode::None);
        assert_eq!(none.assign(PlayerId(1)), PortalAccess::None);
        assert_eq!(none.get(&PlayerId(1)), PortalAccess::None);
    }
}
