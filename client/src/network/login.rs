use bevy::prelude::*;

use super::{
    components::ClientAssets,
    quests::{handle_quest_completed_message, handle_quest_progress_message, handle_quests_assigned_message},
};
use crate::{players::MyPlayerId, vfx::ExplosionRadii};
use common::{physics::CollisionWorld, protocol::*};

// Pre-bootstrap dispatcher: handles every message that can arrive before
// `MyPlayerId` is observable.
//
// `MyPlayerId` is inserted by the `Init` arm via `commands.insert_resource`,
// which is *deferred* — it doesn't become visible to the current system run.
// Anything that arrives in the same tick as `SInit` (and that's the common
// case for the login burst: `SInit` → `SQuestsAssigned` → `SSnapshot`) lands here.
//
// Most messages are safe to drop: the next `SSnapshot` reconciles durable
// world state, and one-shot cues are ephemeral by design. The exceptions are
// the per-client quest state events (`SQuestsAssigned`, `SQuestProgress`,
// `SQuestCompleted`): they install durable per-player state with no
// snapshot-side fallback, so we must process them here regardless of bootstrap
// state. They don't depend on `MyPlayerId` anyway — they're unicast, identity
// is implicit.
pub fn handle_pre_bootstrap_message(msg: ServerMessage, commands: &mut Commands, client_assets: &mut ClientAssets) {
    match msg {
        ServerMessage::Init(init_msg) => {
            debug!("received Init: my_id=player#{}", init_msg.id.0);
            commands.insert_resource(MyPlayerId(init_msg.id));
            let collision_world =
                CollisionWorld::from_map_layout(&init_msg.map_layout, &client_assets.handles.barrier_kind_table);
            commands.insert_resource(init_msg.map_layout);
            commands.insert_resource(init_msg.map_settings);
            commands.insert_resource(collision_world);
            commands.insert_resource(ExplosionRadii {
                actors: init_msg.actor_explosion_radii.into_iter().collect(),
                player: init_msg.player_explosion_radius,
            });
        }
        ServerMessage::QuestsAssigned(quest_msg) => {
            handle_quests_assigned_message(
                &mut client_assets.hud.quest_log,
                &client_assets.handles.client_settings,
                &mut client_assets.hud.pending_banner,
                quest_msg,
            );
        }
        ServerMessage::QuestProgress(quest_msg) => {
            handle_quest_progress_message(&mut client_assets.hud.quest_log, quest_msg);
        }
        ServerMessage::QuestCompleted(quest_msg) => {
            handle_quest_completed_message(
                commands,
                &mut client_assets.hud.quest_log,
                &client_assets.handles.client_settings,
                &mut client_assets.hud.pending_banner,
                &client_assets.handles.asset_server,
                &client_assets.handles.asset_set,
                quest_msg,
            );
        }
        _ => {
            // Drop. Snapshots are self-healing (next tick reconciles), and
            // one-shot cues are ephemeral side effects.
        }
    }
}
