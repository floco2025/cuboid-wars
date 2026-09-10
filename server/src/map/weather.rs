use bevy::prelude::*;
use rand::{RngExt, rngs::ThreadRng};

use crate::config::{WeatherCycleConfig, WeatherMode};

// Cycles Clear → RampIn → Raining → FadeOut → Clear. Each variant carries
// its countdown; ramp fractions are derived from the config's fixed ramp
// lengths, so no totals need storing.
#[derive(Debug, Clone, Copy, PartialEq)]
enum WeatherPhase {
    Clear { remaining_secs: f32 },
    RampIn { remaining_secs: f32 },
    Raining { remaining_secs: f32 },
    FadeOut { remaining_secs: f32 },
}

// Server-scheduled weather for the loaded map. `intensity` is the single
// authoritative scalar clients drive all rain presentation from; it ships
// in every snapshot. With `auto` off the current state holds (the map's
// concrete mode, or an admin override) until `/weather` changes it.
#[derive(Resource)]
pub struct WeatherState {
    schedule: WeatherCycleConfig,
    phase: WeatherPhase,
    intensity: f32,
    auto: bool,
}

impl WeatherState {
    #[must_use]
    pub fn new(schedule: WeatherCycleConfig, mode: WeatherMode) -> Self {
        let mut rng = rand::rng();
        let (phase, intensity) = match mode {
            WeatherMode::Clear | WeatherMode::Auto => (
                WeatherPhase::Clear {
                    remaining_secs: rng.random_range(schedule.min_clear_secs..=schedule.max_clear_secs),
                },
                0.0,
            ),
            WeatherMode::Rain => (
                WeatherPhase::Raining {
                    remaining_secs: rng.random_range(schedule.min_rain_secs..=schedule.max_rain_secs),
                },
                1.0,
            ),
        };
        Self {
            schedule,
            phase,
            intensity,
            auto: mode == WeatherMode::Auto,
        }
    }

    #[must_use]
    pub fn intensity(&self) -> f32 {
        self.intensity
    }

    #[must_use]
    fn needs_tick(&self) -> bool {
        self.auto || matches!(self.phase, WeatherPhase::RampIn { .. } | WeatherPhase::FadeOut { .. })
    }

    // Admin override: rain now and hold it. Interrupting a fade scales the
    // ramp by the missing intensity, so the transition stays continuous
    // instead of snapping to zero and climbing back.
    pub fn hold_rain(&mut self) -> Result<(), &'static str> {
        match self.phase {
            WeatherPhase::RampIn { .. } | WeatherPhase::Raining { .. } => {
                if self.auto {
                    self.auto = false;
                    Ok(())
                } else {
                    Err("already raining")
                }
            }
            WeatherPhase::Clear { .. } | WeatherPhase::FadeOut { .. } => {
                self.phase = WeatherPhase::RampIn {
                    remaining_secs: self.schedule.ramp_in_secs * (1.0 - self.intensity),
                };
                self.auto = false;
                Ok(())
            }
        }
    }

    // Admin override: clear now and hold it, fading from the current
    // intensity (a mid-ramp stop fades from wherever the ramp got).
    pub fn hold_clear(&mut self) -> Result<(), &'static str> {
        match self.phase {
            WeatherPhase::Clear { .. } | WeatherPhase::FadeOut { .. } => {
                if self.auto {
                    self.auto = false;
                    Ok(())
                } else {
                    Err("not raining")
                }
            }
            WeatherPhase::RampIn { .. } | WeatherPhase::Raining { .. } => {
                self.phase = WeatherPhase::FadeOut {
                    remaining_secs: self.schedule.fade_out_secs * self.intensity,
                };
                self.auto = false;
                Ok(())
            }
        }
    }

    // Admin override: hand control back to the cycle. The held phase simply
    // keeps counting down, so the transition out is the scheduled one.
    pub fn resume_auto(&mut self) -> Result<(), &'static str> {
        if self.auto {
            return Err("weather cycle already running");
        }
        self.auto = true;
        Ok(())
    }

    #[must_use]
    pub fn status(&self) -> String {
        let phase = match self.phase {
            WeatherPhase::Clear { .. } => "clear".to_owned(),
            WeatherPhase::Raining { .. } => "rain".to_owned(),
            WeatherPhase::RampIn { .. } => format!("rain starting ({:.2})", self.intensity),
            WeatherPhase::FadeOut { .. } => format!("clearing ({:.2})", self.intensity),
        };
        let source = if self.auto { "auto" } else { "held" };
        format!("weather: {phase} ({source})")
    }
}

pub fn weather_system(time: Res<Time>, mut weather: ResMut<WeatherState>) {
    let mut rng = rand::rng();
    tick_weather(&mut weather, time.delta_secs(), &mut rng);
}

pub fn weather_needs_tick(weather: Res<WeatherState>) -> bool {
    weather.needs_tick()
}

fn tick_weather(state: &mut WeatherState, delta: f32, rng: &mut ThreadRng) {
    let schedule = state.schedule.clone();
    let remaining = match &mut state.phase {
        WeatherPhase::Clear { remaining_secs }
        | WeatherPhase::RampIn { remaining_secs }
        | WeatherPhase::Raining { remaining_secs }
        | WeatherPhase::FadeOut { remaining_secs } => {
            *remaining_secs -= delta;
            *remaining_secs
        }
    };
    if remaining <= 0.0 {
        state.phase = match state.phase {
            // Held states re-roll forever — only `hold_rain` / `hold_clear`
            // / `resume_auto` leave them.
            WeatherPhase::Clear { .. } if !state.auto => WeatherPhase::Clear {
                remaining_secs: rng.random_range(schedule.min_clear_secs..=schedule.max_clear_secs),
            },
            WeatherPhase::Clear { .. } => WeatherPhase::RampIn {
                remaining_secs: schedule.ramp_in_secs,
            },
            WeatherPhase::RampIn { .. } => WeatherPhase::Raining {
                remaining_secs: rng.random_range(schedule.min_rain_secs..=schedule.max_rain_secs),
            },
            WeatherPhase::Raining { .. } if !state.auto => WeatherPhase::Raining {
                remaining_secs: rng.random_range(schedule.min_rain_secs..=schedule.max_rain_secs),
            },
            WeatherPhase::Raining { .. } => WeatherPhase::FadeOut {
                remaining_secs: schedule.fade_out_secs,
            },
            WeatherPhase::FadeOut { .. } => WeatherPhase::Clear {
                remaining_secs: rng.random_range(schedule.min_clear_secs..=schedule.max_clear_secs),
            },
        };
    }

    state.intensity = match state.phase {
        WeatherPhase::Clear { .. } => 0.0,
        WeatherPhase::Raining { .. } => 1.0,
        WeatherPhase::RampIn { remaining_secs } => 1.0 - remaining_secs / schedule.ramp_in_secs,
        WeatherPhase::FadeOut { remaining_secs } => remaining_secs / schedule.fade_out_secs,
    }
    .clamp(0.0, 1.0);
}

#[cfg(test)]
#[path = "tests/weather.rs"]
mod tests;
