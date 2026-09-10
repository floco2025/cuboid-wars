use super::*;

#[test]
fn checkpoint_banner_uses_its_own_timing_and_text() {
    let mut settings: ClientSettings = serde_json::from_str(include_str!("../../../../config/client/client.json"))
        .expect("client configuration rejected");
    settings.hud.banner.checkpoint_reached = BannerTiming {
        duration_secs: 0.75,
        fade_out_secs: 0.1,
    };
    let (text, timing) = BannerMessage::CheckpointReached.into_timed_text(&settings);
    assert_eq!(text, "Checkpoint reached");
    assert_eq!(timing.duration_secs, 0.75);
    assert_eq!(timing.fade_out_secs, 0.1);
    let (_, timing) = BannerMessage::Death.into_timed_text(&settings);
    assert_eq!(timing.duration_secs, settings.hud.banner.death.duration_secs);
    assert_eq!(timing.fade_out_secs, settings.hud.banner.death.fade_out_secs);
}
