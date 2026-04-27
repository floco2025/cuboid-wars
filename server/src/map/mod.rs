mod building;
mod floors;
mod grid;
mod helpers;
mod lights;
mod mask;
mod ramps;
mod walls;

use std::path::PathBuf;

use crate::resources::GridConfig;
use common::protocol::MapLayout;

pub use helpers::{cell_center, find_unoccupied_cell, find_unoccupied_cell_not_ramp, grid_coords_from_position};

// Load the building definition from disk and compile it to a `MapLayout` +
// `GridConfig`. Hard-fails the server on any parse or validation error so the
// building file stays canonical.
#[must_use]
pub fn generate_grid() -> (MapLayout, GridConfig) {
    let path = building_path();
    let building = building::load_building(&path)
        .unwrap_or_else(|err| panic!("failed to load building at {}: {err:?}", path.display()));
    building::compile_building(&building)
}

fn building_path() -> PathBuf {
    // Look up the building relative to the server crate's manifest, so it
    // works whether the binary is run via `cargo run` or from the target
    // directory.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("buildings")
        .join("default.json")
}
