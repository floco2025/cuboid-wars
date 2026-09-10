// Exhausting this per-tick bounce/portal-hop budget leaves the projectile at its last validated position.
pub const PROJECTILE_EVENT_LIMIT: usize = 8;

// A portal crossing wins a near-simultaneous bounce against the portal's backing surface.
const PORTAL_SURFACE_TIE_EPSILON: f32 = 0.01;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectileEvent {
    Hit,
    Field,
    Surface,
    Portal,
    Fly,
}

#[must_use]
pub fn earliest_projectile_event(
    character_t: Option<f32>,
    field_t: Option<f32>,
    surface_t: Option<f32>,
    portal_t: Option<f32>,
) -> ProjectileEvent {
    if let Some(pt) = portal_t
        && character_t.is_none_or(|ct| pt < ct)
        && field_t.is_none_or(|bt| pt < bt)
        && surface_t.is_none_or(|st| pt <= st + PORTAL_SURFACE_TIE_EPSILON)
    {
        return ProjectileEvent::Portal;
    }
    if let Some(bt) = field_t
        && character_t.is_none_or(|ct| bt <= ct)
        && surface_t.is_none_or(|st| bt <= st)
    {
        return ProjectileEvent::Field;
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
#[path = "tests/events.rs"]
mod tests;
