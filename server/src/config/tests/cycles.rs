use super::*;

#[test]
fn weather_cycle_minimums_may_not_exceed_their_maximums() {
    let cycle = WeatherCycleConfig {
        min_clear_secs: 10.0,
        max_clear_secs: 20.0,
        min_rain_secs: 5.0,
        max_rain_secs: 8.0,
        ramp_in_secs: 2.0,
        fade_out_secs: 4.0,
    };
    cycle
        .validate("cycles.weather")
        .expect("valid weather cycle was rejected");
    for (field, invalid) in [
        (
            "min_clear_secs",
            WeatherCycleConfig {
                min_clear_secs: 30.0,
                ..cycle.clone()
            },
        ),
        (
            "min_rain_secs",
            WeatherCycleConfig {
                min_rain_secs: 30.0,
                ..cycle.clone()
            },
        ),
    ] {
        let err = invalid
            .validate("cycles.weather")
            .expect_err("a minimum above its maximum accepted");
        assert!(err.to_string().contains(field), "{err}");
    }
}
