use super::*;

fn console_with_history(lines: &[&str]) -> ConsoleState {
    let mut console = ConsoleState::default();
    for line in lines {
        console.remember(line);
    }
    console
}

#[test]
fn recall_walks_back_clamps_at_the_oldest_and_clears_past_the_newest() {
    let mut empty = ConsoleState::default();
    empty.recall_previous();
    empty.recall_next();
    assert_eq!(empty.buffer, "");
    assert_eq!(empty.history_index, None);

    let mut console = console_with_history(&["/give keys", "/give missiles"]);
    console.recall_previous();
    assert_eq!(console.buffer, "/give missiles");
    console.recall_previous();
    assert_eq!(console.buffer, "/give keys");
    // Clamped at the oldest entry.
    console.recall_previous();
    assert_eq!(console.buffer, "/give keys");

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

#[test]
fn slash_opens_prefilled_on_any_layout() {
    let mut app = App::new();
    app.add_message::<KeyboardInput>()
        .add_message::<ConsoleSubmission>()
        .init_resource::<ConsoleState>()
        .add_systems(Update, console_input_system);
    // A German layout: `/` is Shift+7, so the physical key isn't `Slash`.
    app.world_mut().write_message(KeyboardInput {
        key_code: KeyCode::Digit7,
        logical_key: Key::Character("/".into()),
        state: ButtonState::Pressed,
        text: Some("/".into()),
        repeat: false,
        window: Entity::PLACEHOLDER,
    });
    app.update();

    let console = app.world().resource::<ConsoleState>();
    assert!(console.open);
    assert_eq!(console.buffer, "/", "the opening keystroke isn't typed twice");
}
