use bevy::{
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
    },
    prelude::*,
};

use crate::{
    config::ClientSettings,
    constants::HUD_EDGE_MARGIN_PX,
    network::{ClientToServer, ClientToServerChannel},
};
use common::protocol::{CAdmin, ClientMessage};

// Anything longer is noise; the server truncates defensively as well.
const MAX_BUFFER_CHARS: usize = 128;
const MAX_HISTORY: usize = 32;

// The admin console: a one-line text input. While `open`, gameplay input
// systems stand down (each gates on this resource) so typing can't move,
// shoot, or hit toggle keys.
#[derive(Resource, Default)]
pub struct ConsoleState {
    pub open: bool,
    pub buffer: String,
    // Submitted commands, oldest first; ArrowUp/ArrowDown walk it.
    history: Vec<String>,
    // `Some(i)` while the buffer shows `history[i]`; cleared by any edit so a
    // recalled command forks instead of rewriting the history entry.
    history_index: Option<usize>,
}

impl ConsoleState {
    fn remember(&mut self, command: &str) {
        if self.history.last().is_none_or(|last| last != command) {
            if self.history.len() >= MAX_HISTORY {
                self.history.remove(0);
            }
            self.history.push(command.to_owned());
        }
        self.history_index = None;
    }

    fn recall_previous(&mut self) {
        let index = match self.history_index {
            None if self.history.is_empty() => return,
            None => self.history.len() - 1,
            Some(i) => i.saturating_sub(1),
        };
        self.history_index = Some(index);
        self.buffer.clone_from(&self.history[index]);
    }

    fn recall_next(&mut self) {
        match self.history_index {
            Some(i) if i + 1 < self.history.len() => {
                self.history_index = Some(i + 1);
                self.buffer.clone_from(&self.history[i + 1]);
            }
            Some(_) => {
                // Stepping past the newest returns to an empty prompt.
                self.history_index = None;
                self.buffer.clear();
            }
            None => {}
        }
    }
}

#[derive(Component)]
pub struct ConsoleMarker;

pub fn spawn_console(commands: &mut Commands, client_settings: &ClientSettings) {
    // Bottom-right, in the strip reserved below the message feed, so the
    // console input and the server replies above it read as one unit.
    commands.spawn((
        ConsoleMarker,
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(HUD_EDGE_MARGIN_PX),
            bottom: Val::Px(HUD_EDGE_MARGIN_PX),
            ..default()
        },
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(client_settings.hud.font_sizes.message_feed),
            ..default()
        },
        TextColor(Color::srgba(1.0, 0.85, 0.4, 1.0)),
        Visibility::Hidden,
    ));
}

// Enter or `/` opens the console (the Minecraft convention — if chat ever
// arrives, Enter becomes chat and `/` stays commands); while open,
// keystrokes edit the buffer, Enter submits the raw command string to the
// server (server parses and replies), Esc cancels. Runs before every gated
// input system so open/close takes effect the same frame the key lands.
pub fn console_input_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut keys: MessageReader<KeyboardInput>,
    mut console: ResMut<ConsoleState>,
    to_server: Res<ClientToServerChannel>,
) {
    if !console.open {
        // ArrowUp straight from gameplay opens the console with the last
        // command pre-filled — no need to press `/` or Enter first.
        if keyboard.just_pressed(KeyCode::ArrowUp) && !console.history.is_empty() {
            console.open = true;
            console.buffer.clear();
            console.history_index = None;
            console.recall_previous();
            keys.clear();
            return;
        }
        if keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::Slash) {
            console.open = true;
            console.buffer.clear();
            console.history_index = None;
            // Minecraft behavior: `/` opens with the slash pre-filled (the
            // command prefix); Enter opens empty. Commands require the
            // slash — the server treats slashless text as not-a-command.
            if keyboard.just_pressed(KeyCode::Slash) {
                console.buffer.push('/');
            }
            // Swallow this frame's key events so the opening keystroke
            // doesn't also submit (Enter) or double-type the slash.
            keys.clear();
        }
        return;
    }

    for input in keys.read() {
        if input.state != ButtonState::Pressed {
            continue;
        }
        match &input.logical_key {
            Key::Enter => {
                let command = console.buffer.trim().to_owned();
                if !command.is_empty() {
                    console.remember(&command);
                    let _ = to_server.send(ClientToServer::Send(ClientMessage::Admin(CAdmin { command })));
                }
                console.buffer.clear();
                console.open = false;
            }
            Key::Escape => {
                console.buffer.clear();
                console.open = false;
            }
            Key::ArrowUp => {
                console.recall_previous();
            }
            Key::ArrowDown => {
                console.recall_next();
            }
            Key::Backspace => {
                console.buffer.pop();
                console.history_index = None;
            }
            _ => {
                if let Some(text) = &input.text {
                    let mut typed = false;
                    for ch in text.chars() {
                        if !ch.is_control() && console.buffer.chars().count() < MAX_BUFFER_CHARS {
                            console.buffer.push(ch);
                            typed = true;
                        }
                    }
                    if typed {
                        console.history_index = None;
                    }
                }
            }
        }
    }
}

pub fn console_render_system(
    console: Res<ConsoleState>,
    node: Single<(&mut Text, &mut Visibility), With<ConsoleMarker>>,
) {
    let (mut text, mut visibility) = node.into_inner();
    if console.open {
        *visibility = Visibility::Visible;
        text.0 = format!("> {}_", console.buffer);
    } else {
        *visibility = Visibility::Hidden;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn console_with_history(commands: &[&str]) -> ConsoleState {
        let mut console = ConsoleState::default();
        for command in commands {
            console.remember(command);
        }
        console
    }

    #[test]
    fn recall_previous_walks_back_from_newest() {
        let mut console = console_with_history(&["/give keys", "/give missiles"]);

        console.recall_previous();
        assert_eq!(console.buffer, "/give missiles");
        console.recall_previous();
        assert_eq!(console.buffer, "/give keys");
        // Clamped at the oldest entry.
        console.recall_previous();
        assert_eq!(console.buffer, "/give keys");
    }

    #[test]
    fn recall_next_steps_forward_and_clears_past_newest() {
        let mut console = console_with_history(&["/give keys", "/give missiles"]);
        console.recall_previous();
        console.recall_previous();

        console.recall_next();
        assert_eq!(console.buffer, "/give missiles");
        console.recall_next();
        assert_eq!(console.buffer, "");
        assert_eq!(console.history_index, None);
    }

    #[test]
    fn remember_dedupes_consecutive_and_caps_length() {
        let mut console = ConsoleState::default();
        console.remember("/help");
        console.remember("/help");
        assert_eq!(console.history.len(), 1);

        for i in 0..(MAX_HISTORY * 2) {
            console.remember(&format!("/cmd {i}"));
        }
        assert_eq!(console.history.len(), MAX_HISTORY);
    }

    #[test]
    fn recall_on_empty_history_is_a_noop() {
        let mut console = ConsoleState::default();
        console.recall_previous();
        console.recall_next();
        assert_eq!(console.buffer, "");
        assert_eq!(console.history_index, None);
    }
}
