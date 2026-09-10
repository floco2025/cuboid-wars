use super::*;

#[test]
fn banner_timing_accepts_inclusive_fades_and_rejects_invalid_intervals() {
    for fade in [0.0, 0.3, 1.2] {
        assert!(
            BannerTiming {
                duration_secs: 1.2,
                fade_out_secs: fade
            }
            .validate("hud.banner.checkpoint_reached")
            .is_ok()
        );
    }
    for (duration, fade) in [
        (0.0, 0.0),
        (-1.0, 0.0),
        (f32::NAN, 0.0),
        (1.2, -0.1),
        (1.2, 1.3),
        (1.2, f32::INFINITY),
    ] {
        let error = BannerTiming {
            duration_secs: duration,
            fade_out_secs: fade,
        }
        .validate("hud.banner.checkpoint_reached")
        .expect_err("invalid banner timing accepted")
        .to_string();
        assert!(error.contains("hud.banner.checkpoint_reached"));
    }
}
