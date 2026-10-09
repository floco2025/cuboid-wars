use super::*;

#[test]
fn per_face_values_override_the_all_shorthand() {
    let materials: FaceMaterials =
        serde_json::from_str(r#"{"all": "brick", "top": "grass"}"#).expect("mixed form parses");
    assert_eq!(materials.top, "grass");
    for face in [
        &materials.bottom,
        &materials.north,
        &materials.south,
        &materials.east,
        &materials.west,
    ] {
        assert_eq!(face, "brick");
    }
}

#[test]
fn missing_face_without_all_is_rejected() {
    let err =
        serde_json::from_str::<FaceMaterials>(r#"{"top": "grass"}"#).expect_err("incomplete faces must be rejected");
    assert!(err.to_string().contains("all"), "error names the fix: {err}");
}
