use super::{super::test_fixtures::physics_with, *};

#[test]
fn portal_frame_snaps_floor_yaw_to_quarter_turns() {
    let physics = physics_with(0.0);
    let frame_up = |yaw: f32| {
        portal_frame(
            &physics,
            &SurfaceSpec {
                center: [0.0, 0.0, 0.0],
                normal: [0.0, 1.0, 0.0],
                yaw,
            },
        )
        .up
    };
    assert!(frame_up(0.3).distance(Vec3::Z) < 1e-5);
    assert!(frame_up(0.9).distance(Vec3::X) < 1e-5);
}

#[test]
fn physics_block_is_validated() {
    let mut physics = physics_with(0.0);
    physics.player_fall.safe_distance = 20.0;
    let error = physics.validate().expect_err("safe beyond lethal is invalid");
    assert!(error.to_string().contains("player_fall"), "{error}");
    let mut physics = physics_with(0.0);
    physics.server_hz = 0;
    assert!(physics.validate().is_err());
    let mut physics = physics_with(0.0);
    physics.player.move_speed = 0.0;
    let error = physics.validate().expect_err("zero move speed is invalid");
    assert!(error.to_string().contains("movement.player.move_speed"), "{error}");
    let mut physics = physics_with(0.0);
    physics.funnel.capture_margin = -0.1;
    let error = physics.validate().expect_err("a negative margin is invalid");
    assert!(error.to_string().contains("funnel.capture_margin"), "{error}");
    let mut physics = physics_with(0.0);
    physics.funnel.capture_growth = -0.1;
    let error = physics.validate().expect_err("a negative growth is invalid");
    assert!(error.to_string().contains("funnel.capture_growth"), "{error}");
}
