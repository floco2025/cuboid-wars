use super::*;

#[test]
fn advancing_wraps_to_zero() {
    assert_eq!(PlayerGeneration(u32::MAX).next(), PlayerGeneration(0));
    assert_eq!(PlayerGeneration(6).next(), PlayerGeneration(7));
}

#[test]
fn ordering_follows_advancement_across_the_wrap() {
    let last = PlayerGeneration(u32::MAX);
    let wrapped = last.next();
    assert!(wrapped.is_newer_than(last));
    assert!(!last.is_newer_than(wrapped));
    assert!(!wrapped.is_newer_than(wrapped));
    assert!(PlayerGeneration(3).is_newer_than(PlayerGeneration(2)));
    assert!(!PlayerGeneration(2).is_newer_than(PlayerGeneration(3)));
}
