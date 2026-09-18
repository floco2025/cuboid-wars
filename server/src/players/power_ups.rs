#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum PowerUpState {
    #[default]
    Inactive,
    Timed(f32),
    Permanent,
}

impl PowerUpState {
    pub fn from_duration(seconds: Option<f32>) -> Self {
        seconds.map_or(Self::Permanent, Self::Timed)
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
