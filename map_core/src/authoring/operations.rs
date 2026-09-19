//! Editable source documents. Normalization preserves invalid policy values;
//! canonicalization is the editor's explicit repair after a geometry edit.
use super::{canonical::*, normalize::*, placement::*};
use crate::{diagnostics, geometry, transforms, values::*};
use anyhow::{Result, bail};
use serde_json::{Value, json};
pub const FACES: [&str; 6] = ["top", "bottom", "north", "south", "east", "west"];
pub const TERRAIN_FACES: [&str; 5] = ["bottom", "north", "south", "east", "west"];
pub const CELL_LISTS: [&str; 5] = ["floors", "inaccessible_floors", "terrain", "light_bridges", "lights"];
pub const EDGE_LISTS: [&str; 3] = ["walls", "barriers", "erasers"];
pub const GLOBAL_LISTS: [&str; 7] = [
    "actor_spawn_zones",
    "checkpoints",
    "items",
    "pressure_plates",
    "ramps",
    "ladders",
    "nested_maps",
];
pub const ZONE_LISTS: [&str; 2] = ["actor_spawn_zones", "checkpoints"];
pub fn level_lists() -> impl Iterator<Item = &'static str> {
    CELL_LISTS.into_iter().chain(EDGE_LISTS)
}

pub fn dispatch(op: &str, a: &Value) -> Result<Value> {
    Ok(match op {
        "floor_rectangles" => json!(geometry::floor_rectangles(
            serde_json::from_value(a[0].clone())?,
            number(&a[1]),
            serde_json::from_value(a[2].clone())?,
            serde_json::from_value(a[3].clone())?
        )),
        "empty_level" => empty_level(int(&a[0])),
        "level_label" => json!(level_label(&a[0], int(&a[1]))),
        "empty_map" => empty_map(int(&a[0]), int(&a[1])),
        "started_map" => {
            let mut v = empty_map(int(&a[0]), int(&a[1]));
            v["fireworks"] = Value::Null;
            let mut floors = vec![];
            for c in 0..i(&v, "grid_cols").min(2) {
                for r in 0..i(&v, "grid_rows").min(2) {
                    floors.push(json!({"col":c,"row":r,"all":a[2]}));
                }
            }
            v["levels"][0]["floors"] = json!(floors);
            v
        }
        "expand_face_materials" => expand_materials(&a[0], &FACES),
        "expand_terrain_materials" => expand_materials(&a[0], &TERRAIN_FACES),
        "compact_face_materials" => compact_materials(&a[0], &FACES),
        "compact_terrain_materials" => compact_materials(&a[0], &TERRAIN_FACES),
        "normalize_map" => {
            anyhow::ensure!(a[0].is_object(), "map must be an object");
            normalize_map(&a[0])
        }
        "canonicalize_map" => canonicalize_map(&a[0]),
        "merge_map_settings" => crate::settings::merge_map_settings(&a[0], &a[1])?,
        "enforce_ramp_floor_rules" => {
            let mut v = a[0].clone();
            enforce_ramp_floor_rules(&mut v);
            v
        }
        "control_fields" => select(&a[0], &["switch", "initially_on"]),
        "normalize_floor"
        | "normalize_terrain"
        | "normalize_wall"
        | "normalize_eraser"
        | "normalize_barrier"
        | "normalize_light_bridge"
        | "normalize_ramp"
        | "normalize_ladder"
        | "normalize_nested_map"
        | "normalize_light"
        | "normalize_actor_spawn_zone"
        | "normalize_checkpoint"
        | "normalize_item"
        | "normalize_pressure_plate" => normalize_record(op.trim_start_matches("normalize_"), &a[0]),
        "edge_key" => json!(geometry::edge(&a[0])),
        "ladder_key" | "light_key" | "nested_map_key" | "pressure_plate_key" | "actor_zone_key" | "checkpoint_key" => {
            record_key(op.trim_end_matches("_key"), &a[0])
        }
        "zone_key" => record_key(
            if a[0] == "actor_spawn_zones" {
                "actor_zone"
            } else {
                "checkpoint"
            },
            &a[1],
        ),
        "ladder_edge_key" => json!(geometry::wall_endpoints(
            i(&a[0], "col") as i32,
            i(&a[0], "row") as i32,
            s(&a[0], "side")
        )?),
        "ladders_overlap" => json!(ladders_overlap(&a[0], &a[1])),
        "ladder_spans_level" => {
            json!(i(&a[0], "lower_level") <= int(&a[1]) && int(&a[1]) <= i(&a[0], "lower_level") + i(&a[0], "levels"))
        }
        "nested_map_spans_level" => {
            let low = i(&a[0], "level").min(i(&a[0], "to_level"));
            let high = i(&a[0], "level").max(i(&a[0], "to_level")) + int(&a[2]).max(1) - 1;
            json!(low <= int(&a[1]) && int(&a[1]) <= high)
        }
        "item_cell_error" | "plate_cell_error" => json!(cell_error(
            &a[0],
            int(&a[1]),
            int(&a[2]),
            int(&a[3]),
            op == "plate_cell_error"
        )),
        "light_placement_error" => json!(light_error(
            &a[0],
            int(&a[1]),
            int(&a[2]),
            int(&a[3]),
            a[4].as_str().unwrap_or("")
        )),
        "actor_count_error" => json!(actor_count_error(&a[0])),
        "validate_map" => json!(diagnostics::validate_map(&a[0], &a[1])),
        "validate_document" => json!(diagnostics::validate_document(&a[0], &a[1])),
        "validate_catalog" => {
            diagnostics::validate_catalog(a[0].as_str().unwrap_or(""), &a[1])?;
            Value::Null
        }
        "document_checkpoint_numbers" => json!(diagnostics::checkpoint_numbers(
            &array(&a[0]).iter().collect::<Vec<_>>()
        )),
        "plated_switches" => json!(diagnostics::plates(&array(&a[0]).iter().collect::<Vec<_>>())),
        "placed_definitions" => diagnostics::placed_definitions(&a[0], &a[1]),
        "nested_map_cycle" => json!(diagnostics::nested_cycle(a[0].as_str(), array(&a[1]), &a[2])),
        "translate_entry" | "translate_map" | "resize_map_offset" | "record_rect" | "record_levels"
        | "map_content_bounds" | "remap_levels" | "insert_level_data" | "remove_level_data" | "edit_levels_data"
        | "transform_block" => transforms::dispatch(op, a)?,
        "normalized_wall"
        | "wall_endpoints_for_cell_side"
        | "ramp_cells_on_level"
        | "ramp_error"
        | "ramp_slope"
        | "zone_rect"
        | "rects_overlap"
        | "grid_point_in_bounds"
        | "nested_map_rest_points"
        | "nested_map_footprints"
        | "nested_map_starts_at_end_2"
        | "ramp_landing_edges" => geometry::dispatch(op, a)?,
        _ => bail!("Unknown map operation: {op}"),
    })
}
