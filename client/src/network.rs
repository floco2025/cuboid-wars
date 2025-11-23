use bevy::app::AppExit;
use bevy::prelude::*;

use crate::io::ServerToClient;
use crate::ui::ChatState;
use crate::{FromServer, GameClient};

// ============================================================================
// Network Polling System
// ============================================================================

pub fn poll_network(
    mut from_server: ResMut<FromServer>,
    mut client: ResMut<GameClient>,
    mut chat_state: ResMut<ChatState>,
    mut exit: EventWriter<AppExit>,
) {
    // Process all available messages
    while let Ok(msg) = from_server.try_recv() {
        match msg {
            ServerToClient::Message(server_msg) => {
                let text = client.process_message(server_msg);
                // Add message to chat history
                chat_state.messages.push(text);
            }
            ServerToClient::Disconnected => {
                error!("Disconnected from server");
                exit.send(AppExit::Success);
                return;
            }
        }
    }
}
