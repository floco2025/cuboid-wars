use super::{MotionDef, carrier_from_motion};
use bevy::math::Vec3;
use common::{map::carrier_offset_at, protocol::CarrierId};

#[test]
fn carrier_travel_pause_and_phase_keep_their_seconds_at_sixty_hz() {
    let motion: MotionDef = serde_json::from_str(
        r#"{"level":0,"from":[0,0],"to":[1,0],"travel_secs":2.0,"pause_secs":0.5,"phase_secs":0.5}"#,
    )
    .expect("motion fixture invalid");
    let carrier = |hz| {
        carrier_from_motion(
            Vec3::ZERO,
            Vec3::new(4.0, 2.0, 0.0),
            &motion,
            [0, 1],
            Vec3::ONE,
            CarrierId::WORLD,
            hz,
            None,
        )
    };
    let baseline = carrier(30);
    let faster = carrier(60);
    assert_eq!(faster.travel_ticks, 120);
    assert_eq!(faster.pause_ticks, 30);
    assert_eq!(faster.phase_ticks, 30);
    for half_seconds in 0..=20 {
        assert!(
            carrier_offset_at(&baseline, half_seconds * 15)
                .abs_diff_eq(carrier_offset_at(&faster, half_seconds * 30), 1e-5)
        );
    }
}
