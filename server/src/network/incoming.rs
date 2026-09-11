use bevy::prelude::*;
use tokio::sync::mpsc::error::TryRecvError;

use crate::{
    network::{FeedAudience, FeedEvent, emit_feed},
    players::PlayerInfo,
    quests::recheck_everyone_quests,
};
use common::protocol::{PlayerId, PlayerMarker};

use super::{
    resources::{ClientLinks, NewLinksChannel},
    routing::{ClientMessageContext, route_client_message},
};

// New links register before any link is drained, so a client's messages are
// only read once its `PlayerInfo` exists; a closed link is the client leaving.
pub(super) fn network_receive_system(
    mut commands: Commands,
    mut new_links: ResMut<NewLinksChannel>,
    mut links: ResMut<ClientLinks>,
    mut context: ClientMessageContext,
) {
    while let Ok(link) = new_links.try_recv() {
        let id = links.register(link.from_client);
        debug!("player#{} registered", id.0);
        let entity = commands.spawn((PlayerMarker, id)).id();
        context.players.insert(id, PlayerInfo::new(entity, link.to_client));
    }

    let mut closed = Vec::new();
    for (id, from_client) in links.iter_mut() {
        loop {
            match from_client.try_recv() {
                Ok(message) => route_client_message(&mut commands, id, message, &mut context),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    closed.push(id);
                    break;
                }
            }
        }
    }
    for id in closed {
        links.remove(id);
        disconnect_player(&mut commands, id, &mut context);
    }
}

fn disconnect_player(commands: &mut Commands, id: PlayerId, context: &mut ClientMessageContext) {
    let Some(player) = context.players.get(&id) else {
        error!("received disconnect for unknown player#{}", id.0);
        return;
    };

    let who = context.players.describe(&id);
    let name = context.players.display_name(&id);
    let was_active = player.connection.logged_in;
    let entity = player.entity();
    context
        .players
        .disconnect(&id, context.world.server_gameplay_config.player.respawn_secs);
    context.missiles.remove_shooter(id);
    let portal_access = context.portal_assignments.release(&id);
    context.portals.remove_access(portal_access);
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
