use bevy::{light::NotShadowCaster, prelude::*};

use super::{
    super::{assets::ExplosionAssets, particles::ExplosionVfxBudget},
    placement::ScorchPlacement,
    variants::ScorchStyle,
};
use crate::{
    carriers::CarrierEntities,
    config::ClientSettings,
    constants::{EXPLOSION_SCORCH_FADE_FRACTION, EXPLOSION_SCORCH_FULL_OPACITY_SECS},
    map::GrassBurn,
};

pub(crate) const SCORCH_SURFACE_OFFSET: f32 = 0.015;
const GRASS_BURN_FADE_STEPS: u32 = 60;

#[derive(Component)]
pub struct ScorchMark {
    elapsed: f32,
    material: Handle<StandardMaterial>,
}

pub(crate) fn spawn_scorch_mark(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    budget: &mut ExplosionVfxBudget,
    explosion_assets: &ExplosionAssets,
    carrier_entities: &CarrierEntities,
    placement: ScorchPlacement,
    style: ScorchStyle,
    max_active_marks: usize,
) {
    let variant = placement
        .region
        .apply(&explosion_assets.scorch_variants[style.mesh_index]);
    if variant.triangles.is_empty() {
        return;
    }
    let material = materials.add(explosion_assets.scorch_template.clone());
    let grass_burn = placement.grass_burn(style);
    let entity = {
        let mut entity_commands = commands.spawn((
            Mesh3d(meshes.add(variant.mesh())),
            MeshMaterial3d(material.clone()),
            NotShadowCaster,
            placement.transform,
            ChildOf(carrier_entities.get(placement.carrier)),
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
    let steps = GRASS_BURN_FADE_STEPS as f32;
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
#[path = "tests/marks.rs"]
mod tests;
