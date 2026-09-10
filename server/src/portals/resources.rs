use std::collections::HashMap;

use bevy::prelude::*;

use common::protocol::{PlayerId, Portal, PortalAccess, PortalEnd, PortalMode, PortalPairId};

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
// snapshot list and the traversal `PortalSet` are derived views.
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
#[path = "tests/resources.rs"]
mod tests;
