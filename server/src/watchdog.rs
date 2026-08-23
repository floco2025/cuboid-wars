use common::protocol::Position;

// The one stall detector for everything that should be making progress:
// trips when a window elapses without enough net displacement from the
// anchor. The response is the caller's — actors shake loose, missiles
// detonate. Tripping re-arms, so an ignored trip fires again a window later.
#[derive(Debug, Clone, Default)]
pub struct ProgressWatchdog {
    anchor: Option<Position>,
    stalled_secs: f32,
}

impl ProgressWatchdog {
    pub fn reset(&mut self) {
        self.anchor = None;
        self.stalled_secs = 0.0;
    }

    // Ground characters: horizontal displacement only — falling or being
    // pushed vertically in place is not progress.
    pub fn tick_horizontal(&mut self, pos: &Position, delta: f32, progress_distance: f32, window_secs: f32) -> bool {
        self.tick(
            pos,
            delta,
            progress_distance,
            window_secs,
            Position::horizontal_distance_sq,
        )
    }

    // Fliers: full 3D displacement.
    pub fn tick_3d(&mut self, pos: &Position, delta: f32, progress_distance: f32, window_secs: f32) -> bool {
        self.tick(pos, delta, progress_distance, window_secs, Position::distance_sq)
    }

    fn tick(
        &mut self,
        pos: &Position,
        delta: f32,
        progress_distance: f32,
        window_secs: f32,
        distance_sq: fn(&Position, &Position) -> f32,
    ) -> bool {
        let Some(anchor) = self.anchor else {
            self.anchor = Some(*pos);
            return false;
        };
        if distance_sq(&anchor, pos) >= progress_distance * progress_distance {
            self.anchor = Some(*pos);
            self.stalled_secs = 0.0;
            return false;
        }
        self.stalled_secs += delta;
        if self.stalled_secs >= window_secs {
            self.reset();
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
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
}
