use super::*;

#[test]
fn any_first_tick_is_accepted() {
    let mut last = None;
    assert!(accept_newer_tick(&mut last, u32::MAX - 3));
    assert_eq!(last, Some(u32::MAX - 3));
}

#[test]
fn ticks_wrap_forward_and_older_ones_are_rejected() {
    let mut last = Some(u32::MAX);
    assert!(accept_newer_tick(&mut last, 0));
    assert!(!accept_newer_tick(&mut last, u32::MAX - 1));
    assert_eq!(last, Some(0));
}
