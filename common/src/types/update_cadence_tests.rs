use super::UpdateCadence;

#[test]
fn every_rate_sends_immediately_without_drift_at_different_simulation_frequencies() {
    for server_hz in [20, 30, 50, 60, 120] {
        for update_hz in 1..=server_hz {
            let mut cadence = UpdateCadence::default();
            let due: Vec<_> = (0..server_hz * 10)
                .filter(|_| cadence.ready(update_hz, server_hz))
                .collect();
            assert_eq!(due[0], 0);
            assert_eq!(due.len(), update_hz as usize * 10);
            for pair in due.windows(2) {
                let interval = pair[1] - pair[0];
                assert!((server_hz / update_hz..=server_hz.div_ceil(update_hz)).contains(&interval));
            }
        }
    }
}
