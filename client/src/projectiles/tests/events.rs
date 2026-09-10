use super::*;

#[test]
fn earliest_event_prefers_closest_with_world_winning_ties() {
    assert_eq!(
        earliest_projectile_event(Some(0.2), Some(0.5), Some(0.6), None),
        ProjectileEvent::Hit
    );
    assert_eq!(
        earliest_projectile_event(Some(0.5), Some(0.3), None, None),
        ProjectileEvent::Field
    );
    assert_eq!(
        earliest_projectile_event(Some(0.5), None, Some(0.3), None),
        ProjectileEvent::Surface
    );
    assert_eq!(
        earliest_projectile_event(Some(0.4), Some(0.4), None, None),
        ProjectileEvent::Field
    );
    assert_eq!(
        earliest_projectile_event(Some(0.4), None, Some(0.4), None),
        ProjectileEvent::Surface
    );
    assert_eq!(
        earliest_projectile_event(Some(0.4), None, None, None),
        ProjectileEvent::Hit
    );
    assert_eq!(
        earliest_projectile_event(None, Some(0.5), Some(0.3), None),
        ProjectileEvent::Surface
    );
    assert_eq!(earliest_projectile_event(None, None, None, None), ProjectileEvent::Fly);
}

#[test]
fn portal_wins_its_surface_tie_but_yields_to_closer_hits() {
    assert_eq!(
        earliest_projectile_event(None, None, Some(0.4), Some(0.4)),
        ProjectileEvent::Portal
    );
    assert_eq!(
        earliest_projectile_event(Some(0.3), None, Some(0.4), Some(0.4)),
        ProjectileEvent::Hit
    );
    assert_eq!(
        earliest_projectile_event(None, Some(0.3), Some(0.4), Some(0.4)),
        ProjectileEvent::Field
    );
    assert_eq!(
        earliest_projectile_event(None, None, Some(0.2), Some(0.6)),
        ProjectileEvent::Surface
    );
}
