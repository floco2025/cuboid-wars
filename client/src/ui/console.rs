use std::collections::VecDeque;

use bevy::{
    ecs::hierarchy::ChildSpawnerCommands,
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
    },
    prelude::*,
};

use crate::{
    config::ClientSettings,
    constants::{CONSOLE_TEXT_COLOR, FEED_CHAT_TEXT_COLOR},
    network::ClientToServerChannel,
};
use common::{
    constants::{CONSOLE_CHAT_MAX_CHARS, CONSOLE_COMMAND_MAX_CHARS},
    protocol::{CAdmin, CChat, ClientMessage},
};

const MAX_HISTORY: usize = 32;

// The chat + admin console: a one-line text input. While `open`, gameplay
// input systems stand down (`console_closed`) so typing can't move, shoot,
// or hit toggle keys.
#[derive(Resource, Default)]
pub struct ConsoleState {
    pub open: bool,
    pub buffer: String,
    // Submitted lines, oldest first; ArrowUp/ArrowDown walk it.
    history: VecDeque<String>,
    // `Some(i)` while the buffer shows `history[i]`; cleared by any edit so a
    // recalled line forks instead of rewriting the history entry.
    history_index: Option<usize>,
}

impl ConsoleState {
    fn open_with(&mut self, buffer: &str) {
        self.open = true;
        self.buffer.clear();
        self.buffer.push_str(buffer);
        self.history_index = None;
    }

    fn close(&mut self) {
        self.buffer.clear();
        self.open = false;
    }

    fn remember(&mut self, line: &str) {
        if self.history.back().is_none_or(|last| last != line) {
            if self.history.len() >= MAX_HISTORY {
                self.history.pop_front();
            }
            self.history.push_back(line.to_owned());
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

    fn type_text(&mut self, text: &str) {
        let mut typed = false;
        for ch in text.chars() {
            if !ch.is_control() && self.buffer.chars().count() < self.max_chars() {
                self.buffer.push(ch);
                typed = true;
            }
        }
        if typed {
            self.history_index = None;
        }
    }

    // A command gets the server's command budget, chat the chat one.
    fn max_chars(&self) -> usize {
        if self.buffer.starts_with('/') {
            CONSOLE_COMMAND_MAX_CHARS
        } else {
            CONSOLE_CHAT_MAX_CHARS
        }
    }

    fn submit(&mut self) -> Option<ConsoleSubmission> {
        let line = self.buffer.trim().to_owned();
        let submission = if line.is_empty() {
            None
        } else {
            self.remember(&line);
            Some(ConsoleSubmission::from_line(line))
        };
        self.close();
        submission
    }
}

// Run condition for the input systems that must stand down while typing.
pub fn console_closed(console: Res<ConsoleState>) -> bool {
    !console.open
}

#[derive(Component)]
pub struct ConsoleMarker;

// The prompt line; hidden rather than collapsed while closed so the feed
// above it never jumps.
pub fn spawn_console(column: &mut ChildSpawnerCommands, client_settings: &ClientSettings) {
    column.spawn((
        ConsoleMarker,
        Text::new(prompt("")),
        TextFont {
            font_size: FontSize::Px(client_settings.hud.font_sizes.message_feed),
            ..default()
        },
        TextColor(CONSOLE_TEXT_COLOR),
        Visibility::Hidden,
    ));
}

fn prompt(buffer: &str) -> String {
    format!("> {buffer}_")
}

#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub(super) enum ConsoleSubmission {
    Admin(String),
    Chat(String),
}

impl ConsoleSubmission {
    fn from_line(line: String) -> Self {
        if line.starts_with('/') {
            Self::Admin(line)
        } else {
            Self::Chat(line)
        }
    }
}

// Enter or `/` opens the console (the Minecraft convention: Enter is chat,
// `/` opens with the command prefix pre-filled); ArrowUp opens it with the
// last line recalled. Keys are matched on their logical value, so `/` works
// on layouts where it isn't its own key. While open, keystrokes edit the
// buffer, Enter submits, and Esc cancels. `ClientSet::Console` runs before
// every gated input system so open/close takes effect the same frame.
pub(super) fn console_input_system(
    mut keys: MessageReader<KeyboardInput>,
    mut console: ResMut<ConsoleState>,
    mut submissions: MessageWriter<ConsoleSubmission>,
) {
    for input in keys.read() {
        if input.state != ButtonState::Pressed {
            continue;
        }
        if !console.open {
            if input.repeat {
                continue;
            }
            match &input.logical_key {
                Key::Enter => console.open_with(""),
                Key::Character(ch) if ch == "/" => console.open_with("/"),
                Key::ArrowUp if !console.history.is_empty() => {
                    console.open_with("");
                    console.recall_previous();
                }
                _ => {}
            }
            continue;
        }
        match &input.logical_key {
            Key::Enter | Key::Escape if input.repeat => {}
            Key::Enter => {
                if let Some(submission) = console.submit() {
                    submissions.write(submission);
                }
            }
            Key::Escape => console.close(),
            Key::ArrowUp => console.recall_previous(),
            Key::ArrowDown => console.recall_next(),
            Key::Backspace => {
                console.buffer.pop();
                console.history_index = None;
            }
            _ => {
                if let Some(text) = &input.text {
                    console.type_text(text);
                }
            }
        }
    }
}

pub(super) fn console_send_system(
    mut submissions: MessageReader<ConsoleSubmission>,
    to_server: Res<ClientToServerChannel>,
) {
    for submission in submissions.read() {
        let message = match submission {
            ConsoleSubmission::Admin(command) => ClientMessage::Admin(CAdmin {
                command: command.clone(),
            }),
            ConsoleSubmission::Chat(text) => ClientMessage::Chat(CChat { text: text.clone() }),
        };
        to_server.send(message);
    }
}

// The prompt reads as what it will send: chat in the chat color, a `/`
// command in the admin color.
pub fn ui_console_render_system(
    console: Res<ConsoleState>,
    node: Single<(&mut Text, &mut TextColor, &mut Visibility), With<ConsoleMarker>>,
) {
    if !console.is_changed() {
        return;
    }
    let (mut text, mut color, mut visibility) = node.into_inner();
    // Console history can change without changing the prompt; an equal `Text`
    // write would rerun text layout and an equal `Visibility` write propagation.
    if console.open {
        let line = prompt(&console.buffer);
        text.set_if_neq(Text(line));
        color.0 = if console.buffer.starts_with('/') {
            CONSOLE_TEXT_COLOR
        } else {
            FEED_CHAT_TEXT_COLOR
        };
    }
    visibility.set_if_neq(if console.open {
        Visibility::Visible
    } else {
        Visibility::Hidden
    });
}

#[cfg(test)]
#[path = "tests/console.rs"]
mod tests;
