use bevy::math::Vec2;
use common::{
    config::MapGeometryConfig,
    map::{Grounds, GroundsSettings},
    protocol::{CarrierId, Floor, LightBridge, MapLayout, Ramp},
};

// The cutout is one bounding rectangle of the root geometry at the ground
// level, including slab trim. Upper storeys must not leave a hole beneath
// an elevated course; gaps within this rectangle still belong to the map
// (notably basement ramp openings). Carriers cannot cut a static landscape.
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
            let (low, high) = ramp.bounds_y();
            ramp.carrier == CarrierId::WORLD && low <= y && y <= high
        })
        .map(Ramp::bounds_xz);
    let bridges = layout
        .light_bridges
        .iter()
        .filter(|bridge| bridge.carrier == CarrierId::WORLD && bridge.level == settings.level)
        .map(LightBridge::bounds_xz);
    let bounds = floors
        .chain(walls)
        .chain(ramps)
        .chain(bridges)
        .map(|(x1, x2, z1, z2)| (Vec2::new(x1, z1), Vec2::new(x2, z2)))
        .reduce(|(min, max), (next_min, next_max)| (min.min(next_min), max.max(next_max)));
    let (min, max) = bounds.unwrap_or((Vec2::ZERO, Vec2::ZERO));
    Grounds {
        center: ((min + max) * 0.5).to_array(),
        half_size: ((max - min) * 0.5).to_array(),
        y,
        settings: settings.clone(),
    }
}

#[cfg(test)]
#[path = "tests/grounds.rs"]
mod tests;
