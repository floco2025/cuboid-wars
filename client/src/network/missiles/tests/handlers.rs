use super::launch_is_stale;

#[test]
fn a_launch_is_stale_only_behind_an_applied_snapshot() {
    assert!(!launch_is_stale(None, 5));
    assert!(!launch_is_stale(Some(4), 5));
    assert!(launch_is_stale(Some(5), 5));
    assert!(launch_is_stale(Some(6), 5));
    assert!(!launch_is_stale(Some(u32::MAX), 0));
    assert!(launch_is_stale(Some(0), u32::MAX));
}
