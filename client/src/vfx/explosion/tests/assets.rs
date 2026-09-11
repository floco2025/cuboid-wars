use super::*;

#[test]
fn larger_explosions_have_a_lower_sound_pitch() {
    assert!(explosion_sound_speed(6.0) > explosion_sound_speed(15.0));
}
