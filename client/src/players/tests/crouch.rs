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
