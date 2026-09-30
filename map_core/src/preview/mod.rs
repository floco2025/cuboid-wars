//! The editor's Measure-tool physics over the game's own movement: open-air
//! flights through the real funnel and portal hop, fall damage, and the
//! regions a floor portal catches a flight in, batched per call.
mod jump;
mod operations;
mod physics;
mod polygons;
mod regions;
mod trajectory;

pub use operations::dispatch;

#[cfg(test)]
#[path = "tests/fixtures.rs"]
mod test_fixtures;
