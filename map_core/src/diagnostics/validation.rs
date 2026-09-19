//! Structured source diagnostics; callers keep the original editable document.
use super::{nesting, placed_definitions, records, surfaces};
use crate::{
    authoring::actor_count_error,
    geometry::{overlap, ramp_error, ramp_levels, zone_rect},
    schema::FireworksConfig,
    transforms,
    values::*,
};
use anyhow::{Result, bail, ensure};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;

// A warning names something inert until another record exists (a switch no
// plate operates yet): Check Map lists it, but it blocks no edit, no save,
// and no load, so authoring order never matters.
#[derive(Clone, Debug, Serialize)]
pub struct Issue {
    pub message: String,
    pub level: Option<i64>,
    pub rect: Option<[i64; 4]>,
    pub map_name: Option<String>,
    pub warning: bool,
}
#[derive(Default)]
pub(super) struct Errors {
    issues: Vec<Issue>,
    level: Option<i64>,
    rect: Option<[i64; 4]>,
}
impl Errors {
    pub(super) fn locate(&mut self, name: &str, entry: &Value, level: Option<i64>) {
        self.level = Some(transforms::record_levels(entry, level)[0]);
        self.rect = Some(transforms::record_rect(name, entry));
    }
    pub(super) fn add(&mut self, message: impl Into<String>) {
        self.push(message.into(), false);
    }
    pub(super) fn warn(&mut self, message: impl Into<String>) {
        self.push(message.into(), true);
    }
    fn push(&mut self, message: String, warning: bool) {
        self.issues.push(Issue {
            message,
            level: self.level,
            rect: self.rect,
            map_name: None,
            warning,
        });
    }
}
pub(super) fn known(v: &Value) -> String {
    let names: Vec<_> = array(v).iter().filter_map(Value::as_str).collect();
    if names.is_empty() {
        "(none listed)".into()
    } else {
        names.join(", ")
    }
}
pub(super) fn contains(values: &Value, value: &Value) -> bool {
    array(values).contains(value)
}
pub(super) fn check_kind(kind: &Value, kinds: &Value) -> bool {
    !kinds.is_null() && !contains(kinds, kind)
}
pub(crate) fn checkpoint_numbers(geometries: &[&Value]) -> BTreeSet<i64> {
    geometries
        .iter()
        .flat_map(|v| list(v, "checkpoints"))
        .filter_map(|c| c["number"].as_i64().filter(|n| *n > 0))
        .collect()
}
pub(crate) fn plates(geometries: &[&Value]) -> BTreeSet<String> {
    geometries
        .iter()
        .flat_map(|v| list(v, "pressure_plates"))
        .filter_map(|p| p["switch"].as_str().filter(|s| !s.is_empty()).map(str::to_owned))
        .collect()
}
pub(super) fn switch_target(entry: &Value, label: &str, context: &Value, errors: &mut Errors) {
    if entry.get("initially_on").is_some_and(|v| !v.is_boolean()) {
        errors.add(format!("{label} initially_on must be true or false"));
    }
    let switch = &entry["switch"];
    if switch.is_null() {
        return;
    }
    if !truth(switch) {
        errors.add(format!("{label} has an empty switch"));
    } else if check_kind(switch, &context["switches"]) {
        errors.add(format!(
            "{label} names unknown switch {}; known: [{}]",
            repr(switch),
            known(&context["switches"])
        ));
    } else if check_kind(switch, &context["plated_switches"]) {
        errors.warn(format!(
            "{label} names switch {}, which no pressure plate operates",
            repr(switch)
        ));
    }
}
pub fn validate_catalog(catalog: &str, entries: &Value) -> Result<()> {
    let Some(values) = entries.as_array() else {
        bail!("{catalog}: expected a list of definitions");
    };
    ensure!(
        values.iter().all(Value::is_object),
        "{catalog}: expected a list of definitions"
    );
    ensure!(
        catalog != "field_kinds" || values.len() <= 256,
        "field_kinds: at most 256 kinds fit in the key inventory"
    );
    let mut seen = BTreeSet::new();
    for entry in values {
        let name = s(entry, "id");
        ensure!(
            !name.trim().is_empty() && name == name.trim(),
            "{catalog}: names must be nonempty, unique, and have no surrounding spaces"
        );
        ensure!(
            seen.insert(name),
            "{catalog}: duplicate id {name:?}; names must be unique"
        );
        let color = &entry["color"];
        if !color.is_null() {
            ensure!(
                color.as_str().is_some_and(|c| c.len() == 7
                    && c.starts_with('#')
                    && c[1..].bytes().all(|b| b.is_ascii_hexdigit())),
                "{name}: color must look like #rrggbb"
            );
        }
        ensure!(catalog == "switches" || !color.is_null(), "{name}: a color is required");
        if catalog == "switches" {
            for (field, choices) in [
                ("activation", &["momentary", "toggle", "auto"][..]),
                ("reset_on_player_death", &["never", "solo", "any", "all"][..]),
                ("held", &["any", "everyone"][..]),
            ] {
                let value =
                    entry
                        .get(field)
                        .and_then(Value::as_str)
                        .or(if field == "held" && entry.get(field).is_none() {
                            Some("any")
                        } else {
                            None
                        });
                ensure!(
                    value.is_some_and(|s| choices.contains(&s)),
                    "{name}: {field} must be one of {}",
                    choices.join(", ")
                );
            }
        }
    }
    Ok(())
}
pub fn validate_map(data: &Value, context: &Value) -> Vec<Issue> {
    let mut errors = Errors::default();
    let cols = i(data, "grid_cols");
    let rows = i(data, "grid_rows");
    let levels = list(data, "levels");
    if cols <= 0 || rows <= 0 {
        errors.add("grid_cols and grid_rows must be positive");
    }
    if levels.is_empty() {
        errors.add("at least one level is required");
    }
    // Bootstrap's MissileAirGrid stores the count, not the highest index, in a u8.
    if levels.len() > 255 {
        errors.add(format!("at most 255 levels are supported (found {})", levels.len()));
    }
    let numbers = if context["checkpoint_numbers"].is_null() {
        checkpoint_numbers(&[data])
    } else {
        array(&context["checkpoint_numbers"]).iter().map(int).collect()
    };
    for (idx, zone) in list(data, "actor_spawn_zones").iter().enumerate() {
        let label = format!("actor_spawn_zones[{idx}]");
        errors.locate("actor_spawn_zones", zone, None);
        records::zone_rect(zone, &label, data, &mut errors);
        if !nonnegative(&get(zone, "roam_distance", json!(0.0))) {
            errors.add(format!("{label} roam_distance must be a finite non-negative number"));
        }
        if !truth(&zone["kind"]) {
            errors.add(format!("{label} has empty `kind`"));
        } else if check_kind(&zone["kind"], &context["actor_kinds"]) {
            errors.add(format!("{label} has unknown actor kind {}", repr(&zone["kind"])));
        }
        if let Some(error) = actor_count_error(&zone["count"]) {
            errors.add(format!("{label} {error}"));
        }
        match zone.get("respawn_secs") {
            None => errors.add(format!(
                "{label} needs `respawn_secs` (seconds, or null to never refill)"
            )),
            Some(v) if !v.is_null() && !nonnegative(v) => {
                errors.add(format!("{label} respawn_secs must be a non-negative number or null"))
            }
            _ => {}
        }
        if !nonnegative(&get(zone, "beam_in_secs", json!(0.0))) {
            errors.add(format!("{label} beam_in_secs must be a finite non-negative number"));
        }
        switch_target(zone, &label, context, &mut errors);
        records::zone_course(
            zone,
            &label,
            if context["defer_course_references"] == true {
                None
            } else {
                Some(&numbers)
            },
            &mut errors,
        );
    }
    records::checkpoints(data, &mut errors);
    records::items(data, context, &mut errors);
    records::pressure_plates(data, context, &mut errors);
    surfaces::validate(data, context, &mut errors);
    // A cell holds one slope and one way down to a slope, so two ramps may
    // share cells only where one ends on the level the other starts from.
    let ramps = list(data, "ramps");
    for (index, ramp) in ramps.iter().enumerate() {
        errors.locate("ramps", ramp, None);
        let span = |r: &Value| (i(r, "lower_level"), i(r, "lower_level") + ramp_levels(r));
        let (lower, upper) = span(ramp);
        if ramps[..index]
            .iter()
            .any(|other| span(other).0 == lower && zone_rect(other) == zone_rect(ramp))
        {
            errors.add(format!("ramp {}: duplicates another ramp", repr(ramp)));
        } else if ramps[..index].iter().any(|other| {
            let (other_lower, other_upper) = span(other);
            lower < other_upper && other_lower < upper && overlap(zone_rect(other), zone_rect(ramp))
        }) {
            errors.add(format!("ramp {}: overlaps another ramp", repr(ramp)));
        }
        if let Some(error) = ramp_error(ramp, cols, rows, levels.len() as i64) {
            errors.add(format!("ramp {}: {error}", repr(ramp)));
        }
    }
    nesting::ladders(data, &mut errors);
    nesting::validate(data, context, &mut errors);
    surfaces::materials(data, context, &mut errors);
    // A source serialized from the typed schema already has its shape.
    if errors.issues.is_empty() && context["typed_source"] != true {
        errors.issues.extend(super::shape::validate(data));
    }
    errors.issues
}
pub fn validate_document(root: &Value, context: &Value) -> Vec<Issue> {
    let definitions = &root["nested_geometry"];
    let mut errors = Errors::default();
    for catalog in ["switches", "field_kinds"] {
        if let Err(error) = validate_catalog(catalog, &get(root, catalog, json!([]))) {
            errors.add(error.to_string());
            break;
        }
    }
    if root.get("fireworks").is_none() {
        errors.add("fireworks requires an object or explicit null");
    }
    let placed = placed_definitions(root, definitions);
    let mut used = vec![root];
    if let Some(defs) = placed.as_object() {
        used.extend(defs.values());
    }
    let mut context = context.clone();
    context["plated_switches"] = json!(plates(&used));
    context["checkpoint_numbers"] = json!(checkpoint_numbers(&used));
    let fireworks = &root["fireworks"];
    if !fireworks.is_null() {
        if !fireworks.is_object() {
            errors.add("fireworks must be an object or null");
        } else {
            let before = errors.issues.len();
            if !truth(&fireworks["switch"]) {
                errors.add("fireworks requires a switch");
            }
            if fireworks.get("initially_on").is_some() {
                errors.add("fireworks has no initial state; remove initially_on");
            }
            switch_target(fireworks, "fireworks", &context, &mut errors);
            if !nonnegative(&fireworks["cooldown_secs"]) {
                errors.add("fireworks cooldown_secs must be finite and nonnegative");
            }
            if errors.issues.len() == before
                && let Err(error) = serde_json::from_value::<FireworksConfig>(fireworks.clone())
            {
                errors.add(format!("fireworks: {error}"));
            }
        }
    }
    let mut geometries = vec![(None, root)];
    if let Some(defs) = definitions.as_object() {
        geometries.extend(defs.iter().map(|(name, v)| (Some(name.as_str()), v)));
    }
    let mut shapes = json!({});
    if let Some(defs) = definitions.as_object() {
        for (name, v) in defs {
            shapes[name] = json!({"grid_cols":v["grid_cols"],"grid_rows":v["grid_rows"],"level_count":list(v,"levels").len(),"nested_names":list(v,"nested_maps").iter().map(|e|e["map"].clone()).collect::<Vec<_>>()});
        }
    }
    context["nested_shapes"] = shapes;
    for (name, data) in geometries {
        let prefix = name.map(|n| format!("Nested {n}: ")).unwrap_or_default();
        let mut found = vec![];
        if let Some(name) = name {
            let mut add = |message: String| {
                found.push(Issue {
                    message,
                    level: None,
                    rect: None,
                    map_name: Some(name.into()),
                    warning: false,
                })
            };
            if !crate::is_valid_geometry_name(name) {
                add(format!("{prefix}the name must be nonempty with no surrounding spaces"));
            }
            for key in ["switches", "field_kinds", "fireworks"] {
                if truth(&data[key]) {
                    add(format!("{prefix}{key}: control definitions belong in the outer map"));
                }
            }
            if data.get("nested_geometry").is_some() {
                add(format!(
                    "{prefix}named geometry belongs in the outer map's nested_geometry"
                ));
            }
        }
        context["map_name"] = json!(name);
        // An unplaced definition is scratch geometry, checked against the tree it
        // would join: the placed one, itself, and whatever it places.
        let scratch = name.is_some_and(|name| placed.get(name).is_none());
        let own = if scratch {
            placed_definitions(data, definitions)
        } else {
            Value::Null
        };
        let mut tree = used.clone();
        if scratch {
            tree.push(data);
            tree.extend(own.as_object().into_iter().flat_map(|defs| defs.values()));
        }
        context["plated_switches"] = json!(plates(&tree));
        context["checkpoint_numbers"] = json!(checkpoint_numbers(&tree));
        let mut issues = validate_map(data, &context);
        for (idx, item) in list(data, "items").iter().enumerate() {
            if item_type(s(item, "type")) && check_kind(&item["type"], &context["pickup_types"]) {
                issues.push(Issue {
                    message: format!(
                        "items[{idx}] {} is always active and cannot be a pickup",
                        s(item, "type")
                    ),
                    level: Some(i(item, "level")),
                    rect: Some(transforms::record_rect("items", item)),
                    map_name: None,
                    warning: false,
                });
            }
        }
        found.extend(issues.into_iter().map(|mut issue| {
            issue.message = format!("{prefix}{}", issue.message);
            issue.map_name = name.map(str::to_owned);
            issue
        }));
        errors.issues.extend(found);
    }
    if !used
        .iter()
        .flat_map(|v| list(v, "checkpoints"))
        .any(|c| whole(&c["number"]) && int(&c["number"]) == 0)
    {
        errors.level = None;
        errors.rect = None;
        errors.add("The placed map has no checkpoint 0, the start.");
    }
    errors.issues
}
pub(super) fn item_type(kind: &str) -> bool {
    kind == common::protocol::ItemType::KEY_CONFIG_ID || common::protocol::ItemType::from_config_id(kind).is_some()
}
