use anyhow::{Context, Result, ensure};
use bevy::math::{UVec3, Vec2, Vec3, Vec3A, Vec3Swizzles};
use common::{
    config::CharacterPhysicsConfig,
    constants::{CHARACTER_CONTACT_OFFSET, CHARACTER_MAX_SLOPE, CHARACTER_STEP_HEIGHT},
    physics::{CollisionMesh, CollisionSource},
    protocol::{CarrierId, FieldId, Position},
};
use rerecast::{AreaType, ConfigBuilder, ConvexVolume, DetailNavmesh, HeightfieldBuilder, PolygonNavmesh, TriMesh};
use std::collections::HashMap;

const LOOKUP_CELL_SIZE: f32 = 4.0;

#[derive(Debug, Clone, Copy)]
pub struct SurfaceBounds {
    pub min: Vec3,
    pub max: Vec3,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceLocation {
    pub carrier: CarrierId,
    pub polygon: usize,
    pub position: Position,
}

pub struct SurfaceMesh {
    pub carrier: CarrierId,
    pub(super) polygons: Vec<Vec<Vec3>>,
    pub(super) neighbors: Vec<Vec<Option<usize>>>,
    pub(super) links: Vec<Vec<super::ladders::SurfaceLink>>,
    pub(super) components: [Vec<usize>; 2],
    detail: DetailNavmesh,
    centers: Vec<Position>,
    spatial: HashMap<(i32, i32), Vec<usize>>,
}

impl SurfaceMesh {
    pub(super) fn voxel_size(physics: CharacterPhysicsConfig) -> (f32, f32) {
        let radius = physics.movement_collider.radius() + CHARACTER_CONTACT_OFFSET;
        // Recast compares the highest and lowest neighboring voxel floors
        // against walkable_climb. Radius-derived voxels can therefore erase
        // motor-walkable ramps, or round a large body's step height to zero.
        // Resolve two slope samples plus one vertical quantization interval
        // within the motor's step height, independently of capsule size.
        let cell_height = (radius / 4.0).min(CHARACTER_STEP_HEIGHT / 4.0);
        let cell_size = (radius / 3.0).min((CHARACTER_STEP_HEIGHT - cell_height) / (2.0 * CHARACTER_MAX_SLOPE.tan()));
        (cell_size, cell_height)
    }

    #[cfg(test)]
    pub fn bake(
        geometry: &[CollisionMesh],
        carrier: CarrierId,
        physics: CharacterPhysicsConfig,
        open: &[FieldId],
    ) -> Result<Self> {
        Self::bake_in(geometry, carrier, physics, open, None, &[])
    }

    pub fn bake_in(
        geometry: &[CollisionMesh],
        carrier: CarrierId,
        physics: CharacterPhysicsConfig,
        open: &[FieldId],
        bounds: Option<SurfaceBounds>,
        excluded: &[SurfaceBounds],
    ) -> Result<Self> {
        physics.movement_collider.validate("navigation.body")?;
        let mut input = TriMesh::default();
        let mut walkable = Vec::new();
        for mesh in geometry
            .iter()
            .filter(|mesh| mesh.carrier == carrier && mesh.field.is_none_or(|field| !open.contains(&field)))
        {
            let offset = u32::try_from(input.vertices.len()).context("too many navigation vertices")?;
            input.vertices.extend(mesh.vertices.iter().copied().map(Vec3A::from));
            input.indices.extend(
                mesh.triangles
                    .iter()
                    .map(|triangle| UVec3::from_array(*triangle) + UVec3::splat(offset)),
            );
            input
                .area_types
                .extend(std::iter::repeat_n(AreaType::NOT_WALKABLE, mesh.triangles.len()));
            walkable.extend(std::iter::repeat_n(
                !matches!(
                    mesh.source,
                    CollisionSource::Wall(_) | CollisionSource::Barrier(_) | CollisionSource::Decoration
                ),
                mesh.triangles.len(),
            ));
        }
        if input.indices.is_empty() {
            return Ok(Self {
                carrier,
                polygons: Vec::new(),
                neighbors: Vec::new(),
                links: Vec::new(),
                components: [Vec::new(), Vec::new()],
                detail: DetailNavmesh::default(),
                centers: Vec::new(),
                spatial: HashMap::new(),
            });
        }
        ensure!(
            input.vertices.iter().all(|vertex| vertex.is_finite()),
            "non-finite navigation geometry"
        );
        let mut aabb = input.compute_aabb().context("navigation geometry has no bounds")?;
        let radius = physics.movement_collider.radius() + CHARACTER_CONTACT_OFFSET;
        aabb.min -= Vec3::splat(radius * 2.0);
        aabb.max += Vec3::splat(radius * 2.0);
        if let Some(bounds) = bounds {
            aabb.min = aabb.min.max(bounds.min);
            aabb.max = aabb.max.min(bounds.max);
            ensure!(
                aabb.min.cmplt(aabb.max).all(),
                "navigation region does not intersect collision geometry"
            );
        }
        let (cell_size, cell_height) = Self::voxel_size(physics);
        let config = ConfigBuilder {
            aabb,
            agent_radius: radius,
            cell_size_fraction: radius / cell_size,
            cell_height_fraction: radius / cell_height,
            agent_height: physics.movement_collider.height + CHARACTER_CONTACT_OFFSET * 2.0,
            walkable_climb: CHARACTER_STEP_HEIGHT,
            walkable_slope_angle: CHARACTER_MAX_SLOPE,
            min_region_size: 0,
            merge_region_size: 0,
            max_simplification_error: 0.5,
            ..Default::default()
        }
        .build();
        let extent = aabb.max - aabb.min;
        ensure!(
            extent.x / config.cell_size < 4096.0
                && extent.z / config.cell_size < 4096.0
                && extent.x * extent.z / config.cell_size.powi(2) <= 4_194_304.0
                && extent.y / config.cell_height < f32::from(u16::MAX),
            "navigation region is too large; split it into smaller regions before baking"
        );
        input.mark_walkable_triangles(config.walkable_slope_angle);
        for (area, allowed) in input.area_types.iter_mut().zip(walkable) {
            if !allowed {
                *area = AreaType::NOT_WALKABLE;
            }
        }
        let mut heightfield = HeightfieldBuilder {
            aabb,
            cell_size: config.cell_size,
            cell_height: config.cell_height,
        }
        .build()?;
        heightfield.rasterize_triangles(&input, config.walkable_climb)?;
        heightfield.filter_low_hanging_walkable_obstacles(config.walkable_climb);
        heightfield.filter_ledge_spans(config.walkable_height, config.walkable_climb);
        heightfield.filter_walkable_low_height_spans(config.walkable_height);
        let mut compact = heightfield.into_compact(config.walkable_height, config.walkable_climb)?;
        for bounds in excluded {
            compact.mark_convex_poly_area(&ConvexVolume {
                vertices: vec![
                    Vec2::new(bounds.min.x, bounds.min.z),
                    Vec2::new(bounds.max.x, bounds.min.z),
                    Vec2::new(bounds.max.x, bounds.max.z),
                    Vec2::new(bounds.min.x, bounds.max.z),
                ],
                min_y: bounds.min.y,
                max_y: bounds.max.y,
                area: AreaType::NOT_WALKABLE,
            });
        }
        compact.erode_walkable_area(config.walkable_radius);
        compact.build_distance_field();
        compact.build_regions(0, config.min_region_area, config.merge_region_area)?;
        let mut contours = compact.build_contours(
            config.max_simplification_error,
            config.max_edge_len,
            config.contour_flags,
        );
        super::contours::join_holes(&mut contours)?;
        let mesh = contours.into_polygon_mesh(config.max_vertices_per_polygon)?;
        // Polygon boundaries describe connectivity, but their planes erase
        // interior hills and valleys. Keep the heightfield's detail so an
        // actor standing on real terrain can still locate and follow a route.
        let detail = DetailNavmesh::new(
            &mesh,
            &compact,
            config.detail_sample_dist,
            config.detail_sample_max_error,
        )?;
        Ok(Self::from_polygons(carrier, mesh, detail))
    }

    fn from_polygons(carrier: CarrierId, mesh: PolygonNavmesh, detail: DetailNavmesh) -> Self {
        let stride = usize::from(mesh.max_vertices_per_polygon);
        let vertices: Vec<Vec3> = mesh
            .vertices
            .iter()
            .map(|vertex| {
                Vec3::new(
                    mesh.aabb.min.x + f32::from(vertex.x) * mesh.cell_size,
                    mesh.aabb.min.y + f32::from(vertex.y) * mesh.cell_height,
                    mesh.aabb.min.z + f32::from(vertex.z) * mesh.cell_size,
                )
            })
            .collect();
        let polygons: Vec<Vec<Vec3>> = mesh
            .polygons
            .chunks(stride)
            .map(|polygon| {
                polygon
                    .iter()
                    .take_while(|&&index| index != PolygonNavmesh::NO_INDEX)
                    .map(|&index| vertices[usize::from(index)])
                    .collect()
            })
            .collect();
        let neighbors = mesh
            .polygon_neighbors
            .chunks(stride)
            .zip(&polygons)
            .map(|(neighbors, polygon)| {
                neighbors
                    .iter()
                    .take(polygon.len())
                    .map(|&index| (index != PolygonNavmesh::NO_CONNECTION).then_some(usize::from(index)))
                    .collect()
            })
            .collect();
        let mut result = Self {
            carrier,
            links: vec![Vec::new(); polygons.len()],
            components: [Vec::new(), Vec::new()],
            detail,
            centers: Vec::new(),
            spatial: HashMap::new(),
            polygons,
            neighbors,
        };
        result.update_components();
        result.centers = result
            .polygons
            .iter()
            .enumerate()
            .map(|(index, polygon)| {
                // A polygon the detail mesh left without triangles keeps its flat center.
                let center = polygon.iter().copied().sum::<Vec3>() / polygon.len() as f32;
                result.project(index, center).unwrap_or(center).into()
            })
            .collect();
        for (index, polygon) in result.polygons.iter().enumerate() {
            let min = polygon.iter().copied().fold(Vec3::splat(f32::INFINITY), Vec3::min);
            let max = polygon.iter().copied().fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
            for x in lookup_cell(min.x)..=lookup_cell(max.x) {
                for z in lookup_cell(min.z)..=lookup_cell(max.z) {
                    result.spatial.entry((x, z)).or_default().push(index);
                }
            }
        }
        result
    }

    pub fn polygon_count(&self) -> usize {
        self.polygons.len()
    }

    pub(super) fn center(&self, polygon: usize) -> Position {
        self.centers[polygon]
    }

    #[cfg(test)]
    pub(crate) fn candidate(&self, index: usize) -> Option<Position> {
        self.centers.get(index % self.centers.len().max(1)).copied()
    }

    pub fn locate(&self, position: Position, max_distance: f32) -> Option<SurfaceLocation> {
        if !Vec3::from(position).is_finite() || !max_distance.is_finite() || max_distance < 0.0 {
            return None;
        }
        let mut candidates = Vec::new();
        if max_distance > LOOKUP_CELL_SIZE * 4.0 {
            candidates.extend(0..self.polygon_count());
        } else {
            for x in lookup_cell(position.x - max_distance)..=lookup_cell(position.x + max_distance) {
                for z in lookup_cell(position.z - max_distance)..=lookup_cell(position.z + max_distance) {
                    if let Some(indices) = self.spatial.get(&(x, z)) {
                        candidates.extend_from_slice(indices);
                    }
                }
            }
            candidates.sort_unstable();
            candidates.dedup();
        }
        candidates
            .into_iter()
            .filter_map(|polygon| {
                let point = self.project(polygon, position.into())?;
                let distance = point.distance_squared(position.into());
                (distance <= max_distance * max_distance).then_some((
                    distance,
                    SurfaceLocation {
                        carrier: self.carrier,
                        polygon,
                        position: point.into(),
                    },
                ))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.polygon.cmp(&b.1.polygon)))
            .map(|(_, location)| location)
    }

    pub(super) fn project(&self, polygon: usize, point: Vec3) -> Option<Vec3> {
        let boundary = closest_on_polygon(self.polygons.get(polygon)?, point)?;
        let detail = self.detail.meshes.get(polygon)?;
        let vertices = &self.detail.vertices[detail.base_vertex_index as usize..][..detail.vertex_count as usize];
        self.detail.triangles[detail.base_triangle_index as usize..][..detail.triangle_count as usize]
            .iter()
            .filter_map(|triangle| {
                let vertices = triangle.map(|index| vertices[index as usize]);
                closest_on_polygon(&vertices, boundary)
            })
            .min_by(|a, b| {
                a.xz()
                    .distance_squared(boundary.xz())
                    .total_cmp(&b.xz().distance_squared(boundary.xz()))
            })
    }
}

fn lookup_cell(coordinate: f32) -> i32 {
    (coordinate / LOOKUP_CELL_SIZE).floor() as i32
}

pub(super) fn closest_on_polygon(vertices: &[Vec3], point: Vec3) -> Option<Vec3> {
    let &origin = vertices.first()?;
    for pair in vertices[1..].windows(2) {
        let [b, c] = [pair[0], pair[1]];
        let v = b.xz() - origin.xz();
        let w = c.xz() - origin.xz();
        let offset = point.xz() - origin.xz();
        let area = v.perp_dot(w);
        if area.abs() < 1e-6 {
            continue;
        }
        let u = offset.perp_dot(w) / area;
        let t = v.perp_dot(offset) / area;
        if u >= -1e-5 && t >= -1e-5 && u + t <= 1.00001 {
            return Some(origin + (b - origin) * u + (c - origin) * t);
        }
    }
    vertices
        .iter()
        .zip(vertices.iter().cycle().skip(1))
        .take(vertices.len())
        .map(|(&a, &b)| {
            let edge = b.xz() - a.xz();
            let t = ((point.xz() - a.xz()).dot(edge) / edge.length_squared().max(1e-8)).clamp(0.0, 1.0);
            a.lerp(b, t)
        })
        .min_by(|a, b| a.distance_squared(point).total_cmp(&b.distance_squared(point)))
}

#[cfg(test)]
#[path = "tests/mesh.rs"]
mod tests;
