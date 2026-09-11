use bevy::prelude::Resource;
use crossbeam_channel::{Receiver, Sender};
use renet::ClientId;

use common::protocol::{ClientMessage, PlayerId, ServerMessage};

// The server's ends of the host's own client's queues; that client holds
// the other ends.
pub struct LocalLink {
    pub to_client: Sender<ServerMessage>,
    pub from_client: Receiver<ClientMessage>,
}

// Where a registered client's messages come from: a queue for the host's own
// client, the listener for a remote one, whose replies wait in `outgoing`
// until the flush hands them to renet.
pub enum LinkSource {
    Local(Receiver<ClientMessage>),
    Remote {
        client_id: ClientId,
        outgoing: Receiver<ServerMessage>,
    },
}

// Every registered client's source, in registration order, with the id
// assigned on registration.
#[derive(Resource, Default)]
pub struct ClientLinks {
    last_id: u32,
    links: Vec<(PlayerId, LinkSource)>,
}

impl ClientLinks {
    pub fn register(&mut self, source: LinkSource) -> PlayerId {
        self.last_id = self.last_id.checked_add(1).expect("player id counter overflowed");
        let id = PlayerId(self.last_id);
        self.links.push((id, source));
        id
    }

    pub fn remove(&mut self, id: PlayerId) {
        self.links.retain(|(link, _)| *link != id);
    }

    #[must_use]
    pub fn player_of(&self, client_id: ClientId) -> Option<PlayerId> {
        self.links.iter().find_map(|(id, source)| match source {
            LinkSource::Remote { client_id: remote, .. } if *remote == client_id => Some(*id),
            _ => None,
        })
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (PlayerId, &mut LinkSource)> {
        self.links.iter_mut().map(|(id, source)| (*id, source))
    }
}
