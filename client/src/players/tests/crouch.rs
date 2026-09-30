use super::*;

#[test]
fn a_grounded_blend_takes_its_time_and_an_airborne_duck_is_instant() {
    let crouched = PlayerStance { crouched: true };
    let standing = PlayerStance::default();
    let mut blend = CrouchBlend::settled(standing);
    blend.advance(crouched, CharacterSupport::Ground, PLAYER_CROUCH_BLEND_SECS * 0.25);
    assert!((blend.0 - 0.25).abs() < 1e-5, "{blend:?}");
    blend.advance(crouched, CharacterSupport::Ground, PLAYER_CROUCH_BLEND_SECS);
    assert_eq!(blend, CrouchBlend(1.0));
    blend.advance(standing, CharacterSupport::Airborne, 0.0);
    assert_eq!(blend, CrouchBlend::settled(standing));
}

#[test]
fn the_eye_and_the_model_follow_the_blend() {
    let config = crate::test_fixtures::gameplay_config().player;
    let standing = CrouchBlend::settled(PlayerStance::default());
    let crouched = CrouchBlend::settled(PlayerStance { crouched: true });
    assert_eq!(standing.eye_height(&config), config.eye_height());
    assert_eq!(
        crouched.eye_height(&config),
        config.eye_height() * PLAYER_CROUCH_EYE_RATIO
    );
    assert_eq!(standing.model_height_scale(), 1.0);
    assert_eq!(crouched.model_height_scale(), PLAYER_CROUCH_HULL_RATIO);
}
