use std::collections::HashMap;

use bevy::prelude::*;

use common::protocol::{Portal, PortalEnd, PortalPairId};

pub struct PortalInfo {
    pub entity: Entity,
    // The wire value the entity was spawned from; a changed value means the
    // end was re-shot and the visual must move.
    pub portal: Portal,
}

#[derive(Resource, Default)]
pub struct PortalMap(HashMap<(PortalPairId, PortalEnd), PortalInfo>);

impl PortalMap {
    #[must_use]
    pub fn get(&self, key: &(PortalPairId, PortalEnd)) -> Option<&PortalInfo> {
        self.0.get(key)
    }

    pub fn insert(&mut self, key: (PortalPairId, PortalEnd), info: PortalInfo) {
        self.0.insert(key, info);
    }

    pub fn retain(&mut self, f: impl FnMut(&(PortalPairId, PortalEnd), &mut PortalInfo) -> bool) {
        self.0.retain(f);
    }

    // The stored wire values, for rebuilding the shared `PortalSet`.
    #[must_use]
    pub fn wire_portals(&self) -> Vec<Portal> {
        let mut portals: Vec<_> = self.0.values().map(|info| info.portal).collect();
        portals.sort_by_key(|portal| (portal.pair.0, portal.end == PortalEnd::B));
        portals
    }
}
