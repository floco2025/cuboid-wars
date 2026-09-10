use super::*;

fn pos(x: f32, y: f32, z: f32) -> Position {
    Position { x, y, z }
}

#[test]
fn trips_after_window_without_progress() {
    let mut watchdog = ProgressWatchdog::default();
    let pinned = pos(0.0, 2.0, 0.0);

    assert!(!watchdog.tick_3d(&pinned, 0.1, 1.0, 1.0), "first tick arms");
    for _ in 0..9 {
        assert!(!watchdog.tick_3d(&pinned, 0.1, 1.0, 1.0));
    }
    assert!(watchdog.tick_3d(&pinned, 0.1, 1.0, 1.0), "window elapsed while pinned");
}

#[test]
fn progress_re_anchors_and_restarts_the_window() {
    let mut watchdog = ProgressWatchdog::default();
    let start = pos(0.0, 2.0, 0.0);
    let moved = pos(0.0, 2.0, 1.5);

    assert!(!watchdog.tick_3d(&start, 0.5, 1.0, 1.0));
    assert!(!watchdog.tick_3d(&moved, 0.5, 1.0, 1.0), "progress re-anchors");
    assert!(!watchdog.tick_3d(&moved, 0.5, 1.0, 1.0), "full window needed again");
    assert!(watchdog.tick_3d(&moved, 0.5, 1.0, 1.0));
}

#[test]
fn horizontal_tick_ignores_vertical_motion() {
    let mut watchdog = ProgressWatchdog::default();
    assert!(!watchdog.tick_horizontal(&pos(0.0, 0.0, 0.0), 0.6, 0.5, 1.0));
    assert!(
        !watchdog.tick_horizontal(&pos(0.0, 5.0, 0.0), 0.6, 0.5, 1.0),
        "vertical displacement is not progress"
    );
    assert!(watchdog.tick_horizontal(&pos(0.0, 9.0, 0.0), 0.6, 0.5, 1.0));
}

#[test]
fn tripping_re_arms() {
    let mut watchdog = ProgressWatchdog::default();
    let pinned = pos(0.0, 0.0, 0.0);
    assert!(!watchdog.tick_horizontal(&pinned, 2.0, 0.5, 1.0));
    assert!(watchdog.tick_horizontal(&pinned, 2.0, 0.5, 1.0));
    assert!(
        !watchdog.tick_horizontal(&pinned, 2.0, 0.5, 1.0),
        "fresh anchor after a trip"
    );
    assert!(watchdog.tick_horizontal(&pinned, 2.0, 0.5, 1.0));
}
