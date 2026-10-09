use crate::types::Position;

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

    // Fliers: full 3D displacement.
    pub fn tick_3d(&mut self, pos: &Position, delta: f32, progress_distance: f32, window_secs: f32) -> bool {
        let Some(anchor) = self.anchor else {
            self.anchor = Some(*pos);
            return false;
        };
        if anchor.distance_sq(pos) >= progress_distance * progress_distance {
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
#[path = "tests/watchdog.rs"]
mod tests;
