use bevy::math::Vec2;
use common::{
    config::MapGeometryConfig,
    map::{Grounds, GroundsSettings},
    protocol::{CarrierId, Floor, LightBridge, MapLayout, Ramp, RampShape},
};

// Only the root geometry at ground level cuts the landscape. Upper storeys
// must not leave a hole beneath an elevated course, and carriers cannot cut
// static terrain. A ramp cuts it where it comes up through this plane, which
// keeps basement access open even outside the base, and where a solid one
// stands on it, since the wedge fills its own cut. A plank standing on the
// plane leaves the ground under it.
pub(super) fn compile_grounds(layout: &MapLayout, settings: &GroundsSettings, geometry: MapGeometryConfig) -> Grounds {
    let y = geometry.level_y(settings.level);
    let floors = layout
        .floors
        .iter()
        .filter(|floor| floor.carrier == CarrierId::WORLD && floor.level == settings.level)
        .map(Floor::bounds_xz);
    let walls = layout
        .walls
        .iter()
        .filter(|wall| wall.carrier == CarrierId::WORLD && wall.level == settings.level)
        .map(|wall| {
            let start = Vec2::new(wall.x1, wall.z1);
            let end = Vec2::new(wall.x2, wall.z2);
            let padding = (end - start).normalize_or_zero().perp().abs() * wall.width * 0.5;
            let min = start.min(end) - padding;
            let max = start.max(end) + padding;
            (min.x, max.x, min.y, max.y)
        });
    let ramps = layout
        .ramps
        .iter()
        .filter(|ramp| {
            let through = ramp.level < settings.level && settings.level <= ramp.level + ramp.levels;
            let standing = ramp.level == settings.level && ramp.shape == RampShape::Solid;
            ramp.carrier == CarrierId::WORLD && (through || standing)
        })
        .map(Ramp::bounds_xz);
    let bridges = layout
        .light_bridges
        .iter()
        .filter(|bridge| bridge.carrier == CarrierId::WORLD && bridge.level == settings.level)
        .map(LightBridge::bounds_xz);
    Grounds::new(floors.chain(walls).chain(ramps).chain(bridges), y, settings.clone())
}

#[cfg(test)]
#[path = "tests/grounds.rs"]
mod tests;
