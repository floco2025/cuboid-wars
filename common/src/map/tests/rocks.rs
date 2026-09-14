use std::collections::HashMap;

use super::*;

#[test]
fn coarser_levels_keep_the_finer_levels_vertices_in_place() {
    for class in RockClass::ALL {
        for variant in 0..ROCK_VARIANTS {
            let coarse = rock_shape(class, variant, ROCK_HULL_SUBDIVISIONS);
            let fine = rock_shape(class, variant, 3);
            assert_eq!(fine.triangles.len(), coarse.triangles.len() * 16);
            for (index, vertex) in coarse.vertices.iter().enumerate() {
                assert!(
                    vertex.distance(fine.vertices[index]) < 1e-4,
                    "{class:?} {variant} vertex {index} moved"
                );
            }
        }
    }
}

#[test]
fn shapes_are_closed_outward_facing_bodies_about_a_metre_across() {
    for class in RockClass::ALL {
        for variant in 0..ROCK_VARIANTS {
            let shape = rock_shape(class, variant, 2);
            let mut edges: HashMap<(u32, u32), u32> = HashMap::new();
            for &[a, b, c] in &shape.triangles {
                for (from, to) in [(a, b), (b, c), (c, a)] {
                    *edges.entry((from, to)).or_default() += 1;
                }
                let (a, b, c) = (
                    shape.vertices[a as usize],
                    shape.vertices[b as usize],
                    shape.vertices[c as usize],
                );
                let normal = (b - a).cross(c - a);
                assert!(
                    normal.length_squared() > 1e-10,
                    "{class:?} {variant} has a degenerate face"
                );
                assert!(normal.dot(a + b + c) > 0.0, "{class:?} {variant} has an inward face");
            }
            for (&(from, to), &count) in &edges {
                assert_eq!(count, 1);
                assert_eq!(edges.get(&(to, from)), Some(&1), "{class:?} {variant} has an open edge");
            }
            let extent = shape.vertices.iter().fold(Vec3::ZERO, |extent, v| extent.max(v.abs()));
            assert!(extent.max_element() > 0.6 && extent.max_element() < 2.0);
            assert!(extent.y < extent.x, "rocks lie flatter than they are wide");
        }
    }
}

#[test]
fn classes_and_variants_differ() {
    let stone = rock_shape(RockClass::Stone, 0, 1);
    let other_stone = rock_shape(RockClass::Stone, 1, 1);
    let boulder = rock_shape(RockClass::Boulder, 0, 1);
    let differs = |a: &RockShape, b: &RockShape| a.vertices.iter().zip(&b.vertices).any(|(a, b)| a.distance(*b) > 0.05);
    assert!(differs(&stone, &other_stone));
    assert!(differs(&stone, &boulder));
    assert!(!differs(&stone, &rock_shape(RockClass::Stone, ROCK_VARIANTS, 1)));
}
