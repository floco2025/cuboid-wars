use bevy::prelude::*;
use crossbeam_channel::{Sender, TryRecvError, unbounded};
use renet::ServerEvent;

use crate::{
    network::{FeedAudience, FeedEvent, emit_feed},
    players::{PlayerInfo, PlayerMap},
    quests::recheck_everyone_quests,
};
use common::{
    network::{CHANNELS, decode_message},
    protocol::{ClientMessage, PlayerId, PlayerMarker, ServerMessage},
};

use super::{
    links::{ClientLinks, LinkSource, Listener, LocalLink},
    routing::{ClientMessageContext, route_client_message},
};

// Registers the host's own client before the app runs, so its messages are
// only read once its `PlayerInfo` exists.
pub fn register_local(world: &mut World, link: LocalLink) -> PlayerId {
    let id = world
        .resource_mut::<ClientLinks>()
        .register(LinkSource::Local(link.from_client));
    let entity = world.spawn((PlayerMarker, id)).id();
    world
        .resource_mut::<PlayerMap>()
        .insert(id, PlayerInfo::new(entity, link.to_client));
    id
}

// A remote client's messages are only read once its `PlayerInfo` exists:
// netcode reports a connection before its first payload. A closed queue or a
// netcode disconnect is the client leaving.
pub(super) fn network_receive_system(
    mut commands: Commands,
    mut links: ResMut<ClientLinks>,
    mut listener: Option<ResMut<Listener>>,
    mut context: ClientMessageContext,
) {
    if let Some(listener) = listener.as_deref_mut() {
        if let Err(error) = listener.poll() {
            error!("listener failed to poll: {error:#}");
        }
        while let Some(event) = listener.server.get_event() {
            match event {
                ServerEvent::ClientConnected { client_id } => {
                    let (to_client, outgoing) = unbounded();
                    register(
                        &mut commands,
                        &mut links,
                        &mut context,
                        LinkSource::Remote { client_id, outgoing },
                        to_client,
                    );
                }
                ServerEvent::ClientDisconnected { client_id, reason } => {
                    let Some(id) = links.player_of(client_id) else {
                        continue;
                    };
                    debug!("player#{} left: {reason}", id.0);
                    links.remove(id);
                    disconnect_player(&mut commands, id, &mut context);
                }
            }
        }
    }

    let mut closed = Vec::new();
    for (id, source) in links.iter_mut() {
        match source {
            LinkSource::Local(from_client) => loop {
                match from_client.try_recv() {
                    Ok(message) => route_client_message(&mut commands, id, message, &mut context),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        closed.push(id);
                        break;
                    }
                }
            },
            LinkSource::Remote { client_id, .. } => {
                let Some(listener) = listener.as_deref_mut() else {
                    continue;
                };
                for channel in CHANNELS {
                    while let Some(bytes) = listener.server.receive_message(*client_id, channel) {
                        match decode_message::<ClientMessage>(&bytes) {
                            Ok(message) => route_client_message(&mut commands, id, message, &mut context),
                            Err(error) => warn!("skipping an undecodable message from player#{}: {error}", id.0),
                        }
                    }
                }
            }
        }
    }
    for id in closed {
        links.remove(id);
        disconnect_player(&mut commands, id, &mut context);
    }
}

fn register(
    commands: &mut Commands,
    links: &mut ClientLinks,
    context: &mut ClientMessageContext,
    source: LinkSource,
    to_client: Sender<ServerMessage>,
) {
    let id = links.register(source);
    debug!("player#{} registered", id.0);
    let entity = commands.spawn((PlayerMarker, id)).id();
    context.players.insert(id, PlayerInfo::new(entity, to_client));
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
