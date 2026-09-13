use super::*;

fn map(latitude: f32, season: Season) -> CelestialMapSettings {
    CelestialMapSettings {
        latitude_degrees: latitude,
        season,
        north_yaw_degrees: 0.0,
        start_local_time: LocalTime::parse("09:00").expect("valid fixture time"),
        start_moon_phase: MoonPhase::FirstQuarter,
    }
}

fn at(hour: f32, phase: f32) -> CelestialTime {
    CelestialTime {
        solar_day_fraction: hour / 24.0,
        lunar_phase_fraction: phase,
    }
}

#[test]
fn equinox_equator_has_twelve_hour_day_and_cardinal_sun() {
    assert!((daylight_hours(0.0, Season::Spring) - 12.0).abs() < 0.001);
    let sunrise = celestial_directions(map(0.0, Season::Spring), at(6.0, 0.0));
    let noon = celestial_directions(map(0.0, Season::Spring), at(12.0, 0.0));
    assert!(sunrise.sun.dot(Vec3::X) > 0.999);
    assert!(noon.sun.dot(Vec3::Y) > 0.999);
}

#[test]
fn forty_north_summer_day_matches_expected_length() {
    let hours = daylight_hours(40.0, Season::Summer);
    assert!((hours - 14.84).abs() < 0.05);
    assert!((hours / 24.0 * 600.0 - 371.0).abs() < 2.0);
    let noon = celestial_directions(map(40.0, Season::Summer), at(12.0, 0.0));
    assert!((noon.sun_altitude_radians.to_degrees() - 73.44).abs() < 0.05);
}

#[test]
fn local_seasons_reverse_between_hemispheres_and_handle_poles() {
    assert!((daylight_hours(40.0, Season::Summer) - daylight_hours(-40.0, Season::Summer)).abs() < 0.001);
    assert!(daylight_hours(80.0, Season::Summer) > 23.9);
    assert!(daylight_hours(80.0, Season::Winter) < 0.1);
    assert!(daylight_hours(-80.0, Season::Summer) > 23.9);
}

#[test]
fn north_yaw_rotates_the_local_cardinal_frame_clockwise() {
    let mut fixture = map(0.0, Season::Spring);
    fixture.north_yaw_degrees = 90.0;
    let directions = celestial_directions(fixture, at(12.0, 0.0));
    assert!(directions.celestial_pole.dot(Vec3::X) > 0.999);
    let sunrise = celestial_directions(fixture, at(6.0, 0.0));
    assert!(sunrise.sun.dot(Vec3::NEG_Z) > 0.999);
}

#[test]
fn moon_phases_have_expected_light_and_rough_rise_relationships() {
    let fixture = map(0.0, Season::Spring);
    for (phase, expected_light, rise_hour) in [
        (MoonPhase::New, 0.0, 6.0),
        (MoonPhase::FirstQuarter, 0.5, 12.0),
        (MoonPhase::Full, 1.0, 18.0),
        (MoonPhase::ThirdQuarter, 0.5, 0.0),
    ] {
        let directions = celestial_directions(fixture, at(rise_hour, phase.fraction()));
        assert!((directions.moon_illuminated_fraction - expected_light).abs() < 0.001);
        assert!(directions.moon_altitude_radians.abs() < 0.12);
    }
    assert!(celestial_directions(fixture, at(12.0, 0.125)).moon.y < 0.8);
}

#[test]
fn lunar_inclination_and_terminator_orientation_are_preserved() {
    let fixture = map(40.0, Season::Summer);
    let waxing = celestial_directions(fixture, at(12.0, 0.125));
    let waning = celestial_directions(fixture, at(12.0, 0.875));
    assert!(waxing.moon_phase_orientation > 0.0);
    assert!(waning.moon_phase_orientation < 0.0);
    assert!((waxing.moon_altitude_radians - waning.moon_altitude_radians).abs() > 0.01);
}

#[test]
fn clock_extrapolates_wraps_pauses_resumes_and_rephases() {
    let cycle = CelestialCycleSettings {
        day_duration_secs: 600.0,
        lunar_cycle_days: 8.0,
    };
    let fixture = map(40.0, Season::Summer);
    let mut clock = CelestialClockAnchor::initial(&fixture, u32::MAX - 9);
    let later = clock.at_tick(20, 10, cycle);
    assert!((later.solar_day_fraction - (0.375 + 3.0 / 600.0)).abs() < 0.0001);
    assert!((later.lunar_phase_fraction - (0.25 + 3.0 / 4800.0)).abs() < 0.0001);

    let future_anchor = CelestialClockAnchor {
        anchor_tick: 21,
        ..clock
    };
    assert_eq!(
        future_anchor.at_tick(20, 10, cycle),
        future_anchor.at_tick(21, 10, cycle)
    );

    clock.seek_time(LocalTime::parse("23:30").expect("valid time"), 20, 10, cycle);
    assert!(!clock.running);
    assert_eq!(clock.at_tick(200, 10, cycle).solar_day_fraction, 23.5 / 24.0);
    clock.set_moon_phase(MoonPhase::Full, 200, 10, cycle);
    assert_eq!(clock.lunar_phase_fraction, 0.5);
    assert!(!clock.running);
    assert!(clock.resume(200, 10, cycle));
    assert!(!clock.resume(200, 10, cycle));
    let wrapped = clock.at_tick(200 + 600 * 10, 10, cycle);
    assert!((wrapped.solar_day_fraction - 23.5 / 24.0).abs() < 0.0001);
    assert!((wrapped.lunar_phase_fraction - 0.625).abs() < 0.0001);
    let full_lunar_wrap = clock.at_tick(200 + 8 * 600 * 10, 10, cycle);
    assert!((full_lunar_wrap.solar_day_fraction - 23.5 / 24.0).abs() < 0.0001);
    assert!((full_lunar_wrap.lunar_phase_fraction - 0.5).abs() < 0.0001);
}

#[test]
fn local_time_parser_is_strict() {
    assert_eq!(LocalTime::parse("09:00").map(LocalTime::minutes), Some(540));
    for invalid in ["9:00", "09:0", "24:00", "12:60", "noon"] {
        assert!(LocalTime::parse(invalid).is_none(), "accepted {invalid}");
    }
}
