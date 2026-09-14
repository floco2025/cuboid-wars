use super::*;

fn ok_weather_cycle() -> WeatherCycleConfig {
    WeatherCycleConfig {
        min_clear_secs: 10.0,
        max_clear_secs: 20.0,
        min_rain_secs: 5.0,
        max_rain_secs: 8.0,
        ramp_in_secs: 2.0,
        fade_out_secs: 4.0,
    }
}

#[test]
fn weather_cycle_accepts_valid_config() {
    ok_weather_cycle()
        .validate("cycles.weather")
        .expect("valid weather cycle was rejected");
}

#[test]
fn weather_cycle_rejects_min_clear_above_max() {
    let mut cycle = ok_weather_cycle();
    cycle.min_clear_secs = 30.0;
    let err = cycle
        .validate("cycles.weather")
        .expect_err("min_clear > max_clear must be rejected");
    assert!(err.to_string().contains("min_clear_secs"));
}

#[test]
fn weather_cycle_rejects_min_rain_above_max() {
    let mut cycle = ok_weather_cycle();
    cycle.min_rain_secs = 30.0;
    let err = cycle
        .validate("cycles.weather")
        .expect_err("min_rain > max_rain must be rejected");
    assert!(err.to_string().contains("min_rain_secs"));
}

#[test]
fn weather_cycle_rejects_non_positive_ramp() {
    let mut cycle = ok_weather_cycle();
    cycle.ramp_in_secs = 0.0;
    let err = cycle
        .validate("cycles.weather")
        .expect_err("zero ramp_in must be rejected");
    assert!(err.to_string().contains("ramp_in_secs"));
}

#[test]
fn celestial_cycle_parses_and_validates() {
    let cycle: common::celestial::CelestialCycleSettings =
        serde_json::from_str(r#"{"day_duration_secs":600.0,"lunar_cycle_days":8.0}"#)
            .expect("celestial cycle failed to deserialize");
    cycle.validate("cycles.celestial").expect("valid cycle rejected");
}

#[test]
fn celestial_cycle_rejects_non_positive_and_non_finite_values() {
    for (day_duration_secs, lunar_cycle_days, field) in [
        (0.0, 8.0, "day_duration_secs"),
        (600.0, -1.0, "lunar_cycle_days"),
        (f32::NAN, 8.0, "day_duration_secs"),
    ] {
        let cycle = common::celestial::CelestialCycleSettings {
            day_duration_secs,
            lunar_cycle_days,
        };
        let error = cycle.validate("cycles.celestial").expect_err("invalid cycle accepted");
        assert!(error.to_string().contains(field));
    }
}
