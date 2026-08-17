use std::f32::consts::TAU;

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use rand::{Rng, RngExt, SeedableRng, rngs::SmallRng};

use crate::constants::{
    EXPLOSION_SCORCH_RING_ALPHA, EXPLOSION_SCORCH_RING_RADII, EXPLOSION_SCORCH_SURFACE_OFFSET,
    EXPLOSION_SCORCH_WALL_SEAM_OVERSCAN_FACTOR,
};
use common::{
    constants::{LEVEL_HEIGHT, WALL_HEIGHT},
    physics::WorldSurfaceHit,
    protocol::MapLayout,
};

use bevy::light::NotShadowCaster;

use super::assets::ExplosionAssets;
use super::particles::ExplosionVfxBudget;
use crate::config::ClientSettings;
use crate::constants::*;
use crate::map::GrassBurn;

const SCORCH_RESOLUTION: usize = 128;
const OUTLINE_CONTROL_POINTS: usize = 24;
const DETAIL_CONTROL_POINTS: usize = 17;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ScorchOutline {
    radii: [f32; OUTLINE_CONTROL_POINTS],
}

impl ScorchOutline {
    fn random(rng: &mut impl Rng) -> Self {
        Self {
            radii: std::array::from_fn(|_| rng.random_range(0.72..1.0)),
        }
    }

    pub(crate) fn for_mesh(mesh_index: usize) -> Self {
        let seed = u64::try_from(mesh_index).expect("scorch mesh index exceeds u64");
        let mut rng = SmallRng::seed_from_u64(seed.wrapping_add(0x5C0C_4A11));
        Self::random(&mut rng)
    }

    pub(crate) fn radius_factor(self, local_angle: f32) -> f32 {
        smooth_cyclic_sample(&self.radii, local_angle.rem_euclid(TAU) / TAU)
    }
}

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

    pub(super) fn rotation(self) -> f32 {
        self.rotation
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

    pub(super) fn normal(self) -> Vec3 {
        self.normal
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
            let min_y = (clipped_min_y - visible_height * EXPLOSION_SCORCH_WALL_SEAM_OVERSCAN_FACTOR).max(-half);
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
    let outline = ScorchOutline::random(&mut rng);
    let ring_detail: Vec<Vec<f32>> = (0..EXPLOSION_SCORCH_RING_RADII.len())
        .map(|_| {
            (0..DETAIL_CONTROL_POINTS)
                .map(|_| rng.random_range(-0.04..0.04))
                .collect()
        })
        .collect();
    let alpha_detail: Vec<Vec<f32>> = (0..EXPLOSION_SCORCH_RING_RADII.len() - 1)
        .map(|_| {
            (0..DETAIL_CONTROL_POINTS)
                .map(|_| rng.random_range(-0.10..0.10))
                .collect()
        })
        .collect();

    let mut positions = Vec::with_capacity(1 + EXPLOSION_SCORCH_RING_RADII.len() * SCORCH_RESOLUTION);
    let mut normals = Vec::with_capacity(positions.capacity());
    let mut colors = Vec::with_capacity(positions.capacity());
    let mut indices = Vec::with_capacity(SCORCH_RESOLUTION * (3 + 6 * (EXPLOSION_SCORCH_RING_RADII.len() - 1)));

    positions.push([0.0, 0.0, 0.0]);
    normals.push([0.0, 1.0, 0.0]);
    colors.push(scorch_color(0.88, 0.0));

    for (ring_index, (&radius, &base_alpha)) in EXPLOSION_SCORCH_RING_RADII
        .iter()
        .zip(&EXPLOSION_SCORCH_RING_ALPHA)
        .enumerate()
    {
        for segment in 0..SCORCH_RESOLUTION {
            let progress = segment as f32 / SCORCH_RESOLUTION as f32;
            let angle = progress * TAU;
            let noise = smooth_cyclic_sample(&outline.radii, progress)
                + smooth_cyclic_sample(&ring_detail[ring_index], progress);
            let max_noise = if ring_index + 1 == EXPLOSION_SCORCH_RING_RADII.len() {
                1.0
            } else {
                1.04
            };
            let ring_radius = radius * noise.clamp(0.55, max_noise);
            let alpha_noise = if ring_index + 1 == EXPLOSION_SCORCH_RING_RADII.len() {
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

    for segment in 0..SCORCH_RESOLUTION {
        let current = 1 + segment as u32;
        let next = 1 + ((segment + 1) % SCORCH_RESOLUTION) as u32;
        indices.extend([0, next, current]);
    }
    for ring_index in 0..EXPLOSION_SCORCH_RING_RADII.len() - 1 {
        let inner_start = 1 + ring_index * SCORCH_RESOLUTION;
        let outer_start = inner_start + SCORCH_RESOLUTION;
        for segment in 0..SCORCH_RESOLUTION {
            let next = (segment + 1) % SCORCH_RESOLUTION;
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

#[derive(Component)]
pub struct ScorchMark {
    elapsed: f32,
    material: Handle<StandardMaterial>,
}

pub(super) fn spawn_scorch_mark(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    budget: &mut ExplosionVfxBudget,
    explosion_assets: &ExplosionAssets,
    placement: ScorchPlacement,
    style: ScorchStyle,
    max_active_marks: usize,
) {
    let material = materials.add(explosion_assets.scorch_template.clone());
    let scorch_mesh = explosion_assets.scorch_meshes[style.mesh_index].clone();
    let grass_burn = (placement.normal().dot(Vec3::Y) > 0.999).then(|| {
        GrassBurn::new(
            placement.transform.translation - placement.normal() * EXPLOSION_SCORCH_SURFACE_OFFSET,
            placement.transform.scale.x * 0.5,
            style.rotation(),
            style.mesh_index,
        )
    });
    let entity = {
        let mut entity_commands = commands.spawn((
            Mesh3d(scorch_mesh),
            MeshMaterial3d(material.clone()),
            NotShadowCaster,
            placement.transform,
            ScorchMark { elapsed: 0.0, material },
        ));
        if let Some(grass_burn) = grass_burn {
            entity_commands.insert(grass_burn);
        }
        entity_commands.id()
    };
    budget.register_scorch(commands, entity, max_active_marks);
}

fn scorch_fade_duration(full_opacity_duration: f32) -> f32 {
    full_opacity_duration * EXPLOSION_SCORCH_FADE_FRACTION
}

fn scorch_alpha(elapsed: f32, full_opacity_duration: f32) -> f32 {
    let fade_duration = scorch_fade_duration(full_opacity_duration);
    ((full_opacity_duration + fade_duration - elapsed) / fade_duration).clamp(0.0, 1.0)
}

fn grass_burn_intensity(scorch_alpha: f32) -> f32 {
    let steps = EXPLOSION_GRASS_BURN_FADE_STEPS as f32;
    (scorch_alpha.clamp(0.0, 1.0) * steps).floor() / steps
}

pub fn scorch_marks_system(
    mut commands: Commands,
    time: Res<Time>,
    _settings: Res<ClientSettings>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut budget: ResMut<ExplosionVfxBudget>,
    mut marks: Query<(Entity, &mut ScorchMark, Option<&mut GrassBurn>)>,
) {
    let delta = time.delta_secs();
    let full_opacity_duration = EXPLOSION_SCORCH_FULL_OPACITY_SECS;
    let fade_duration = scorch_fade_duration(full_opacity_duration);
    let total_duration = full_opacity_duration + fade_duration;
    for (entity, mut mark, grass_burn) in &mut marks {
        mark.elapsed += delta;
        if mark.elapsed >= total_duration {
            budget.remove_scorch(entity);
            commands.entity(entity).despawn();
            continue;
        }
        if mark.elapsed < full_opacity_duration {
            continue;
        }

        let alpha = scorch_alpha(mark.elapsed, full_opacity_duration);
        if let Some(mut material) = materials.get_mut(&mark.material) {
            material.base_color.set_alpha(alpha);
        }
        if let Some(mut grass_burn) = grass_burn {
            grass_burn.set_intensity(grass_burn_intensity(alpha));
        }
    }
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

    #[test]
    fn scorch_alpha_stays_opaque_then_fades_to_zero() {
        let full_opacity_duration = EXPLOSION_SCORCH_FULL_OPACITY_SECS;
        let fade_duration = scorch_fade_duration(full_opacity_duration);
        assert_eq!(scorch_alpha(0.0, full_opacity_duration), 1.0);
        assert_eq!(scorch_alpha(full_opacity_duration, full_opacity_duration), 1.0);
        assert_eq!(
            scorch_alpha(full_opacity_duration + fade_duration / 2.0, full_opacity_duration),
            0.5
        );
        assert_eq!(
            scorch_alpha(full_opacity_duration + fade_duration, full_opacity_duration),
            0.0
        );
    }

    #[test]
    fn grass_burn_intensity_tracks_scorch_fade_in_bounded_steps() {
        let step = 1.0 / EXPLOSION_GRASS_BURN_FADE_STEPS as f32;
        assert_eq!(grass_burn_intensity(1.0), 1.0);
        assert_eq!(grass_burn_intensity(0.5), 0.5);
        assert_eq!(grass_burn_intensity(step * 0.9), 0.0);
        assert_eq!(grass_burn_intensity(-1.0), 0.0);
        assert_eq!(grass_burn_intensity(2.0), 1.0);
    }
}
