use super::*;
use crate::config::BarrierPulseVfxConfig;

#[test]
fn pulse_uses_configured_range_and_frequency_and_can_be_disabled() {
    let mut config = BarrierVfxConfig {
        emissive_brightness: 2.0,
        opacity: 0.6,
        pulse: BarrierPulseVfxConfig {
            min_opacity: 0.2,
            frequency_hz: 2.0,
        },
    };
    assert!((pulse_opacity(config, 0.125, 0.0) - 0.6).abs() < 1e-6);
    assert!((pulse_opacity(config, 0.375, 0.0) - 0.2).abs() < 1e-6);

    config.pulse.frequency_hz = 0.0;
    assert_eq!(pulse_opacity(config, 0.375, 0.0), config.opacity);
}
