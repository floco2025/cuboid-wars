use super::*;

#[test]
fn rates_are_independent_and_fit_within_simulation_frequency() {
    for update_hz in [1, 7, TICK_HZ] {
        for snapshot_hz in [1, 4, TICK_HZ] {
            assert!(
                NetworkConfig {
                    server_hz: 30,
                    update_hz,
                    snapshot_hz
                }
                .validate()
                .is_ok()
            );
        }
    }
    for hz in [0, TICK_HZ + 1, u32::MAX] {
        assert!(
            NetworkConfig {
                update_hz: hz,
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            NetworkConfig {
                snapshot_hz: hz,
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }
}

#[test]
fn missing_network_fields_use_defaults() {
    let config: NetworkConfig = serde_json::from_str("{}").expect("network defaults invalid");
    assert_eq!(config.server_hz, 30);
    assert_eq!(config.update_hz, TICK_HZ);
    assert_eq!(config.snapshot_hz, 4);
    let config: NetworkConfig = serde_json::from_str(r#"{"update_hz":10}"#).expect("partial network settings invalid");
    assert_eq!(config.update_hz, 10);
    assert_eq!(config.snapshot_hz, 4);
}

#[test]
fn sixty_hz_allows_independent_update_rates_but_neither_can_exceed_the_server() {
    for update_hz in [10, 30, 60] {
        for snapshot_hz in [4, 30, 60] {
            let config = NetworkConfig {
                server_hz: 60,
                update_hz,
                snapshot_hz,
            };
            assert!(config.validate().is_ok());
            assert!((config.tick_duration().as_secs_f64() - 1.0 / 60.0).abs() < 1e-9);
        }
    }
    for config in [
        NetworkConfig {
            server_hz: 0,
            ..Default::default()
        },
        NetworkConfig {
            server_hz: 60,
            update_hz: 61,
            ..Default::default()
        },
        NetworkConfig {
            server_hz: 60,
            snapshot_hz: 61,
            ..Default::default()
        },
    ] {
        assert!(config.validate().is_err());
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
