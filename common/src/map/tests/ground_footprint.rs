use super::*;

#[test]
fn concave_and_disconnected_bases_leave_the_outdoor_gaps_open() {
    let footprint = GroundFootprint::new([(0.0, 12.0, 0.0, 4.0), (8.0, 12.0, 4.0, 12.0), (16.0, 20.0, 0.0, 12.0)]);
    for (x, z) in [(4.0, 8.0), (14.0, 6.0)] {
        assert!(footprint.distance(x, z) > 0.0);
        assert!(footprint.infill.iter().any(|r| r.distance(x, z) < 0.0));
    }
    for (x, z) in [(2.0, 2.0), (10.0, 8.0), (18.0, 6.0)] {
        assert!(footprint.distance(x, z) < 0.0);
        assert!(footprint.infill.iter().all(|r| r.distance(x, z) > 0.0));
    }
    for row in 0..24 {
        for col in 0..40 {
            let (x, z) = (col as f32 * 0.5 + 0.25, row as f32 * 0.5 + 0.25);
            let pieces = footprint
                .cutouts
                .iter()
                .chain(&footprint.infill)
                .filter(|r| r.distance(x, z) < 0.0)
                .count();
            assert_eq!(pieces, 1, "the bounding area is covered exactly once at {x},{z}");
        }
    }
}

#[test]
fn enclosed_voids_stay_cut_out_but_a_narrow_exterior_passage_is_filled() {
    let frame = [(0.0, 12.0, 0.0, 2.0), (0.0, 12.0, 10.0, 12.0), (0.0, 2.0, 2.0, 10.0)];
    let closed = GroundFootprint::new(frame.into_iter().chain([(10.0, 12.0, 2.0, 10.0)]));
    assert!(closed.distance(6.0, 6.0) < 0.0);
    assert!(closed.infill.is_empty());

    let open = GroundFootprint::new(
        frame
            .into_iter()
            .chain([(10.0, 12.0, 2.0, 5.75), (10.0, 12.0, 6.25, 10.0)]),
    );
    assert!(open.distance(6.0, 6.0) > 0.0);
    assert!(open.distance(11.0, 6.0) > 0.0);
    assert!(open.infill.iter().any(|r| r.distance(11.0, 6.0) <= 0.0));
}

#[test]
fn overlapping_trim_is_unioned_independently_of_source_order() {
    let rectangles = [
        (-10.2, 2.2, -4.2, 0.2),
        (-2.2, 2.2, -4.2, 8.2),
        (5.8, 10.2, -4.2, 8.2),
        (-2.2, 0.0, -0.0, 4.0),
    ];
    let first = GroundFootprint::new(rectangles);
    let reverse = GroundFootprint::new(rectangles.into_iter().rev());
    let config = bincode::config::standard();
    assert_eq!(
        bincode::encode_to_vec(&first, config).expect("encode"),
        bincode::encode_to_vec(&reverse, config).expect("encode")
    );
    assert!(first.distance(0.0, 0.0) < 0.0);
    assert!(first.distance(-4.0, 4.0) > 0.0);
    assert!(first.distance(4.0, 0.0) > 0.0);
}
