use super::*;

#[test]
fn smoke_indices_allocate_two_radial_bands_per_particle() {
    let indices = smoke_indices(2);
    assert_eq!(indices.len(), SMOKE_RING_SEGMENTS * 9 * 2);
    assert_eq!(indices.iter().copied().max(), Some(49));
}

#[test]
fn smoke_reaches_full_opacity_as_fireball_ends_then_holds() {
    let lifetime = EXPLOSION_SMOKE_LIFETIME_SECS;
    let opacity = EXPLOSION_SMOKE_MAX_OPACITY;
    assert_eq!(smoke_alpha(0.0, lifetime, opacity), 0.0);
    assert!(smoke_alpha(0.25, lifetime, opacity) < smoke_alpha(0.5, lifetime, opacity));
    assert_eq!(smoke_alpha(0.5, lifetime, opacity), opacity);
    assert_eq!(smoke_alpha(2.0, lifetime, opacity), opacity);
    assert!(smoke_alpha(3.5, lifetime, opacity) < opacity);
    assert_eq!(smoke_alpha(lifetime, lifetime, opacity), 0.0);
}
