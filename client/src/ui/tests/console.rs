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
    console.type_text(&"x".repeat(CONSOLE_COMMAND_MAX_CHARS));
    assert_eq!(console.buffer.chars().count(), CONSOLE_CHAT_MAX_CHARS);

    let mut console = ConsoleState::default();
    console.type_text(&format!("/{}", "x".repeat(CONSOLE_COMMAND_MAX_CHARS)));
    assert_eq!(console.buffer.chars().count(), CONSOLE_COMMAND_MAX_CHARS);
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
