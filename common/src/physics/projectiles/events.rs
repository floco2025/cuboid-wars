use crate::constants::PORTAL_SURFACE_TIE_EPSILON;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectileEvent {
    Hit,
    Barrier,
    Surface,
    Portal,
    Fly,
}

#[must_use]
pub fn earliest_projectile_event(
    character_t: Option<f32>,
    barrier_t: Option<f32>,
    surface_t: Option<f32>,
    portal_t: Option<f32>,
) -> ProjectileEvent {
    if let Some(pt) = portal_t
        && character_t.is_none_or(|ct| pt < ct)
        && barrier_t.is_none_or(|bt| pt < bt)
        && surface_t.is_none_or(|st| pt <= st + PORTAL_SURFACE_TIE_EPSILON)
    {
        return ProjectileEvent::Portal;
    }
    if let Some(bt) = barrier_t
        && character_t.is_none_or(|ct| bt <= ct)
        && surface_t.is_none_or(|st| bt <= st)
    {
        return ProjectileEvent::Barrier;
    }
    if let Some(st) = surface_t
        && character_t.is_none_or(|ct| st <= ct)
    {
        return ProjectileEvent::Surface;
    }
    if character_t.is_some() {
        ProjectileEvent::Hit
    } else {
        ProjectileEvent::Fly
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn earliest_event_prefers_closest_with_world_winning_ties() {
        assert_eq!(
            earliest_projectile_event(Some(0.2), Some(0.5), Some(0.6), None),
            ProjectileEvent::Hit
        );
        assert_eq!(
            earliest_projectile_event(Some(0.5), Some(0.3), None, None),
            ProjectileEvent::Barrier
        );
        assert_eq!(
            earliest_projectile_event(Some(0.5), None, Some(0.3), None),
            ProjectileEvent::Surface
        );
        assert_eq!(
            earliest_projectile_event(Some(0.4), Some(0.4), None, None),
            ProjectileEvent::Barrier
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
            ProjectileEvent::Barrier
        );
        assert_eq!(
            earliest_projectile_event(None, None, Some(0.2), Some(0.6)),
            ProjectileEvent::Surface
        );
    }
}
