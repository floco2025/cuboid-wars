use super::validation::Issue;
use crate::{schema::*, transforms};
use serde::de::DeserializeOwned;
use serde_json::Value;

fn record_error<T: DeserializeOwned>(value: &Value) -> Option<String> {
    serde_json::from_value::<T>(value.clone())
        .err()
        .map(|error| error.to_string())
}

pub(super) fn validate(data: &Value) -> Vec<Issue> {
    let mut issues = Vec::new();
    for (level, name, record) in transforms::records(data) {
        let error = match name {
            "floors" | "inaccessible_floors" => record_error::<FloorDef>(record),
            "terrain" => record_error::<TerrainDef>(record),
            "walls" => record_error::<WallDef>(record),
            "barriers" => record_error::<BarrierDef>(record),
            "erasers" => record_error::<EraserDef>(record),
            "light_bridges" => record_error::<LightBridgeDef>(record),
            "lights" => record_error::<WallLightDef>(record),
            "actor_spawn_zones" => record_error::<ActorSpawnZoneDef>(record),
            "checkpoints" => record_error::<CheckpointDef>(record),
            "items" => record_error::<ItemDef>(record),
            "pressure_plates" => record_error::<PressurePlateDef>(record),
            "ramps" => record_error::<RampDef>(record),
            "ladders" => record_error::<LadderDef>(record),
            "nested_maps" => record_error::<NestedMapDef>(record),
            _ => None,
        };
        if let Some(error) = error {
            issues.push(Issue {
                message: format!("{name}: {error}"),
                level: Some(transforms::record_levels(record, level.map(|n| n as i64))[0]),
                rect: Some(transforms::record_rect(name, record)),
                map_name: None,
            });
        }
    }
    // Entry checks retain locations; the final parse also checks list types,
    // root fields, and numeric representation against the game's source schema.
    if issues.is_empty() {
        let mut source = data.clone();
        // Nested geometries are checked independently with their own locations.
        if let Some(object) = source.as_object_mut() {
            for field in [
                "nested_geometry",
                "switches",
                "barrier_kinds",
                "bridge_kinds",
                "fireworks",
            ] {
                object.remove(field);
            }
        }
        if let Some(error) = record_error::<MapDef>(&source) {
            issues.push(Issue {
                message: error,
                level: None,
                rect: None,
                map_name: None,
            });
        }
    }
    issues
}
