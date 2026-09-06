#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum PowerUpState {
    #[default]
    Inactive,
    Timed(f32),
    Permanent,
}

impl PowerUpState {
    pub fn from_duration(seconds: f32) -> Self {
        if seconds == 0.0 {
            Self::Permanent
        } else {
            Self::Timed(seconds)
        }
    }

    pub fn is_active(self) -> bool {
        !matches!(self, Self::Inactive)
    }

    pub fn tick(&mut self, delta: f32) {
        if let Self::Timed(remaining) = self {
            *remaining -= delta;
            if *remaining <= 0.0 {
                *self = Self::Inactive;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_duration_stays_active_without_expiry() {
        let mut state = PowerUpState::from_duration(0.0);
        state.tick(1000.0);
        assert_eq!(state, PowerUpState::Permanent);
    }

    #[test]
    fn timed_power_up_expires_at_its_duration() {
        let mut state = PowerUpState::from_duration(3.0);
        state.tick(1.0);
        assert_eq!(state, PowerUpState::Timed(2.0));
        state.tick(2.0);
        assert_eq!(state, PowerUpState::Inactive);
        state.tick(1.0);
        assert!(!state.is_active());
    }
}
