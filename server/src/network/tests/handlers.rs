use super::*;

#[test]
fn chat_text_is_sanitized() {
    assert_eq!(sanitize_chat_text("hello there"), Some("hello there".to_owned()));
    assert_eq!(sanitize_chat_text("he\nllo\u{7}"), Some("hello".to_owned()));
    assert_eq!(sanitize_chat_text("   \u{1b}  "), None);
    assert_eq!(sanitize_chat_text(""), None);
    let long = "x".repeat(400);
    assert_eq!(
        sanitize_chat_text(&long)
            .expect("long chat missing after truncation")
            .len(),
        CONSOLE_CHAT_MAX_CHARS
    );
}
