use super::*;

#[test]
fn server_rates_require_a_representable_nonzero_tick() {
    for server_hz in [1, 30, 60, 1_000_000_000] {
        let config = NetworkConfig {
            server_hz,
            update_hz: 1,
            snapshot_hz: 1,
        };
        config.validate().expect("representable tick rejected");
        assert!(config.tick_duration() >= Duration::from_nanos(1));
    }
    for server_hz in [0, 1_000_000_001, u32::MAX] {
        let error = NetworkConfig {
            server_hz,
            update_hz: 1,
            snapshot_hz: 1,
        }
        .validate()
        .expect_err("invalid server rate accepted");
        assert!(error.to_string().contains("network.server_hz"), "{error}");
    }
}

#[test]
fn update_and_snapshot_rates_are_independent_but_neither_can_exceed_the_server() {
    for server_hz in [30, 60] {
        for update_hz in [1, 7, server_hz] {
            for snapshot_hz in [1, 4, server_hz] {
                let config = NetworkConfig {
                    server_hz,
                    update_hz,
                    snapshot_hz,
                };
                assert!(config.validate().is_ok(), "{config:?}");
            }
        }
        for hz in [0, server_hz + 1, u32::MAX] {
            for config in [
                NetworkConfig {
                    server_hz,
                    update_hz: hz,
                    snapshot_hz: 1,
                },
                NetworkConfig {
                    server_hz,
                    update_hz: 1,
                    snapshot_hz: hz,
                },
            ] {
                assert!(config.validate().is_err(), "{config:?}");
            }
        }
    }
}

#[test]
fn every_rate_sends_immediately_without_drift_at_different_simulation_frequencies() {
    for server_hz in [20, 30, 50, 60, 120] {
        for update_hz in 1..=server_hz {
            let mut cadence = UpdateCadence::new(update_hz, server_hz);
            let due: Vec<_> = (0..server_hz * 10).filter(|_| cadence.ready()).collect();
            assert_eq!(due[0], 0);
            assert_eq!(due.len(), update_hz as usize * 10);
            for pair in due.windows(2) {
                let interval = pair[1] - pair[0];
                assert!((server_hz / update_hz..=server_hz.div_ceil(update_hz)).contains(&interval));
            }
        }
    }
}
