use std::collections::HashSet;

use anyhow::{Result, bail};
use serde_json::{Map, Value};

const INHERIT_FROM_KEY: &str = "inherit_from";

// Resolve `inherit_from` references inside `root[actors_key]`. Each kind whose
// value is a JSON object may include `"inherit_from": "<other_kind>"`. After
// resolution, every kind's value is its parent chain deep-merged with the
// child's own fields applied last; the `inherit_from` field is removed.
//
// Inheritance is per-file: parents must be other kinds in the same `actors_key`
// map. Chains of any depth are allowed; cycles and missing parents fail.
//
// Nested objects deep-merge field-by-field; scalars and arrays replace whole.
//
// Errors: missing parent, cycle, non-object kind value, non-string parent.
pub fn resolve_actor_inheritance(root: &mut Value, actors_key: &str) -> Result<()> {
    let Some(actors_value) = root.get_mut(actors_key) else {
        return Ok(());
    };
    let Some(actors) = actors_value.as_object_mut() else {
        bail!("`{actors_key}` must be a JSON object");
    };

    let kind_names: Vec<String> = actors.keys().cloned().collect();
    let mut resolved: Map<String, Value> = Map::new();
    for kind in &kind_names {
        let value = resolve_kind(kind, actors, &mut resolved)?;
        resolved.insert(kind.clone(), value);
    }
    *actors = resolved;
    Ok(())
}

// Resolve one kind, walking its `inherit_from` chain. Uses `resolved` as a
// memo: if a parent has already been resolved, reuse it. Otherwise recurse.
fn resolve_kind(kind: &str, actors: &Map<String, Value>, resolved: &mut Map<String, Value>) -> Result<Value> {
    let mut visited: HashSet<String> = HashSet::new();
    let mut chain: Vec<String> = Vec::new();
    walk_chain(kind, actors, resolved, &mut visited, &mut chain)?;

    // Apply ancestors in order, root parent first, this kind last.
    let mut merged = Value::Object(Map::new());
    for ancestor in chain {
        let value = if let Some(memoized) = resolved.get(&ancestor) {
            memoized.clone()
        } else {
            actors
                .get(&ancestor)
                .expect("ancestor kind missing from actors")
                .clone()
        };
        deep_merge(&mut merged, &value);
    }

    if let Value::Object(map) = &mut merged {
        map.remove(INHERIT_FROM_KEY);
    }
    Ok(merged)
}

// Walk from `kind` upward through `inherit_from`, pushing ancestors onto
// `chain` in root-first order. Stops walking when an ancestor has no
// `inherit_from`, when its parent is already resolved (cached), or when a
// cycle is detected. Validates kind/parent structure along the way.
fn walk_chain(
    kind: &str,
    actors: &Map<String, Value>,
    resolved: &Map<String, Value>,
    visited: &mut HashSet<String>,
    chain: &mut Vec<String>,
) -> Result<()> {
    if !visited.insert(kind.to_string()) {
        let mut path = chain.clone();
        path.push(kind.to_string());
        bail!(
            "actor kind {:?} has cyclic inherit_from chain: {}",
            kind,
            path.join(" -> ")
        );
    }
    let value = actors
        .get(kind)
        .unwrap_or_else(|| panic!("walk_chain called with unknown kind {kind:?}"));
    let object = match value {
        Value::Object(map) => map,
        _ => bail!(
            "actor kind {:?} value must be a JSON object, got {}",
            kind,
            json_type_name(value)
        ),
    };

    if let Some(parent_value) = object.get(INHERIT_FROM_KEY) {
        let parent_name = parent_value.as_str().ok_or_else(|| {
            anyhow::anyhow!(
                "actor kind {:?} has non-string inherit_from (got {})",
                kind,
                json_type_name(parent_value)
            )
        })?;
        if !actors.contains_key(parent_name) {
            bail!("actor kind {:?} inherits from unknown kind {:?}", kind, parent_name);
        }
        if !resolved.contains_key(parent_name) {
            walk_chain(parent_name, actors, resolved, visited, chain)?;
        } else {
            chain.push(parent_name.to_string());
        }
    }
    chain.push(kind.to_string());
    Ok(())
}

fn deep_merge(into: &mut Value, from: &Value) {
    match (into, from) {
        (Value::Object(into_map), Value::Object(from_map)) => {
            for (key, from_value) in from_map {
                match into_map.get_mut(key) {
                    Some(into_value) => deep_merge(into_value, from_value),
                    None => {
                        into_map.insert(key.clone(), from_value.clone());
                    }
                }
            }
        }
        (into_slot, from_value) => {
            *into_slot = from_value.clone();
        }
    }
}

fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn resolve(value: &mut Value) -> Result<()> {
        resolve_actor_inheritance(value, "actors")
    }

    #[test]
    fn passthrough_when_no_inheritance() {
        let mut input = json!({
            "actors": {
                "a": { "speed": 3.0 },
                "b": { "speed": 5.0 }
            }
        });
        let expected = input.clone();
        resolve(&mut input).expect("resolve");
        assert_eq!(input, expected);
    }

    #[test]
    fn child_inherits_parent_fields_and_overrides_one() {
        let mut input = json!({
            "actors": {
                "a": { "speed": 3.0, "health": 100 },
                "b": { "inherit_from": "a", "speed": 5.0 }
            }
        });
        resolve(&mut input).expect("resolve");
        assert_eq!(
            input["actors"]["b"],
            json!({ "speed": 5.0, "health": 100 }),
            "b should inherit health and override speed"
        );
        assert!(
            input["actors"]["b"].get("inherit_from").is_none(),
            "inherit_from should be removed from resolved value"
        );
    }

    #[test]
    fn nested_object_deep_merges_preserving_parent_siblings() {
        let mut input = json!({
            "actors": {
                "a": { "collider": { "width": 1.0, "height": 2.0, "depth": 3.0 } },
                "b": { "inherit_from": "a", "collider": { "width": 1.5 } }
            }
        });
        resolve(&mut input).expect("resolve");
        assert_eq!(
            input["actors"]["b"]["collider"],
            json!({ "width": 1.5, "height": 2.0, "depth": 3.0 })
        );
    }

    #[test]
    fn arrays_replace_whole_not_merge() {
        let mut input = json!({
            "actors": {
                "a": { "tags": ["one", "two"] },
                "b": { "inherit_from": "a", "tags": ["three"] }
            }
        });
        resolve(&mut input).expect("resolve");
        assert_eq!(input["actors"]["b"]["tags"], json!(["three"]));
    }

    #[test]
    fn three_deep_chain_resolves_with_child_winning() {
        let mut input = json!({
            "actors": {
                "a": { "x": 1, "y": 2, "z": 3 },
                "b": { "inherit_from": "a", "y": 20 },
                "c": { "inherit_from": "b", "z": 300 }
            }
        });
        resolve(&mut input).expect("resolve");
        assert_eq!(input["actors"]["c"], json!({ "x": 1, "y": 20, "z": 300 }));
        // b is also resolved (used as a parent), and remains valid on its own.
        assert_eq!(input["actors"]["b"], json!({ "x": 1, "y": 20, "z": 3 }));
    }

    #[test]
    fn unknown_parent_is_rejected_with_named_error() {
        let mut input = json!({
            "actors": {
                "b": { "inherit_from": "a", "speed": 5.0 }
            }
        });
        let err = resolve(&mut input).expect_err("missing parent must error");
        let msg = err.to_string();
        assert!(msg.contains("\"b\""), "error names the child: {msg}");
        assert!(msg.contains("\"a\""), "error names the missing parent: {msg}");
    }

    #[test]
    fn two_cycle_is_rejected() {
        let mut input = json!({
            "actors": {
                "a": { "inherit_from": "b" },
                "b": { "inherit_from": "a" }
            }
        });
        let err = resolve(&mut input).expect_err("2-cycle must error");
        assert!(err.to_string().contains("cyclic"), "got: {err}");
    }

    #[test]
    fn three_cycle_is_rejected() {
        let mut input = json!({
            "actors": {
                "a": { "inherit_from": "c" },
                "b": { "inherit_from": "a" },
                "c": { "inherit_from": "b" }
            }
        });
        let err = resolve(&mut input).expect_err("3-cycle must error");
        assert!(err.to_string().contains("cyclic"), "got: {err}");
    }

    #[test]
    fn non_string_inherit_from_is_rejected() {
        let mut input = json!({
            "actors": {
                "a": {},
                "b": { "inherit_from": 42 }
            }
        });
        let err = resolve(&mut input).expect_err("non-string parent must error");
        assert!(err.to_string().contains("non-string"), "got: {err}");
    }

    #[test]
    fn non_object_kind_value_is_rejected() {
        let mut input = json!({
            "actors": {
                "a": "not an object"
            }
        });
        let err = resolve(&mut input).expect_err("non-object kind must error");
        assert!(err.to_string().contains("must be a JSON object"), "got: {err}");
    }

    #[test]
    fn missing_actors_key_is_passthrough() {
        let mut input = json!({ "version": 1, "other": {} });
        let expected = input.clone();
        resolve(&mut input).expect("absence of actors is fine");
        assert_eq!(input, expected);
    }

    #[test]
    fn non_object_actors_value_is_rejected() {
        let mut input = json!({ "actors": [] });
        let err = resolve(&mut input).expect_err("array actors must error");
        assert!(err.to_string().contains("must be a JSON object"), "got: {err}");
    }
}
