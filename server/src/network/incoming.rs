use bevy::prelude::*;

use crate::{
    network::{ClientToServer, FeedAudience, FeedEvent, FromClientsChannel, emit_feed},
    players::PlayerInfo,
    quests::recheck_everyone_quests,
};
use common::protocol::PlayerMarker;

use super::routing::{ClientMessageContext, route_client_message};

// Registrations, messages, and disconnects share one channel so processing
// them directly in this loop preserves each connection's transport order.
pub(super) fn network_receive_system(
    mut commands: Commands,
    mut from_clients: ResMut<FromClientsChannel>,
    mut context: ClientMessageContext,
) {
    while let Ok((id, incoming)) = from_clients.try_recv() {
        match incoming {
            ClientToServer::Registration { to_client } => {
                debug!("player#{} registered", id.0);
                let entity = commands.spawn((PlayerMarker, id)).id();
                context.players.insert(id, PlayerInfo::new(entity, to_client));
            }
            ClientToServer::Message(message) => {
                route_client_message(&mut commands, id, message, &mut context);
            }
            ClientToServer::Disconnected => {
                let Some(player) = context.players.get(&id) else {
                    error!("received disconnect for unknown player#{}", id.0);
                    continue;
                };

                let who = context.players.describe(&id);
                let name = context.players.display_name(&id);
                let was_active = player.connection.logged_in;
                let entity = player.entity();
                context.players.remove(&id);
                let portal_access = context.portal_assignments.release(&id);
                if context.portals.remove_access(portal_access) {
                    *context.portal_set = context.portals.rebuild_set(&context.world.collision_world);
                }
                if let Some(entity) = entity {
                    commands.entity(entity).despawn();
                }

                debug!("{} disconnected (active: {})", who, was_active);
                if was_active {
                    emit_feed(
                        &context.players,
                        &context.world.server_gameplay_config.feed,
                        FeedAudience::Everyone,
                        FeedEvent::PlayerLeft { name },
                    );
                    recheck_everyone_quests(
                        &mut context.players,
                        &mut context.quest_board,
                        &context.quest_catalog,
                        &context.world.server_gameplay_config.feed,
                    );
                }
            }
        }
    }
}
