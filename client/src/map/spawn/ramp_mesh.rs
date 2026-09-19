use bevy::{asset::RenderAssetUsages, prelude::*, render::render_resource::PrimitiveTopology};
use common::protocol::{Face, FaceMaterials, Ramp};

// One flat face of a ramp's prism, wound counter-clockwise seen from outside.
pub(super) struct RampFace {
    pub face: Face,
    pub normal: Vec3,
    pub triangles: Vec<[Vec3; 3]>,
}

// The prism's two caps and one quad per profile edge. A polygon whose normal
// points at the prism's centre is reversed, so no direction or shape needs
// its winding worked out by hand.
pub(super) fn ramp_faces(ramp: &Ramp) -> Vec<RampFace> {
    let prism = ramp.prism();
    let profile = &prism.profile;
    let centre = prism.points().sum::<Vec3>() / (profile.len() * 2) as f32;

    let mut polygons = vec![
        profile.clone(),
        profile.iter().map(|&point| point + prism.sweep).collect(),
    ];
    for (index, &start) in profile.iter().enumerate() {
        let end = profile[(index + 1) % profile.len()];
        polygons.push(vec![start, end, end + prism.sweep, start + prism.sweep]);
    }

    polygons
        .into_iter()
        .filter_map(|mut polygon| {
            let mut normal = (polygon[1] - polygon[0])
                .cross(polygon[2] - polygon[0])
                .try_normalize()?;
            if normal.dot(polygon[0] - centre) < 0.0 {
                polygon.reverse();
                normal = -normal;
            }
            let triangles = (1..polygon.len() - 1)
                .map(|index| [polygon[0], polygon[index], polygon[index + 1]])
                .collect();
            Some(RampFace {
                face: Face::from_normal(normal),
                normal,
                triangles,
            })
        })
        .collect()
}

// One mesh per material alias, in the carrier's frame. UVs come from each
// vertex's carrier-frame position, so the texture runs on from adjoining
// floors and walls: tops and undersides share the floors' axes, vertical
// faces the walls'.
#[must_use]
pub fn build_ramp_meshes(
    ramp: &Ramp,
    materials: &FaceMaterials,
    tile_size: impl Fn(&str) -> f32,
) -> Vec<(String, Mesh)> {
    let mut batches: Vec<(String, Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[f32; 2]>)> = Vec::new();
    for ramp_face in ramp_faces(ramp) {
        let alias = materials.face(ramp_face.face);
        let tile = tile_size(alias);
        let uv = |point: Vec3| match ramp_face.face {
            Face::Top => [point.z / tile, point.x / tile],
            Face::Bottom => [-point.z / tile, point.x / tile],
            Face::East | Face::West => [point.z / tile, point.y / tile],
            Face::North | Face::South => [point.x / tile, point.y / tile],
        };
        let found = batches.iter().position(|(batch_alias, ..)| batch_alias == alias);
        let index = found.unwrap_or_else(|| {
            batches.push((alias.to_owned(), Vec::new(), Vec::new(), Vec::new()));
            batches.len() - 1
        });
        let (_, positions, normals, uvs) = &mut batches[index];
        for point in ramp_face.triangles.iter().flatten() {
            positions.push(point.to_array());
            normals.push(ramp_face.normal.to_array());
            uvs.push(uv(*point));
        }
    }

    batches
        .into_iter()
        .map(|(alias, positions, normals, uvs)| {
            let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
            let _ = mesh.generate_tangents();
            (alias, mesh)
        })
        .collect()
}

#[cfg(test)]
#[path = "tests/ramp_mesh.rs"]
mod tests;
