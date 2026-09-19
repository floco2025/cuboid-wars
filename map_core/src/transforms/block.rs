use super::records;
use crate::{
    authoring::{EDGE_LISTS, ZONE_LISTS},
    geometry,
    values::*,
};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy)]
struct Plan<'a> {
    width: f64,
    height: f64,
    operation: &'a str,
}
fn n(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() < i64::MAX as f64 {
        json!(value as i64)
    } else {
        json!(value)
    }
}
impl<'a> Plan<'a> {
    fn new(width: f64, height: f64, operation: &'a str) -> Result<Self> {
        ensure!(
            ["rotate", "mirror_x", "mirror_y"].contains(&operation),
            "Unknown transform: {operation}"
        );
        Ok(Self {
            width,
            height,
            operation,
        })
    }
    fn point(self, x: f64, y: f64) -> [f64; 2] {
        match self.operation {
            "rotate" => [self.height - y, x],
            "mirror_x" => [self.width - x, y],
            _ => [x, self.height - y],
        }
    }
    fn vector(self, x: f64, y: f64) -> [f64; 2] {
        match self.operation {
            "rotate" => [-y, x],
            "mirror_x" => [-x, y],
            _ => [x, -y],
        }
    }
    fn rect(self, x: f64, y: f64, w: f64, h: f64) -> [f64; 2] {
        let points = [
            self.point(x, y),
            self.point(x + w, y),
            self.point(x, y + h),
            self.point(x + w, y + h),
        ];
        [
            points.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min),
            points.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min),
        ]
    }
    fn direction(self, side: &str) -> String {
        let dirs = [
            ("N", [0.0, -1.0]),
            ("E", [1.0, 0.0]),
            ("S", [0.0, 1.0]),
            ("W", [-1.0, 0.0]),
        ];
        let Some((_, [x, y])) = dirs.iter().find(|(s, _)| *s == side) else {
            return side.into();
        };
        let v = self.vector(*x, *y);
        dirs.iter()
            .find(|(_, p)| *p == v)
            .expect("transformed side is not cardinal")
            .0
            .into()
    }
}
struct Context<'a> {
    operation: &'a str,
    definitions: &'a Value,
    additions: BTreeMap<String, Value>,
    names: BTreeMap<String, String>,
    visiting: BTreeSet<String>,
}
impl Context<'_> {
    fn definition(&mut self, name: &str) -> Result<String> {
        ensure!(
            !self.visiting.contains(name),
            "Cannot transform cyclic nested geometry."
        );
        let data = self
            .definitions
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Nested geometry '{name}' is missing."))?;
        if let Some(new) = self.names.get(name) {
            return Ok(new.clone());
        }
        let suffix = match self.operation {
            "rotate" => "rotated",
            "mirror_x" => "mirrored_x",
            _ => "mirrored_y",
        };
        let stem = format!("{name}_{suffix}");
        let mut candidate = stem.clone();
        let mut idx = 2;
        while self.definitions.get(&candidate).is_some()
            || self.additions.contains_key(&candidate)
            || self.names.values().any(|v| v == &candidate)
        {
            candidate = format!("{stem}_{idx}");
            idx += 1;
        }
        self.names.insert(name.into(), candidate.clone());
        self.visiting.insert(name.into());
        let transformed = self.geometry(data)?;
        self.additions.insert(candidate.clone(), transformed);
        self.visiting.remove(name);
        Ok(candidate)
    }
    fn geometry(&mut self, data: &Value) -> Result<Value> {
        let mut result = data.clone();
        let plan = Plan::new(number(&data["grid_cols"]), number(&data["grid_rows"]), self.operation)?;
        if self.operation == "rotate" {
            result["grid_cols"] = data["grid_rows"].clone();
            result["grid_rows"] = data["grid_cols"].clone();
        }
        let mut transformed = BTreeMap::<(Option<usize>, String), Vec<Value>>::new();
        for (level, name, original) in records(data) {
            let mut entry = original.clone();
            if ZONE_LISTS.contains(&name) || name == "ramps" {
                let [x0, y0, x1, y1] = geometry::zone_rect(&entry).map(|v| v as f64);
                let [x, y] = plan.rect(x0, y0, x1 - x0, y1 - y0);
                let [w, h] = if self.operation == "rotate" {
                    [y1 - y0, x1 - x0]
                } else {
                    [x1 - x0, y1 - y0]
                };
                entry["cols"] = json!([n(x), n(x + w)]);
                entry["rows"] = json!([n(y), n(y + h)]);
            } else if EDGE_LISTS.contains(&name) {
                let a = plan.point(number(&entry["c0"]), number(&entry["r0"]));
                let b = plan.point(number(&entry["c1"]), number(&entry["r1"]));
                let edge = geometry::normalized_wall([a[0] as i32, a[1] as i32, b[0] as i32, b[1] as i32]);
                for (key, v) in ["c0", "r0", "c1", "r1"].into_iter().zip(edge) {
                    entry[key] = json!(v);
                }
            } else if name == "nested_maps" {
                let original = s(&entry, "map").to_owned();
                let child = self
                    .definitions
                    .get(&original)
                    .ok_or_else(|| anyhow::anyhow!("Nested geometry '{original}' is missing."))?;
                let width = number(&child["grid_cols"]);
                let height = number(&child["grid_rows"]);
                for end in ["from", "to"] {
                    let [x, y] = point(&entry[end]).map(|n| n as f64);
                    ensure!(
                        x >= 0.0 && x + width <= plan.width && y >= 0.0 && y + height <= plan.height,
                        "Include the full nested-map footprints at both ends before transforming."
                    );
                    entry[end] = json!(plan.rect(x, y, width, height).map(n));
                    let key = format!("{end}_nudge");
                    let v = &entry[&key];
                    let [nx, nz] = plan.vector(number(&v[0]), number(&v[2]));
                    // Nudges stay fractions in the file; adding zero clears a mirrored -0.0.
                    entry[&key] = json!([nx + 0.0, v[1], nz + 0.0]);
                }
                entry["map"] = json!(self.definition(&original)?);
            } else {
                let [x, y] = plan.rect(number(&entry["col"]), number(&entry["row"]), 1.0, 1.0);
                entry["col"] = n(x);
                entry["row"] = n(y);
            }
            for key in ["side", "direction"] {
                if let Some(side) = entry[key].as_str() {
                    entry[key] = json!(plan.direction(side));
                }
            }
            let faces = [("north", "N"), ("east", "E"), ("south", "S"), ("west", "W")];
            let old = entry.clone();
            for (face, side) in faces {
                if let Some(value) = old.get(face) {
                    let transformed = plan.direction(side);
                    let key = faces
                        .iter()
                        .find(|(_, s)| *s == transformed)
                        .expect("transformed face is not cardinal")
                        .0;
                    entry[key] = value.clone();
                }
            }
            transformed.entry((level, name.into())).or_default().push(entry);
        }
        for ((level, name), entries) in transformed {
            if let Some(level) = level {
                result["levels"][level][&name] = json!(entries);
            } else {
                result[&name] = json!(entries);
            }
        }
        Ok(result)
    }
}
pub fn transform(data: &Value, operation: &str, definitions: &Value) -> Result<Value> {
    let mut context = Context {
        operation,
        definitions,
        additions: BTreeMap::new(),
        names: BTreeMap::new(),
        visiting: BTreeSet::new(),
    };
    let transformed = context.geometry(data)?;
    Ok(json!([transformed, context.additions]))
}
