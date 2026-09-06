use bevy::prelude::*;

// Marker for wall mesh entities.
#[derive(Component)]
pub struct WallMarker;

// Marker for upper-level floor slabs. Level-0 ground is not tagged.
#[derive(Component)]
pub struct RoofMarker;

// Marker for the ground plane (level-0 floor).
#[derive(Component)]
pub struct GroundMarker;

// Marker for ramp mesh entities.
#[derive(Component)]
pub struct RampMarker;

// The world storeys a map entity belongs to: `level` is the storey it sits
// on and `span` how many more it reaches — a ramp's upper landing, a
// ladder's climb, a stacked barrier, or the motion of the carrier it rides
// (`CarrierStoreys::tag` adds that). The level-focus toggle shows an entity
// while the focused storey is within `level..=level + span`.
#[derive(Component, Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MapLevel {
    pub level: u8,
    pub span: u8,
}

impl MapLevel {
    #[must_use]
    pub fn contains(self, storey: u8) -> bool {
        (self.level..=self.level.saturating_add(self.span)).contains(&storey)
    }
}
