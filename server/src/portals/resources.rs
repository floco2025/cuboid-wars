use std::collections::HashMap;

use bevy::prelude::*;

use common::{
    physics::{CollisionWorld, PortalSet},
    protocol::{PlayerId, Portal, PortalEnd},
};

// Both ends a player owns. Re-shooting an end replaces just that end.
#[derive(Default, Clone, Copy)]
pub struct PortalPair {
    pub a: Option<Portal>,
    pub b: Option<Portal>,
}

// Every placed portal end, keyed by owner. The authoritative store — the
// snapshot list and the traversal `PortalSet` are derived views. Portals
// survive their owner's death and leave with their owner's disconnect.
#[derive(Resource, Default)]
pub struct PortalMap(HashMap<PlayerId, PortalPair>);

impl PortalMap {
    pub fn set(&mut self, portal: Portal) {
        let pair = self.0.entry(portal.owner).or_default();
        match portal.end {
            PortalEnd::A => pair.a = Some(portal),
            PortalEnd::B => pair.b = Some(portal),
        }
    }

    // Returns true when the owner had any portal to remove.
    pub fn remove_owner(&mut self, id: &PlayerId) -> bool {
        self.0.remove(id).is_some()
    }

    // Sorted by (owner, end) so the encoded snapshot bytes are deterministic.
    #[must_use]
    pub fn snapshot_portals(&self) -> Vec<Portal> {
        let mut portals: Vec<Portal> = self.0.values().flat_map(|pair| [pair.a, pair.b]).flatten().collect();
        portals.sort_by_key(|portal| (portal.owner.0, portal.end == PortalEnd::B));
        portals
    }

    #[must_use]
    pub fn rebuild_set(&self, collision_world: &CollisionWorld) -> PortalSet {
        PortalSet::rebuild(&self.snapshot_portals(), collision_world)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn portal(owner: u32, end: PortalEnd, x: f32) -> Portal {
        Portal {
            owner: PlayerId(owner),
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
        map.set(portal(1, PortalEnd::A, 1.0));
        map.set(portal(1, PortalEnd::B, 2.0));
        map.set(portal(1, PortalEnd::A, 9.0));

        let portals = map.snapshot_portals();
        assert_eq!(portals.len(), 2);
        assert_eq!(portals[0].pos.x, 9.0);
        assert_eq!(portals[1].pos.x, 2.0);
    }

    #[test]
    fn remove_owner_drops_both_ends() {
        let mut map = PortalMap::default();
        map.set(portal(1, PortalEnd::A, 1.0));
        map.set(portal(1, PortalEnd::B, 2.0));
        map.set(portal(2, PortalEnd::A, 3.0));

        assert!(map.remove_owner(&PlayerId(1)));
        assert!(!map.remove_owner(&PlayerId(1)));
        let portals = map.snapshot_portals();
        assert_eq!(portals.len(), 1);
        assert_eq!(portals[0].owner, PlayerId(2));
    }

    #[test]
    fn snapshot_portals_sorts_by_owner_then_end() {
        let mut map = PortalMap::default();
        map.set(portal(2, PortalEnd::B, 4.0));
        map.set(portal(2, PortalEnd::A, 3.0));
        map.set(portal(1, PortalEnd::B, 2.0));

        let portals = map.snapshot_portals();
        let keys: Vec<(u32, PortalEnd)> = portals.iter().map(|p| (p.owner.0, p.end)).collect();
        assert_eq!(keys, vec![(1, PortalEnd::B), (2, PortalEnd::A), (2, PortalEnd::B)]);
    }

    #[test]
    fn rebuild_set_pairs_only_complete_owners() {
        let world = CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default());
        let mut map = PortalMap::default();
        map.set(portal(1, PortalEnd::A, 1.0));
        assert!(map.rebuild_set(&world).is_empty());

        map.set(portal(1, PortalEnd::B, 2.0));
        assert!(!map.rebuild_set(&world).is_empty());
    }
}
