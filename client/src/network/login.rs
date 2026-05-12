use bevy::prelude::*;

use crate::players::MyPlayerId;
use common::{physics::CollisionWorld, protocol::*};

// Handle Init message when not yet logged in - stores player ID and map layout.
// All player spawning (including the local player) happens via the snapshot
// diff in `sync_players` once the first `SUpdate` arrives.
pub fn handle_init_message(msg: ServerMessage, commands: &mut Commands, barrier_kind_table: &BarrierKindTable) {
    if let ServerMessage::Init(init_msg) = msg {
        debug!("received Init: my_id={:?}", init_msg.id);

        commands.insert_resource(MyPlayerId(init_msg.id));

        let collision_world = CollisionWorld::from_map_layout(&init_msg.map_layout, barrier_kind_table);
        commands.insert_resource(init_msg.map_layout);
        commands.insert_resource(collision_world);
    }
}
