#[derive(Default)]
pub struct UpdateCadence {
    started: bool,
    phase: u64,
}

impl UpdateCadence {
    pub fn ready(&mut self, update_hz: u32, server_hz: u32) -> bool {
        if !self.started {
            self.started = true;
            return true;
        }
        self.phase += u64::from(update_hz);
        if self.phase < u64::from(server_hz) {
            return false;
        }
        self.phase -= u64::from(server_hz);
        true
    }
}

#[cfg(test)]
#[path = "update_cadence_tests.rs"]
mod tests;
