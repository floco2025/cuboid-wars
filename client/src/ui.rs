use crate::game::{ChatState, GameClient};
use crate::net::{ClientToServer, ServerToClient};
use bevy::app::AppExit;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
#[allow(clippy::wildcard_imports)]
use common::protocol::*;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, error::{SendError, TryRecvError}};

// ============================================================================
// Bevy Resources for Network Communication
// ============================================================================

#[derive(Resource)]
pub struct ToServer(UnboundedSender<ClientToServer>);

impl ToServer {
    pub fn new(sender: UnboundedSender<ClientToServer>) -> Self {
        Self(sender)
    }

    pub fn send(&self, msg: ClientToServer) -> Result<(), SendError<ClientToServer>> {
        self.0.send(msg)
    }
}

#[derive(Resource)]
pub struct FromServer(UnboundedReceiver<ServerToClient>);

impl FromServer {
    pub fn new(receiver: UnboundedReceiver<ServerToClient>) -> Self {
        Self(receiver)
    }

    pub fn try_recv(&mut self) -> Result<ServerToClient, TryRecvError> {
        self.0.try_recv()
    }
}

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

// ============================================================================
// UI System
// ============================================================================

pub fn chat_ui(
    mut contexts: EguiContexts,
    mut chat_state: ResMut<ChatState>,
    client: Res<GameClient>,
    to_server: Res<ToServer>,
) {
    egui::CentralPanel::default().show(contexts.ctx_mut(), |ui| {
        ui.heading("Game Chat");

        // Chat history
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .stick_to_bottom(true)
            .max_height(400.0)
            .show(ui, |ui| {
                for msg in &chat_state.messages {
                    ui.label(msg);
                }
            });

        ui.separator();

        // Input box
        let response = ui.text_edit_singleline(&mut chat_state.input);

        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            let text = chat_state.input.trim().to_string();
            if !text.is_empty() {
                // Send message
                let _ = to_server.send(ClientToServer::Send(ClientMessage::Say(CSay { text })));
                chat_state.input.clear();
            }
            response.request_focus();
        }

        ui.separator();

        // Player list
        ui.label("Players:");
        for name in client.get_all_names() {
            ui.label(format!("  {name}"));
        }
    });
}
