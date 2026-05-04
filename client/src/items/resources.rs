use bevy::prelude::*;
use std::collections::HashMap;

use common::protocol::ItemId;

// Item information (client-side).
pub struct ItemInfo {
    pub entity: Entity,
}

// Map of all items (client-side source of truth).
#[derive(Resource, Default)]
pub struct ItemMap(pub HashMap<ItemId, ItemInfo>);
