use bevy::prelude::*;

use super::{components::ClientAssets, quests::handle_quest_message};
use crate::{players::MyPlayerId, vfx::ExplosionRadii};
use common::{physics::CollisionWorld, protocol::*};

// Pre-bootstrap dispatcher: handles every message that can arrive before
// `MyPlayerId` is observable.
//
// `MyPlayerId` is inserted by the `Init` arm via `commands.insert_resource`,
// which is *deferred* — it doesn't become visible to the current system run.
// Anything that arrives in the same tick as `SInit` (and that's the common
// case for the login burst: `SInit` → `SQuestsAssigned` → `SSnapshot`) lands here.
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
        // Quest state events install durable per-player state with no
        // snapshot-side fallback, so they can't wait for bootstrap (and
        // don't need `MyPlayerId`: unicast, identity implicit). Everything
        // else is dropped — the next snapshot reconciles durable world state,
        // and one-shot cues are ephemeral by design.
        msg => {
            handle_quest_message(&mut client_assets.hud.quest_log, &mut client_assets.hud.banner, msg);
        }
    }
}
