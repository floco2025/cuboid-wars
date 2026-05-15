use bevy::prelude::*;

use super::{
    components::ClientAssets,
    players::{handle_quest_achieved_message, handle_quest_new_message},
};
use crate::players::MyPlayerId;
use common::{physics::CollisionWorld, protocol::*};

// Pre-bootstrap dispatcher: handles every message that can arrive before
// `MyPlayerId` is observable.
//
// `MyPlayerId` is inserted by the `Init` arm via `commands.insert_resource`,
// which is *deferred* — it doesn't become visible to the current system run.
// Anything that arrives in the same tick as `SInit` (and that's the common
// case for the login burst: `SInit` → `SQuestNew` → `SSnapshot`) lands here.
//
// Most messages are safe to drop: the next `SSnapshot` reconciles durable
// world state, and one-shot cues are ephemeral by design. The exceptions
// are per-client state events (`SQuestNew`, `SQuestAchieved`): they install
// durable per-player state with no snapshot-side fallback, so we must
// process them here regardless of bootstrap state. They don't depend on
// `MyPlayerId` anyway — they're unicast, identity is implicit.
pub fn handle_pre_bootstrap_message(msg: ServerMessage, commands: &mut Commands, client_assets: &mut ClientAssets) {
    match msg {
        ServerMessage::Init(init_msg) => {
            debug!("received Init: my_id={:?}", init_msg.id);
            commands.insert_resource(MyPlayerId(init_msg.id));
            let collision_world =
                CollisionWorld::from_map_layout(&init_msg.map_layout, &client_assets.barrier_kind_table);
            commands.insert_resource(init_msg.map_layout);
            commands.insert_resource(collision_world);
        }
        ServerMessage::QuestNew(quest_msg) => {
            handle_quest_new_message(
                commands,
                &mut client_assets.active_quests,
                &client_assets.client_settings,
                quest_msg,
            );
        }
        ServerMessage::QuestAchieved(quest_msg) => {
            handle_quest_achieved_message(
                commands,
                &mut client_assets.active_quests,
                &client_assets.client_settings,
                quest_msg,
            );
        }
        _ => {
            // Drop. Snapshots are self-healing (next tick reconciles), and
            // one-shot cues are ephemeral side effects.
        }
    }
}
