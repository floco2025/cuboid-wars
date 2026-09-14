use super::*;

#[test]
fn cloud_cover_and_precipitation_ramps_are_sequenced_independently() {
    let early = RainIntensity {
        target: 0.2,
        current: 0.2,
        raining: false,
        precipitation: 0.0,
    };
    assert_eq!(early.cloud_cover(), 0.2);
    assert_eq!(early.precipitation(), 0.0);

    let starting = RainIntensity {
        target: 0.4,
        current: 0.4,
        raining: false,
        precipitation: 0.0,
    };
    assert_eq!(starting.cloud_cover(), 0.4);
    assert_eq!(starting.precipitation(), 0.0);

    let nearly_covered = RainIntensity {
        target: 0.999,
        current: 0.999,
        raining: false,
        precipitation: 0.0,
    };
    assert_eq!(nearly_covered.cloud_cover(), 0.999);
    assert_eq!(nearly_covered.precipitation(), 0.0);

    let storm = RainIntensity {
        target: 1.0,
        current: 1.0,
        raining: true,
        precipitation: 1.0,
    };
    assert_eq!(storm.cloud_cover(), 1.0);
    assert_eq!(storm.precipitation(), 1.0);

    let clearing = RainIntensity {
        target: 1.0,
        current: 1.0,
        raining: false,
        precipitation: 1.0,
    };
    assert_eq!(clearing.cloud_cover(), 1.0);
    assert_eq!(clearing.precipitation(), 1.0);

    assert_eq!(step_precipitation(0.0, true, 0.5), 0.25);
    assert_eq!(step_precipitation(0.75, true, 0.5), 1.0);
    assert_eq!(step_precipitation(1.0, false, 0.5), 0.75);
    assert_eq!(step_precipitation(0.25, false, 0.5), 0.0);
}
