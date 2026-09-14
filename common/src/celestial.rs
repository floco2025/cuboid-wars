//! Shared celestial configuration, clock extrapolation, and astronomical math.
//!
//! The model deliberately uses fixed equinox/solstice geometry rather than a
//! calendar. It is stable and deterministic on the server and every client;
//! it is not intended for navigation or eclipse prediction.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use anyhow::{Result, bail};
use bevy_ecs::prelude::Resource;
use bevy_math::{Quat, Vec3};
use bincode::{Decode, Encode};
use serde::Deserialize;

use crate::math::sequence_is_newer;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

// Parsed from the map's HH:MM string and kept compact on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct LocalTime {
    minutes: u16,
}

impl LocalTime {
    #[must_use]
    pub const fn from_minutes(minutes: u16) -> Option<Self> {
        if minutes < 24 * 60 {
            Some(Self { minutes })
        } else {
            None
        }
    }

    #[must_use]
    pub const fn minutes(self) -> u16 {
        self.minutes
    }

    #[must_use]
    pub fn day_fraction(self) -> f32 {
        f32::from(self.minutes) / (24.0 * 60.0)
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let (hours, minutes) = value.split_once(':')?;
        if !(1..=2).contains(&hours.len())
            || minutes.len() != 2
            || !hours.bytes().all(|byte| byte.is_ascii_digit())
            || !minutes.bytes().all(|byte| byte.is_ascii_digit())
        {
            return None;
        }
        let hours = hours.parse::<u16>().ok()?;
        let minutes = minutes.parse::<u16>().ok()?;
        if hours >= 24 || minutes >= 60 {
            return None;
        }
        Self::from_minutes(hours * 60 + minutes)
    }

    #[must_use]
    pub fn format(self) -> String {
        format!("{:02}:{:02}", self.minutes / 60, self.minutes % 60)
    }

    #[must_use]
    pub fn from_day_fraction(fraction: f32) -> Self {
        let minutes = (fraction.rem_euclid(1.0) * 24.0 * 60.0).floor() as u16 % (24 * 60);
        Self { minutes }
    }
}

impl<'de> Deserialize<'de> for LocalTime {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value)
            .ok_or_else(|| serde::de::Error::custom("expected local time in H:MM or HH:MM (0:00..23:59)"))
    }
}

#[derive(Debug, Clone, Copy, Encode, Decode, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CelestialMapSettings {
    pub latitude_degrees: f32,
    pub season: Season,
    pub north_yaw_degrees: f32,
    pub start_local_time: LocalTime,
    pub start_moon_phase: f32,
}

impl CelestialMapSettings {
    pub fn validate(&self, path: &str) -> Result<()> {
        if !(self.latitude_degrees.is_finite() && (-90.0..=90.0).contains(&self.latitude_degrees)) {
            bail!("{path}.latitude_degrees must be finite and in [-90, 90]");
        }
        if !self.north_yaw_degrees.is_finite() {
            bail!("{path}.north_yaw_degrees must be finite");
        }
        if !(self.start_moon_phase.is_finite() && (0.0..=1.0).contains(&self.start_moon_phase)) {
            bail!("{path}.start_moon_phase must be finite and in [0, 1]");
        }
        Ok(())
    }
}

#[derive(Resource, Debug, Clone, Copy, Encode, Decode, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CelestialCycleSettings {
    pub day_duration_secs: f32,
    pub lunar_cycle_days: f32,
}

impl CelestialCycleSettings {
    pub fn validate(&self, path: &str) -> Result<()> {
        for (name, value) in [
            ("day_duration_secs", self.day_duration_secs),
            ("lunar_cycle_days", self.lunar_cycle_days),
        ] {
            if !(value.is_finite() && value > 0.0) {
                bail!("{path}.{name} must be positive and finite");
            }
        }
        Ok(())
    }
}

// Durable, repairable clock state. While running, both fractions are values
// at `anchor_tick`; while held, they are simply the displayed values.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct CelestialClockAnchor {
    pub anchor_tick: u32,
    pub solar_day_fraction: f32,
    pub lunar_phase_fraction: f32,
    pub running: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CelestialTime {
    pub solar_day_fraction: f32,
    pub lunar_phase_fraction: f32,
}

impl CelestialClockAnchor {
    #[must_use]
    pub fn initial(map: &CelestialMapSettings, tick: u32) -> Self {
        Self {
            anchor_tick: tick,
            solar_day_fraction: map.start_local_time.day_fraction(),
            lunar_phase_fraction: map.start_moon_phase.rem_euclid(1.0),
            running: true,
        }
    }

    #[must_use]
    pub fn at_tick(self, tick: u32, server_hz: u32, cycle: CelestialCycleSettings) -> CelestialTime {
        if !self.running {
            return CelestialTime {
                solar_day_fraction: self.solar_day_fraction.rem_euclid(1.0),
                lunar_phase_fraction: self.lunar_phase_fraction.rem_euclid(1.0),
            };
        }
        // A newly replicated anchor can be a few ticks ahead of a client's
        // corrected estimate. Hold at the anchor rather than interpreting
        // that ordinary clock skew as almost an entire u32 wrap.
        let elapsed_ticks = if sequence_is_newer(self.anchor_tick, tick) {
            0
        } else {
            tick.wrapping_sub(self.anchor_tick)
        };
        let elapsed_secs = f64::from(elapsed_ticks) / f64::from(server_hz.max(1));
        let elapsed_days = elapsed_secs / f64::from(cycle.day_duration_secs);
        CelestialTime {
            solar_day_fraction: (f64::from(self.solar_day_fraction) + elapsed_days).rem_euclid(1.0) as f32,
            lunar_phase_fraction: (f64::from(self.lunar_phase_fraction)
                + elapsed_days / f64::from(cycle.lunar_cycle_days))
            .rem_euclid(1.0) as f32,
        }
    }

    pub fn seek_time(&mut self, time: LocalTime, tick: u32, server_hz: u32, cycle: CelestialCycleSettings) {
        let current = self.at_tick(tick, server_hz, cycle);
        self.anchor_tick = tick;
        self.solar_day_fraction = time.day_fraction();
        self.lunar_phase_fraction = current.lunar_phase_fraction;
        self.running = false;
    }

    pub fn resume(&mut self, tick: u32, server_hz: u32, cycle: CelestialCycleSettings) -> bool {
        if self.running {
            return false;
        }
        let current = self.at_tick(tick, server_hz, cycle);
        self.anchor_tick = tick;
        self.solar_day_fraction = current.solar_day_fraction;
        self.lunar_phase_fraction = current.lunar_phase_fraction;
        self.running = true;
        true
    }

    pub fn set_moon_phase_fraction(&mut self, fraction: f32, tick: u32, server_hz: u32, cycle: CelestialCycleSettings) {
        assert!(fraction.is_finite(), "moon phase fraction is not finite");
        let current = self.at_tick(tick, server_hz, cycle);
        self.anchor_tick = tick;
        self.solar_day_fraction = current.solar_day_fraction;
        self.lunar_phase_fraction = fraction.rem_euclid(1.0);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CelestialDirections {
    // Unit vectors from the observer toward each body.
    pub sun: Vec3,
    pub moon: Vec3,
    pub celestial_pole: Vec3,
    pub star_rotation_radians: f32,
    pub sun_altitude_radians: f32,
    pub moon_altitude_radians: f32,
    pub moon_illuminated_fraction: f32,
}

const OBLIQUITY_RADIANS: f32 = 23.44_f32.to_radians();
const MOON_INCLINATION_RADIANS: f32 = 5.14_f32.to_radians();

#[must_use]
pub fn solar_declination(latitude_degrees: f32, season: Season) -> f32 {
    let hemisphere = if latitude_degrees < 0.0 { -1.0 } else { 1.0 };
    match season {
        Season::Spring | Season::Autumn => 0.0,
        Season::Summer => hemisphere * OBLIQUITY_RADIANS,
        Season::Winter => -hemisphere * OBLIQUITY_RADIANS,
    }
}

fn global_solar_longitude(latitude_degrees: f32, season: Season) -> f32 {
    let local = match season {
        Season::Spring => 0.0,
        Season::Summer => FRAC_PI_2,
        Season::Autumn => PI,
        Season::Winter => 3.0 * FRAC_PI_2,
    };
    if latitude_degrees < 0.0 {
        (local + PI) % TAU
    } else {
        local
    }
}

fn horizontal_direction(latitude: f32, declination: f32, hour_angle: f32) -> Vec3 {
    let east = -declination.cos() * hour_angle.sin();
    let up = latitude.sin() * declination.sin() + latitude.cos() * declination.cos() * hour_angle.cos();
    let north = latitude.cos() * declination.sin() - latitude.sin() * declination.cos() * hour_angle.cos();
    Vec3::new(east, up, north).normalize()
}

fn ecliptic_to_equatorial(longitude: f32, latitude: f32) -> (f32, f32) {
    let x = latitude.cos() * longitude.cos();
    let y = latitude.cos() * longitude.sin() * OBLIQUITY_RADIANS.cos() - latitude.sin() * OBLIQUITY_RADIANS.sin();
    let z = latitude.cos() * longitude.sin() * OBLIQUITY_RADIANS.sin() + latitude.sin() * OBLIQUITY_RADIANS.cos();
    (y.atan2(x).rem_euclid(TAU), z.asin())
}

#[must_use]
pub fn celestial_directions(map: CelestialMapSettings, time: CelestialTime) -> CelestialDirections {
    let latitude = map.latitude_degrees.to_radians();
    let yaw = map.north_yaw_degrees.to_radians();
    let frame_rotation = Quat::from_rotation_y(yaw);
    let solar_hour_angle = TAU * (time.solar_day_fraction - 0.5);
    let sun_declination = solar_declination(map.latitude_degrees, map.season);
    let unrotated_sun = horizontal_direction(latitude, sun_declination, solar_hour_angle);

    let phase_angle = TAU * time.lunar_phase_fraction.rem_euclid(1.0);
    let solar_longitude = global_solar_longitude(map.latitude_degrees, map.season);
    let (sun_right_ascension, _) = ecliptic_to_equatorial(solar_longitude, 0.0);
    let moon_longitude = solar_longitude + phase_angle;
    let moon_latitude = MOON_INCLINATION_RADIANS * phase_angle.sin();
    let (moon_right_ascension, moon_declination) = ecliptic_to_equatorial(moon_longitude, moon_latitude);
    let right_ascension_delta = (moon_right_ascension - sun_right_ascension + PI).rem_euclid(TAU) - PI;
    let moon_hour_angle = solar_hour_angle - right_ascension_delta;
    let unrotated_moon = horizontal_direction(latitude, moon_declination, moon_hour_angle);

    let pole = frame_rotation * Vec3::new(0.0, latitude.sin(), latitude.cos());
    CelestialDirections {
        sun: frame_rotation * unrotated_sun,
        moon: frame_rotation * unrotated_moon,
        celestial_pole: pole,
        // Noon at each fixed season retains a stable relationship between
        // the sun and the stars; time of day supplies the daily rotation.
        star_rotation_radians: (solar_hour_angle + sun_right_ascension).rem_euclid(TAU),
        sun_altitude_radians: unrotated_sun.y.asin(),
        moon_altitude_radians: unrotated_moon.y.asin(),
        moon_illuminated_fraction: (1.0 - phase_angle.cos()) * 0.5,
    }
}

#[cfg(test)]
#[path = "tests/celestial.rs"]
mod tests;
