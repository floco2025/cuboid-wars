use std::f32::consts::TAU;

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use rand::{Rng, RngExt, SeedableRng, rngs::SmallRng};

use crate::constants::{EXPLOSION_SCORCH_RESOLUTION, EXPLOSION_SCORCH_SURFACE_OFFSET};
use common::{
    constants::{LEVEL_HEIGHT, WALL_HEIGHT},
    physics::WorldSurfaceHit,
    protocol::MapLayout,
};

const RING_RADII: [f32; 3] = [0.22, 0.39, 0.5];
const RING_ALPHA: [f32; 3] = [0.84, 0.60, 0.0];
const OUTLINE_CONTROL_POINTS: usize = 24;
const DETAIL_CONTROL_POINTS: usize = 17;
const WALL_SEAM_OVERSCAN_FACTOR: f32 = 0.35;

#[derive(Clone, Copy)]
pub(super) struct ScorchStyle {
    pub(super) mesh_index: usize,
    rotation: f32,
}

impl ScorchStyle {
    pub(super) fn random(mesh_count: usize, rng: &mut impl Rng) -> Self {
        Self {
            mesh_index: rng.random_range(0..mesh_count),
            rotation: rng.random_range(0.0..TAU),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct ScorchPlacement {
    pub(super) transform: Transform,
    normal: Vec3,
}

impl ScorchPlacement {
    pub(super) fn on_surface(surface: WorldSurfaceHit, diameter: f32, style: ScorchStyle) -> Self {
        let alignment = Quat::from_rotation_arc(Vec3::Y, surface.normal);
        let random_rotation = Quat::from_axis_angle(surface.normal, style.rotation);
        Self {
            transform: Transform {
                translation: surface.point + surface.normal * EXPLOSION_SCORCH_SURFACE_OFFSET,
                rotation: random_rotation * alignment,
                scale: Vec3::splat(diameter),
            },
            normal: surface.normal,
        }
    }
}

pub(super) fn wall_scorch_placements(
    map_layout: &MapLayout,
    center: Vec3,
    scorch_radius: f32,
    reach_factor: f32,
    style: ScorchStyle,
) -> Vec<ScorchPlacement> {
    let mut placements = Vec::<ScorchPlacement>::new();
    for wall in &map_layout.walls {
        let start = Vec3::new(wall.x1, 0.0, wall.z1);
        let end = Vec3::new(wall.x2, 0.0, wall.z2);
        let segment = end - start;
        let length = segment.length();
        if length <= f32::EPSILON {
            continue;
        }
        let wall_direction = segment / length;
        let progress = ((Vec3::new(center.x, 0.0, center.z) - start).dot(wall_direction) / length).clamp(0.0, 1.0);
        let closest = start + segment * progress;
        let side = Vec3::new(-wall_direction.z, 0.0, wall_direction.x);
        let signed_side_distance = (center - closest).dot(side);
        let normals: &[Vec3] = if signed_side_distance.abs() <= wall.width * 0.5 {
            &[side, -side]
        } else if signed_side_distance > 0.0 {
            &[side]
        } else {
            &[-side]
        };

        let bottom = f32::from(wall.level) * LEVEL_HEIGHT;
        let top = bottom + WALL_HEIGHT;
        for normal in normals {
            let point = Vec3::new(
                closest.x + normal.x * wall.width * 0.5,
                center.y.clamp(bottom, top),
                closest.z + normal.z * wall.width * 0.5,
            );
            let distance = center.distance(point);
            let Some(diameter) = wall_scorch_diameter(scorch_radius, distance, reach_factor) else {
                continue;
            };
            let tangent = normal.cross(Vec3::Y).normalize_or_zero();
            if tangent == Vec3::ZERO {
                continue;
            }
            let half = diameter * 0.5;
            let start_t = (start - point).dot(tangent);
            let end_t = (end - point).dot(tangent);
            let min_t = start_t.min(end_t).max(-half);
            let max_t = start_t.max(end_t).min(half);
            let clipped_min_y = (bottom - point.y).max(-half);
            let max_y = (top - point.y).min(half);
            if max_t - min_t <= 0.02 || max_y - clipped_min_y <= 0.02 {
                continue;
            }
            let visible_height = max_y - clipped_min_y;
            let min_y = (clipped_min_y - visible_height * WALL_SEAM_OVERSCAN_FACTOR).max(-half);
            let translation = point
                + tangent * f32::midpoint(min_t, max_t)
                + Vec3::Y * f32::midpoint(min_y, max_y)
                + *normal * EXPLOSION_SCORCH_SURFACE_OFFSET;
            let basis = Mat3::from_cols(tangent, *normal, Vec3::Y);
            let half_turn = if style.rotation >= std::f32::consts::PI {
                Quat::from_axis_angle(*normal, std::f32::consts::PI)
            } else {
                Quat::IDENTITY
            };
            let placement = ScorchPlacement {
                transform: Transform {
                    translation,
                    rotation: half_turn * Quat::from_mat3(&basis),
                    scale: Vec3::new(max_t - min_t, 1.0, max_y - min_y),
                },
                normal: *normal,
            };
            insert_largest_distinct(&mut placements, placement);
        }
    }
    placements
}

pub(super) fn wall_scorch_diameter(scorch_radius: f32, wall_distance: f32, reach_factor: f32) -> Option<f32> {
    if wall_distance > scorch_radius * reach_factor {
        return None;
    }
    surface_cross_section_diameter(scorch_radius, wall_distance)
}

pub(super) fn surface_cross_section_diameter(radius: f32, surface_distance: f32) -> Option<f32> {
    if surface_distance >= radius {
        return None;
    }
    Some(2.0 * radius.mul_add(radius, -surface_distance * surface_distance).sqrt())
}

fn insert_largest_distinct(placements: &mut Vec<ScorchPlacement>, candidate: ScorchPlacement) {
    if let Some(existing) = placements.iter_mut().find(|existing| {
        existing.normal.dot(candidate.normal) > 0.99
            && existing
                .transform
                .translation
                .distance_squared(candidate.transform.translation)
                < 0.04
    }) {
        let existing_area = existing.transform.scale.x * existing.transform.scale.z;
        let candidate_area = candidate.transform.scale.x * candidate.transform.scale.z;
        if candidate_area > existing_area {
            *existing = candidate;
        }
        return;
    }
    placements.push(candidate);
}

pub(super) fn scorch_mesh(seed: u64) -> Mesh {
    let mut rng = SmallRng::seed_from_u64(seed.wrapping_add(0x5C0C_4A11));
    let outline: Vec<f32> = (0..OUTLINE_CONTROL_POINTS)
        .map(|_| rng.random_range(0.72..1.0))
        .collect();
    let ring_detail: Vec<Vec<f32>> = (0..RING_RADII.len())
        .map(|_| {
            (0..DETAIL_CONTROL_POINTS)
                .map(|_| rng.random_range(-0.04..0.04))
                .collect()
        })
        .collect();
    let alpha_detail: Vec<Vec<f32>> = (0..RING_RADII.len() - 1)
        .map(|_| {
            (0..DETAIL_CONTROL_POINTS)
                .map(|_| rng.random_range(-0.10..0.10))
                .collect()
        })
        .collect();

    let mut positions = Vec::with_capacity(1 + RING_RADII.len() * EXPLOSION_SCORCH_RESOLUTION);
    let mut normals = Vec::with_capacity(positions.capacity());
    let mut colors = Vec::with_capacity(positions.capacity());
    let mut indices = Vec::with_capacity(EXPLOSION_SCORCH_RESOLUTION * (3 + 6 * (RING_RADII.len() - 1)));

    positions.push([0.0, 0.0, 0.0]);
    normals.push([0.0, 1.0, 0.0]);
    colors.push(scorch_color(0.88, 0.0));

    for (ring_index, (&radius, &base_alpha)) in RING_RADII.iter().zip(&RING_ALPHA).enumerate() {
        for segment in 0..EXPLOSION_SCORCH_RESOLUTION {
            let progress = segment as f32 / EXPLOSION_SCORCH_RESOLUTION as f32;
            let angle = progress * TAU;
            let noise =
                smooth_cyclic_sample(&outline, progress) + smooth_cyclic_sample(&ring_detail[ring_index], progress);
            let max_noise = if ring_index + 1 == RING_RADII.len() { 1.0 } else { 1.04 };
            let ring_radius = radius * noise.clamp(0.55, max_noise);
            let alpha_noise = if ring_index + 1 == RING_RADII.len() {
                0.0
            } else {
                smooth_cyclic_sample(&alpha_detail[ring_index], progress)
            };
            positions.push([ring_radius * angle.cos(), 0.0, ring_radius * angle.sin()]);
            normals.push([0.0, 1.0, 0.0]);
            colors.push(scorch_color(
                (base_alpha + alpha_noise).clamp(0.0, 1.0),
                ring_index as f32,
            ));
        }
    }

    for segment in 0..EXPLOSION_SCORCH_RESOLUTION {
        let current = 1 + segment as u32;
        let next = 1 + ((segment + 1) % EXPLOSION_SCORCH_RESOLUTION) as u32;
        indices.extend([0, next, current]);
    }
    for ring_index in 0..RING_RADII.len() - 1 {
        let inner_start = 1 + ring_index * EXPLOSION_SCORCH_RESOLUTION;
        let outer_start = inner_start + EXPLOSION_SCORCH_RESOLUTION;
        for segment in 0..EXPLOSION_SCORCH_RESOLUTION {
            let next = (segment + 1) % EXPLOSION_SCORCH_RESOLUTION;
            let inner = (inner_start + segment) as u32;
            let inner_next = (inner_start + next) as u32;
            let outer = (outer_start + segment) as u32;
            let outer_next = (outer_start + next) as u32;
            indices.extend([inner, outer_next, outer, inner, inner_next, outer_next]);
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn smooth_cyclic_sample(samples: &[f32], progress: f32) -> f32 {
    let sample_position = progress * samples.len() as f32;
    let current = sample_position.floor() as usize % samples.len();
    let next = (current + 1) % samples.len();
    let fraction = sample_position.fract();
    let smooth_fraction = fraction * fraction * (3.0 - 2.0 * fraction);
    samples[current] + (samples[next] - samples[current]) * smooth_fraction
}

fn scorch_color(alpha: f32, ring: f32) -> [f32; 4] {
    Color::srgba(0.035 + ring * 0.004, 0.022 + ring * 0.002, 0.012, alpha)
        .to_linear()
        .to_f32_array()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wall_cross_section_stops_at_reach_limit() {
        assert!(wall_scorch_diameter(2.0, 1.21, 0.6).is_none());
        assert!(wall_scorch_diameter(2.0, 1.20, 0.6).is_some());
    }

    #[test]
    fn surface_cross_section_shrinks_with_distance() {
        assert_eq!(surface_cross_section_diameter(2.0, 0.0), Some(4.0));
        assert_eq!(surface_cross_section_diameter(2.0, 2.0), None);
        let diameter = surface_cross_section_diameter(2.0, 1.0).expect("surface intersects scorch volume");
        assert!((diameter - 2.0 * 3.0_f32.sqrt()).abs() < 0.001);
    }
}
