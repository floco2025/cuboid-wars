use super::*;
use bevy::mesh::VertexAttributeValues;
use common::map::RockClass;

#[test]
fn swaying_tree_vertices_stay_inside_transformed_bounds() {
    for variant in 0..TREE_VARIANTS {
        let tree = Tree::grow(variant);
        for lod in 0..NEAR_LODS {
            let (wood, leaves) = tree.meshes(lod);
            for mesh in [wood, leaves] {
                let bounds = mesh.compute_aabb().expect("tree bounds");
                let Some(VertexAttributeValues::Float32x3(vertices)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
                    panic!("tree positions");
                };
                for scale in [Vec3::splat(0.8), Vec3::splat(1.6), Vec3::new(0.8, 1.2, 1.6)] {
                    let padded = padded_tree_bounds(bounds, scale);
                    for yaw in [0.0, 0.7, 2.8] {
                        let rotation = Quat::from_rotation_y(yaw);
                        for phase in [0.0_f32, 1.7, 4.2] {
                            for time in [0.0_f32, 2.0, 7.0] {
                                for gust in [0.0_f32, 0.5, 1.0] {
                                    for vertex in vertices.iter().step_by(13) {
                                        let local = Vec3::from_array(*vertex);
                                        let weight = (local.y / 10.0).clamp(0.0, 1.0);
                                        let drive = 0.5 + 0.5 * gust;
                                        let trunk = weight * weight * ((time + phase).sin() * drive + 1.2 * gust);
                                        let branch = weight
                                            * 0.35
                                            * drive
                                            * (time * 1.8 + phase - local.xz().length() * 0.4).sin();
                                        let cross = weight * weight * 0.25 * (time * 0.83 + phase * 1.7).sin();
                                        let wind = Vec3::new(trunk + branch, 0.0, cross) * TREE_WIND_STRENGTH;
                                        let displaced = local + rotation.inverse() * wind / scale;
                                        assert!(
                                            (displaced - Vec3::from(padded.center))
                                                .abs()
                                                .cmple(padded.half_extents.into())
                                                .all()
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn tree_detail_levels_have_valid_surfaces_and_reduce_geometry() {
    for variant in 0..TREE_VARIANTS {
        let tree = Tree::grow(variant);
        let mut previous_triangles = usize::MAX;
        for lod in 0..4 {
            let (wood, leaves) = tree.meshes(lod);
            let mut triangles = 0;
            for mesh in [&wood, &leaves] {
                let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
                    panic!("tree positions missing");
                };
                let Some(VertexAttributeValues::Float32x3(normals)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL) else {
                    panic!("tree normals missing");
                };
                assert!(positions.iter().all(|p| Vec3::from_array(*p).is_finite()));
                assert!(
                    normals
                        .iter()
                        .all(|n| (Vec3::from_array(*n).length() - 1.0).abs() < 0.001)
                );
                let indices: Vec<_> = mesh.indices().expect("tree indices missing").iter().collect();
                for face in indices.chunks_exact(3) {
                    let [a, b, c] = [face[0], face[1], face[2]].map(|i| Vec3::from_array(positions[i]));
                    assert!((b - a).cross(c - a).length_squared() > 1e-12);
                }
                triangles += indices.len() / 3;
            }
            assert!(triangles < previous_triangles);
            previous_triangles = triangles;
        }
    }
}

#[test]
fn a_far_chunk_merges_only_its_trees() {
    let assets = TreeAssets {
        near: Vec::new(),
        far: (0..TREE_VARIANTS)
            .map(|variant| Tree::grow(variant).meshes(FAR_LOD))
            .collect(),
        bark: Handle::default(),
        foliage: Handle::default(),
        far_bark: Handle::default(),
        far_foliage: Handle::default(),
    };
    let decoration = |x: f32, kind: DecorationKind| GroundDecoration {
        position: Vec3::new(x, 0.0, 0.0),
        scale: Vec3::ONE,
        rotation: Quat::IDENTITY,
        variant: 0,
        kind,
    };
    let trees = [
        decoration(0.0, DecorationKind::Tree),
        decoration(20.0, DecorationKind::Tree),
    ];
    let (wood, leaves) = assets
        .far_chunk(&trees, Vec3::ZERO)
        .expect("two trees merge into a chunk");
    let mixed = [
        trees[0],
        decoration(10.0, DecorationKind::Rock(RockClass::Boulder)),
        trees[1],
        decoration(30.0, DecorationKind::Rock(RockClass::Stone)),
    ];
    let (mixed_wood, mixed_leaves) = assets.far_chunk(&mixed, Vec3::ZERO).expect("the trees still merge");
    assert_eq!(
        mixed_wood.count_vertices(),
        wood.count_vertices(),
        "rocks grow no trunks"
    );
    assert_eq!(mixed_leaves.count_vertices(), leaves.count_vertices());
    let rocks = [mixed[1], mixed[3]];
    assert!(
        assets.far_chunk(&rocks, Vec3::ZERO).is_none(),
        "a chunk of rocks has no tree meshes"
    );
}
