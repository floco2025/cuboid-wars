use bevy::prelude::Resource;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, error::TryRecvError};

use common::protocol::{ClientMessage, PlayerId, ServerMessage};

// The server's ends of one client's queues. A QUIC task holds the other ends
// for a remote client; the host's own client holds them directly.
pub struct ClientLink {
    pub to_client: UnboundedSender<ServerMessage>,
    pub from_client: UnboundedReceiver<ClientMessage>,
}

#[derive(Resource)]
pub struct NewLinksChannel(UnboundedReceiver<ClientLink>);

impl NewLinksChannel {
    #[must_use]
    pub const fn new(receiver: UnboundedReceiver<ClientLink>) -> Self {
        Self(receiver)
    }

    pub fn try_recv(&mut self) -> Result<ClientLink, TryRecvError> {
        self.0.try_recv()
    }
}

// Every registered client's receiver, in registration order, with the id
// assigned on registration.
#[derive(Resource, Default)]
pub struct ClientLinks {
    last_id: u32,
    links: Vec<(PlayerId, UnboundedReceiver<ClientMessage>)>,
}

impl ClientLinks {
    pub fn register(&mut self, from_client: UnboundedReceiver<ClientMessage>) -> PlayerId {
        self.last_id = self.last_id.checked_add(1).expect("player id counter overflowed");
        let id = PlayerId(self.last_id);
        self.links.push((id, from_client));
        id
    }

    pub fn remove(&mut self, id: PlayerId) {
        self.links.retain(|(link, _)| *link != id);
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (PlayerId, &mut UnboundedReceiver<ClientMessage>)> {
        self.links.iter_mut().map(|(id, from_client)| (*id, from_client))
    }
}
