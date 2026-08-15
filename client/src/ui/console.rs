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

// The admin console: a one-line text input. While `open`, gameplay input
// systems stand down (each gates on this resource) so typing can't move,
// shoot, or hit toggle keys.
#[derive(Resource, Default)]
pub struct ConsoleState {
    pub open: bool,
    pub buffer: String,
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
        if keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::Slash) {
            console.open = true;
            console.buffer.clear();
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
                    let _ = to_server.send(ClientToServer::Send(ClientMessage::Admin(CAdmin { command })));
                }
                console.buffer.clear();
                console.open = false;
            }
            Key::Escape => {
                console.buffer.clear();
                console.open = false;
            }
            Key::Backspace => {
                console.buffer.pop();
            }
            _ => {
                if let Some(text) = &input.text {
                    for ch in text.chars() {
                        if !ch.is_control() && console.buffer.chars().count() < MAX_BUFFER_CHARS {
                            console.buffer.push(ch);
                        }
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
