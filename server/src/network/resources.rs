use bevy::prelude::Resource;
use tokio::sync::mpsc::{UnboundedReceiver, error::TryRecvError};

use crate::network::ClientToServer;
use common::protocol::PlayerId;

#[derive(Resource)]
pub struct FromClientsChannel(UnboundedReceiver<(PlayerId, ClientToServer)>);

impl FromClientsChannel {
    #[must_use]
    pub const fn new(receiver: UnboundedReceiver<(PlayerId, ClientToServer)>) -> Self {
        Self(receiver)
    }

    pub fn try_recv(&mut self) -> Result<(PlayerId, ClientToServer), TryRecvError> {
        self.0.try_recv()
    }
}
