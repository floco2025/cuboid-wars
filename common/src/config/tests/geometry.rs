use super::*;
use crate::test_geometry::{FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_HEIGHT, sizes};

#[test]
fn wall_height_is_the_pitch_minus_the_slab() {
    assert_eq!(sizes().wall_height(), WALL_HEIGHT);
    assert_eq!(sizes().wall_height() + FLOOR_THICKNESS, LEVEL_HEIGHT);
}

#[test]
fn level_for_y_tolerates_a_small_dip_and_clamps_below_ground() {
    let sizes = sizes();
    assert_eq!(sizes.level_for_y(0.0), 0);
    assert_eq!(sizes.level_for_y(-0.4), 0);
    assert_eq!(sizes.level_for_y(-10.0), 0);
    assert_eq!(sizes.level_for_y(LEVEL_HEIGHT - 0.2), 1);
    assert_eq!(sizes.level_for_y(LEVEL_HEIGHT - 0.6), 0);
    assert_eq!(sizes.level_for_y(2.0 * LEVEL_HEIGHT + 1.0), 2);
}

#[test]
fn nearest_level_rounds_to_the_closest_surface() {
    let sizes = sizes();
    assert_eq!(sizes.nearest_level_to_y(0.4 * LEVEL_HEIGHT), 0);
    assert_eq!(sizes.nearest_level_to_y(0.6 * LEVEL_HEIGHT), 1);
    assert_eq!(sizes.nearest_level_to_y(-3.0), 0);
}

#[test]
fn validation_names_the_bad_field() {
    let zero_cell = MapGeometryConfig {
        grid_cell_size: 0.0,
        ..sizes()
    };
    let error = zero_cell
        .validate("maps.hotel.geometry")
        .expect_err("zero cell size accepted");
    assert!(
        error.to_string().contains("maps.hotel.geometry.grid_cell_size"),
        "{error}"
    );

    let no_wall = MapGeometryConfig {
        floor_thickness: LEVEL_HEIGHT,
        ..sizes()
    };
    let error = no_wall
        .validate("maps.hotel.geometry")
        .expect_err("slab as thick as the storey accepted");
    assert!(error.to_string().contains("floor_thickness"), "{error}");
}
