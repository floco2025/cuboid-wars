use crate::game::GameState;
use crate::net::{BevyToServer, ServerToBevy};
use bevy::app::AppExit;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
#[allow(clippy::wildcard_imports)]
use common::protocol::*;
use tokio::sync::mpsc::{
    UnboundedReceiver, UnboundedSender,
    error::{SendError, TryRecvError},
};

// ============================================================================
// Server Communication Resources
// ============================================================================

#[derive(Resource)]
pub struct BevyToServerChannel(UnboundedSender<BevyToServer>);

impl BevyToServerChannel {
    pub fn new(sender: UnboundedSender<BevyToServer>) -> Self {
        Self(sender)
    }

    pub fn send(&self, msg: BevyToServer) -> Result<(), SendError<BevyToServer>> {
        self.0.send(msg)
    }
}

#[derive(Resource)]
pub struct ServerToBevyChannel(UnboundedReceiver<ServerToBevy>);

impl ServerToBevyChannel {
    pub fn new(receiver: UnboundedReceiver<ServerToBevy>) -> Self {
        Self(receiver)
    }

    pub fn try_recv(&mut self) -> Result<ServerToBevy, TryRecvError> {
        self.0.try_recv()
    }
}

// ============================================================================
// Chat State Resource
// ============================================================================

#[derive(Resource, Default)]
pub struct ChatUi {
    pub history: Vec<String>,
    pub input: String,
}

// ============================================================================
// Network Polling System
// ============================================================================

pub fn server_to_bevy_system(
    mut from_server: ResMut<ServerToBevyChannel>,
    mut game_client: ResMut<GameState>,
    mut chat_ui: ResMut<ChatUi>,
    mut exit: EventWriter<AppExit>,
) {
    // Process all available messages
    while let Ok(msg) = from_server.try_recv() {
        match msg {
            ServerToBevy::Message(server_msg) => {
                let text = game_client.process_message(server_msg);
                // Add message to chat history
                chat_ui.history.push(text);
            }
            ServerToBevy::Disconnected => {
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

pub fn chat_ui_system(
    mut contexts: EguiContexts,
    mut chat_ui: ResMut<ChatUi>,
    game_client: Res<GameState>,
    to_server: Res<BevyToServerChannel>,
) {
    egui::CentralPanel::default().show(contexts.ctx_mut(), |ui| {
        ui.heading("Game Chat");

        // Chat history
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .stick_to_bottom(true)
            .max_height(400.0)
            .show(ui, |ui| {
                for msg in &chat_ui.history {
                    ui.label(msg);
                }
            });

        ui.separator();

        // Input box
        let response = ui.text_edit_singleline(&mut chat_ui.input);

        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            let text = chat_ui.input.trim().to_string();
            if !text.is_empty() {
                // Send message
                let _ = to_server.send(BevyToServer::Send(ClientMessage::Say(CSay { text })));
                chat_ui.input.clear();
            }
            response.request_focus();
        }

        ui.separator();

        // Player list
        ui.label("Players:");
        for name in game_client.get_all_names() {
            ui.label(format!("  {name}"));
        }
    });
}
