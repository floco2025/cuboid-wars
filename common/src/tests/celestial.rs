use super::*;

fn map(latitude: f32, season: Season) -> CelestialMapSettings {
    CelestialMapSettings {
        latitude_degrees: latitude,
        season,
        north_yaw_degrees: 0.0,
        start_local_time: LocalTime::parse("09:00").expect("valid fixture time"),
        start_moon_phase: 0.25,
    }
}

fn at(hour: f32, phase: f32) -> CelestialTime {
    CelestialTime {
        solar_day_fraction: hour / 24.0,
        lunar_phase_fraction: phase,
    }
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
    for (phase, expected_light, rise_hour) in [(0.0, 0.0, 6.0), (0.25, 0.5, 12.0), (0.5, 1.0, 18.0), (0.75, 0.5, 0.0)] {
        let directions = celestial_directions(fixture, at(rise_hour, phase));
        assert!((directions.moon_illuminated_fraction - expected_light).abs() < 0.001);
        assert!(directions.moon_altitude_radians.abs() < 0.12);
    }
    assert!(celestial_directions(fixture, at(12.0, 0.125)).moon.y < 0.8);
}

#[test]
fn lunar_inclination_separates_waxing_and_waning_altitudes() {
    let fixture = map(40.0, Season::Summer);
    let waxing = celestial_directions(fixture, at(12.0, 0.125));
    let waning = celestial_directions(fixture, at(12.0, 0.875));
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
    clock.set_moon_phase_fraction(0.5, 200, 10, cycle);
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
fn map_moon_phase_must_be_a_normalized_finite_number() {
    for invalid in [-0.001, 1.001, f32::NAN] {
        let mut fixture = map(40.0, Season::Summer);
        fixture.start_moon_phase = invalid;
        let error = fixture
            .validate("map.celestial")
            .expect_err("invalid phase was accepted");
        assert!(error.to_string().contains("start_moon_phase"));
    }

    for valid in [0.0, 0.25, 0.5, 0.75, 1.0] {
        let mut fixture = map(40.0, Season::Summer);
        fixture.start_moon_phase = valid;
        fixture.validate("map.celestial").expect("valid phase was rejected");
    }
}

#[test]
fn local_time_parser_is_strict() {
    assert_eq!(LocalTime::parse("09:00").map(LocalTime::minutes), Some(540));
    assert_eq!(LocalTime::parse("9:00").map(LocalTime::minutes), Some(540));
    assert_eq!(
        LocalTime::parse("1:00").map(LocalTime::format),
        Some("01:00".to_owned())
    );
    for invalid in ["009:00", "+1:00", "09:0", "24:00", "12:60", "noon"] {
        assert!(LocalTime::parse(invalid).is_none(), "accepted {invalid}");
    }
}
