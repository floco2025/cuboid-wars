use serde_json::{Value, json};
use std::cmp::Ordering;

pub fn array(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or(&[])
}
pub fn list<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    array(&value[key])
}
pub fn int(value: &Value) -> i64 {
    value.as_i64().or_else(|| value.as_f64().map(|n| n as i64)).unwrap_or(0)
}
pub fn i(value: &Value, key: &str) -> i64 {
    int(&value[key])
}
pub fn number(value: &Value) -> f64 {
    value
        .as_f64()
        .unwrap_or_else(|| match value["$map_core_float"].as_str() {
            Some("nan") => f64::NAN,
            Some("inf") => f64::INFINITY,
            Some("-inf") => f64::NEG_INFINITY,
            _ => f64::NAN,
        })
}
pub fn s<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}
pub fn get(value: &Value, key: &str, default: Value) -> Value {
    value.get(key).cloned().unwrap_or(default)
}
pub fn nonnegative(value: &Value) -> bool {
    value.is_number() && number(value).is_finite() && number(value) >= 0.0
}
pub fn whole(value: &Value) -> bool {
    value.is_i64() || value.is_u64()
}
pub fn point(value: &Value) -> [i64; 2] {
    [int(&value[0]), int(&value[1])]
}
pub fn truth(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
        Value::Number(_) => number(value) != 0.0,
    }
}
pub fn py_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "NoneType",
        Value::Bool(_) => "bool",
        Value::String(_) => "str",
        Value::Array(_) => "list",
        Value::Number(n) if n.is_i64() || n.is_u64() => "int",
        Value::Number(_) => "float",
        Value::Object(o) if o.contains_key("$map_core_float") => "float",
        _ => "dict",
    }
}
pub fn repr(value: &Value) -> String {
    match value {
        Value::Null => "None".into(),
        Value::Bool(b) => if *b { "True" } else { "False" }.into(),
        Value::String(s) => format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'")),
        Value::Array(v) => format!("[{}]", v.iter().map(repr).collect::<Vec<_>>().join(", ")),
        Value::Object(o) if o.contains_key("$map_core_float") => o["$map_core_float"].as_str().unwrap_or("nan").into(),
        Value::Object(o) => format!(
            "{{{}}}",
            o.iter()
                .map(|(k, v)| format!("{}: {}", repr(&json!(k)), repr(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        _ => value.to_string(),
    }
}
pub fn cmp(a: &Value, b: &Value) -> Ordering {
    match (a, b) {
        (Value::Array(a), Value::Array(b)) => {
            for (a, b) in a.iter().zip(b) {
                let order = cmp(a, b);
                if order != Ordering::Equal {
                    return order;
                }
            }
            a.len().cmp(&b.len())
        }
        (Value::Number(_), Value::Number(_)) => number(a).total_cmp(&number(b)),
        (Value::String(a), Value::String(b)) => a.cmp(b),
        (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
        _ => py_type(a).cmp(py_type(b)).then_with(|| repr(a).cmp(&repr(b))),
    }
}

fn float_value(value: f32) -> Value {
    if value.is_finite() {
        serde_json::json!(value)
    } else {
        serde_json::json!({"$map_core_float": if value.is_nan() { "nan" } else if value.is_sign_positive() { "inf" } else { "-inf" }})
    }
}
pub fn serialize_number<S: serde::Serializer>(value: &f32, serializer: S) -> Result<S::Ok, S::Error> {
    serde::Serialize::serialize(&float_value(*value), serializer)
}
pub fn serialize_optional_number<S: serde::Serializer>(value: &Option<f32>, serializer: S) -> Result<S::Ok, S::Error> {
    serde::Serialize::serialize(&value.map(float_value), serializer)
}
pub fn serialize_nudge<S: serde::Serializer>(value: &[f32; 3], serializer: S) -> Result<S::Ok, S::Error> {
    serde::Serialize::serialize(&value.map(float_value), serializer)
}

#[derive(Clone, Debug)]
pub struct SortKey(pub Value);
impl PartialEq for SortKey {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}
impl Eq for SortKey {}
impl PartialOrd for SortKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for SortKey {
    fn cmp(&self, other: &Self) -> Ordering {
        cmp(&self.0, &other.0)
    }
}
