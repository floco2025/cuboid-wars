use super::*;

fn square() -> ScorchVariant {
    let corner = |x: f32, y: f32| ScorchVertex {
        position: Vec2::new(x, y),
        color: [x, 0.0, 0.0, x],
    };
    ScorchVariant {
        vertices: vec![corner(0.0, 0.0), corner(1.0, 0.0), corner(1.0, 1.0), corner(0.0, 1.0)],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
    }
}

fn area(variant: &ScorchVariant) -> f32 {
    variant
        .triangles
        .iter()
        .map(|triangle| {
            let [a, b, c] = triangle.map(|index| variant.vertices[index as usize].position);
            (b - a).perp_dot(c - a).abs() * 0.5
        })
        .sum()
}

fn left_of(x: f32) -> HalfPlane {
    HalfPlane {
        normal: Vec2::X,
        offset: x,
    }
}

fn right_of(x: f32) -> HalfPlane {
    HalfPlane {
        normal: Vec2::NEG_X,
        offset: -x,
    }
}

#[test]
fn no_keep_region_leaves_the_geometry_whole() {
    let clipped = ClipRegion::default().apply(&square());
    assert!((area(&clipped) - 1.0).abs() < 1e-5);
    assert_eq!(clipped.triangles.len(), 2);
}

#[test]
fn keep_region_cuts_to_its_inside() {
    let region = ClipRegion {
        keep: vec![vec![left_of(0.5)]],
        cut: Vec::new(),
    };
    let clipped = region.apply(&square());
    assert!((area(&clipped) - 0.5).abs() < 1e-5);
    assert!(clipped.vertices.iter().all(|vertex| vertex.position.x <= 0.5 + 1e-6));
}

#[test]
fn cut_region_removes_only_its_inside() {
    let region = ClipRegion {
        keep: Vec::new(),
        cut: vec![vec![right_of(0.25), left_of(0.5)]],
    };
    let clipped = region.apply(&square());
    assert!((area(&clipped) - 0.75).abs() < 1e-5);
    assert!(
        clipped
            .vertices
            .iter()
            .all(|vertex| vertex.position.x <= 0.25 + 1e-6 || vertex.position.x >= 0.5 - 1e-6)
    );
}

#[test]
fn overlapping_keep_regions_draw_the_overlap_once() {
    let region = ClipRegion {
        keep: vec![vec![left_of(0.75)], vec![right_of(0.25)]],
        cut: Vec::new(),
    };
    let clipped = region.apply(&square());
    assert!((area(&clipped) - 1.0).abs() < 1e-5);
}

#[test]
fn vertices_on_a_cut_take_interpolated_colours() {
    let region = ClipRegion {
        keep: vec![vec![left_of(0.5)]],
        cut: Vec::new(),
    };
    let clipped = region.apply(&square());
    let on_cut: Vec<_> = clipped
        .vertices
        .iter()
        .filter(|vertex| (vertex.position.x - 0.5).abs() < 1e-6)
        .collect();
    assert!(!on_cut.is_empty());
    assert!(on_cut.iter().all(|vertex| (vertex.color[3] - 0.5).abs() < 1e-6));
}
