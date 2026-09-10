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
#[path = "tests/power_ups.rs"]
mod tests;
