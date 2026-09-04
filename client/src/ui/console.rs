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
    network::{ClientToServer, ClientToServerChannel},
};
use common::{
    constants::{CHAT_MAX_CHARS, COMMAND_MAX_CHARS},
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
            COMMAND_MAX_CHARS
        } else {
            CHAT_MAX_CHARS
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
        let _ = to_server.send(ClientToServer::Send(message));
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
mod tests {
    use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

    use super::*;

    fn console_with_history(lines: &[&str]) -> ConsoleState {
        let mut console = ConsoleState::default();
        for line in lines {
            console.remember(line);
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
        assert_eq!(console.history.front().map(String::as_str), Some("/cmd 32"));
    }

    #[test]
    fn recall_on_empty_history_is_a_noop() {
        let mut console = ConsoleState::default();
        console.recall_previous();
        console.recall_next();
        assert_eq!(console.buffer, "");
        assert_eq!(console.history_index, None);
    }

    #[test]
    fn slash_line_is_admin_and_plain_line_is_chat() {
        assert_eq!(
            ConsoleSubmission::from_line("/help".to_owned()),
            ConsoleSubmission::Admin("/help".to_owned())
        );
        assert_eq!(
            ConsoleSubmission::from_line("hello there".to_owned()),
            ConsoleSubmission::Chat("hello there".to_owned())
        );
    }

    #[test]
    fn typing_caps_chat_and_commands_by_their_own_budgets() {
        let mut console = ConsoleState::default();
        console.type_text(&"x".repeat(COMMAND_MAX_CHARS));
        assert_eq!(console.buffer.chars().count(), CHAT_MAX_CHARS);

        let mut console = ConsoleState::default();
        console.type_text(&format!("/{}", "x".repeat(COMMAND_MAX_CHARS)));
        assert_eq!(console.buffer.chars().count(), COMMAND_MAX_CHARS);
    }

    #[test]
    fn typing_drops_control_characters() {
        let mut console = ConsoleState::default();
        console.type_text("a\rb\n");
        assert_eq!(console.buffer, "ab");
    }

    fn app() -> (App, UnboundedReceiver<ClientToServer>) {
        let (tx, rx) = unbounded_channel();
        let mut app = App::new();
        app.add_message::<KeyboardInput>()
            .add_message::<ConsoleSubmission>()
            .insert_resource(ConsoleState::default())
            .insert_resource(ClientToServerChannel::new(tx))
            .add_systems(Update, (console_input_system, console_send_system).chain());
        (app, rx)
    }

    fn press(app: &mut App, key_code: KeyCode, logical_key: Key, text: Option<&str>) {
        app.world_mut().write_message(KeyboardInput {
            key_code,
            logical_key,
            state: ButtonState::Pressed,
            text: text.map(Into::into),
            repeat: false,
            window: Entity::PLACEHOLDER,
        });
        app.update();
    }

    fn console(app: &App) -> &ConsoleState {
        app.world().resource::<ConsoleState>()
    }

    #[test]
    fn slash_opens_prefilled_on_any_layout() {
        let (mut app, _rx) = app();

        // A German layout: `/` is Shift+7, so the physical key isn't `Slash`.
        press(&mut app, KeyCode::Digit7, Key::Character("/".into()), Some("/"));

        assert!(console(&app).open);
        assert_eq!(console(&app).buffer, "/", "the opening keystroke isn't typed twice");
    }

    #[test]
    fn enter_opens_empty_and_submits_the_typed_line() {
        let (mut app, mut rx) = app();

        press(&mut app, KeyCode::Enter, Key::Enter, Some("\r"));
        assert!(console(&app).open);
        assert_eq!(console(&app).buffer, "");

        press(&mut app, KeyCode::KeyH, Key::Character("h".into()), Some("h"));
        press(&mut app, KeyCode::KeyI, Key::Character("i".into()), Some("i"));
        press(&mut app, KeyCode::Enter, Key::Enter, Some("\r"));

        assert!(!console(&app).open);
        assert!(matches!(
            rx.try_recv(),
            Ok(ClientToServer::Send(ClientMessage::Chat(CChat { text }))) if text == "hi"
        ));
    }

    #[test]
    fn escape_cancels_without_sending() {
        let (mut app, mut rx) = app();
        press(&mut app, KeyCode::Enter, Key::Enter, Some("\r"));
        press(&mut app, KeyCode::KeyH, Key::Character("h".into()), Some("h"));

        press(&mut app, KeyCode::Escape, Key::Escape, None);

        assert!(!console(&app).open);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn arrow_up_from_gameplay_opens_with_the_last_line() {
        let (mut app, _rx) = app();
        press(&mut app, KeyCode::Enter, Key::Enter, Some("\r"));
        press(&mut app, KeyCode::Slash, Key::Character("/".into()), Some("/"));
        press(&mut app, KeyCode::KeyH, Key::Character("h".into()), Some("h"));
        press(&mut app, KeyCode::Enter, Key::Enter, Some("\r"));
        assert!(!console(&app).open);

        press(&mut app, KeyCode::ArrowUp, Key::ArrowUp, None);

        assert!(console(&app).open);
        assert_eq!(console(&app).buffer, "/h");
    }
}
