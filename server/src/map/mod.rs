mod blueprint;
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

// Load the building blueprint from disk and compile it to a `MapLayout` +
// `GridConfig`. Hard-fails the server on any parse or validation error so the
// blueprint file stays canonical.
#[must_use]
pub fn generate_grid() -> (MapLayout, GridConfig) {
    let path = blueprint_path();
    let blueprint = blueprint::load_blueprint(&path)
        .unwrap_or_else(|err| panic!("failed to load building blueprint at {}: {err:?}", path.display()));
    blueprint::compile_blueprint(&blueprint)
}

fn blueprint_path() -> PathBuf {
    // Look up the blueprint relative to the server crate's manifest, so it
    // works whether the binary is run via `cargo run` or from the target
    // directory.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("buildings")
        .join("default.toml")
}
