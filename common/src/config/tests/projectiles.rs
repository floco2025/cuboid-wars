use super::*;

fn multi_shot(stencil: &[&str]) -> Result<MultiShotPatternConfig> {
    let rows: Vec<String> = stencil.iter().map(|row| (*row).to_owned()).collect();
    MultiShotPatternConfig::from_stencil("multi_shot", 2.0, 3.0, &rows)
}

fn pattern() -> MultiShotPattern {
    MultiShotPattern {
        column_scale: 1.0,
        row_scale: 1.0,
        stencil: vec!["xo".to_owned()],
    }
}

#[test]
fn selects_and_scales_a_named_pattern() {
    let patterns = HashMap::from([(
        "line".to_owned(),
        MultiShotPattern {
            column_scale: 1.5,
            ..pattern()
        },
    )]);
    let selected = MultiShotConfig::try_from(MultiShotSource {
        spread_degrees: 2.0,
        allowed_patterns: vec!["line".to_owned()],
        patterns: patterns.clone(),
    })
    .expect("named pattern rejected");
    let (yaw, pitch) = selected.shot_offsets(1).expect("named pattern missing")[0];
    assert!((yaw - 3.0_f32.to_radians()).abs() < 1e-6 && pitch == 0.0);

    let missing = MultiShotConfig::try_from(MultiShotSource {
        spread_degrees: 2.0,
        allowed_patterns: vec!["ring".to_owned()],
        patterns,
    });
    assert!(missing.expect_err("unknown name accepted").to_string().contains("ring"));
}

#[test]
fn allowed_patterns_are_ordered_and_may_leave_dormant_patterns() {
    let patterns = HashMap::from([
        ("dormant_2".to_owned(), pattern()),
        ("second_2".to_owned(), pattern()),
        ("first_2".to_owned(), pattern()),
    ]);
    let config = MultiShotConfig::try_from(MultiShotSource {
        spread_degrees: 2.0,
        allowed_patterns: vec!["first_2".to_owned(), "second_2".to_owned()],
        patterns: patterns.clone(),
    })
    .expect("ordered allowed patterns rejected");
    assert_eq!(config.allowed_patterns(), ["first_2", "second_2"]);
    assert!(config.allowed_pattern(2).is_none());

    let duplicate = MultiShotConfig::try_from(MultiShotSource {
        spread_degrees: 2.0,
        allowed_patterns: vec!["first_2".to_owned(), "first_2".to_owned()],
        patterns,
    });
    assert!(
        duplicate
            .expect_err("duplicate accepted")
            .to_string()
            .contains("duplicate")
    );
}

#[test]
fn offsets_are_measured_from_the_aim() {
    let column = 2.0_f32.to_radians();
    let row = 3.0_f32.to_radians();
    assert_eq!(
        multi_shot(&["xox"]).expect("line stencil rejected").shots(),
        &[(column, 0.0), (0.0, 0.0), (-column, 0.0)]
    );
    assert_eq!(
        multi_shot(&["o", "x"]).expect("column stencil rejected").shots(),
        &[(0.0, 0.0), (0.0, -row)]
    );
    assert_eq!(
        multi_shot(&["x..", "..o"]).expect("anchored stencil rejected").shots(),
        &[(2.0 * column, row), (0.0, 0.0)]
    );
}

#[test]
fn name_count_postfix_must_match() {
    let source = |name: &str| MultiShotSource {
        spread_degrees: 2.0,
        allowed_patterns: vec![name.to_owned()],
        patterns: HashMap::from([(name.to_owned(), pattern())]),
    };
    assert!(MultiShotConfig::try_from(source("line_2")).is_ok());
    assert!(MultiShotConfig::try_from(source("line")).is_ok());
    let error = MultiShotConfig::try_from(source("line_3")).expect_err("wrong count accepted");
    assert!(error.to_string().contains("not the 3"));
}

#[test]
fn pattern_is_validated() {
    let error = |pattern: &[&str]| multi_shot(pattern).expect_err("invalid stencil accepted").to_string();
    assert!(error(&["xx", "x"]).contains("width"));
    assert!(error(&["x-x"]).contains("'x', 'o' and '.'"));
    assert!(error(&["..."]).contains("center shot"));
    assert!(error(&["xxx"]).contains("center shot"));
    assert!(error(&["o.o"]).contains("only once"));
    assert!(error(&["oxxxxxxxxx"]).contains("max is"));
    assert!(multi_shot(&["x.x", ".o.", "x.x"]).is_ok());
}
