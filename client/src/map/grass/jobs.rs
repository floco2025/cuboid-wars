use std::cmp::Ordering;

use bevy::{
    ecs::system::SystemParam,
    light::NotShadowCaster,
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
};
use common::{map::Carriers, protocol::CarrierId};

use super::{
    material::GrassMaterials,
    mesh::GrassLod,
    streaming::{GrassChunkVisual, grass_chunk_mesh, padded_grass_bounds},
};
use crate::{
    cameras::MainCameraMarker,
    constants::{GRASS_BUILD_INSTALLS_PER_FRAME, GRASS_BUILD_MAX_ACTIVE},
};

#[derive(Component, Default)]
pub struct GrassChunkBuild {
    revision: u64,
    task: Option<Task<Option<Mesh>>>,
    ready: Option<Option<Mesh>>,
}

// The viewer's eye and the carrier poses, to serve the grass underfoot first:
// near chunks before mid ones, then by distance, as streaming spawned them.
#[derive(SystemParam)]
pub struct GrassBuildOrder<'w, 's> {
    camera: Query<'w, 's, &'static GlobalTransform, With<MainCameraMarker>>,
    carriers: Option<Res<'w, Carriers>>,
}

impl GrassBuildOrder<'_, '_> {
    fn rank(&self, visual: &GrassChunkVisual) -> (bool, f32) {
        let eye = self.camera.single().map_or(Vec3::ZERO, GlobalTransform::translation);
        let carrier = visual.patches.first().map_or(CarrierId::WORLD, |patch| patch.carrier);
        let origin = self.carriers.as_ref().map_or(visual.origin, |carriers| {
            carriers.pose(carrier).transform_point(visual.origin)
        });
        (visual.lod != GrassLod::Near, origin.xz().distance_squared(eye.xz()))
    }
}

fn nearest_first(a: &(bool, f32), b: &(bool, f32)) -> Ordering {
    a.0.cmp(&b.0).then(a.1.total_cmp(&b.1))
}

pub fn grass_chunk_build_system(order: GrassBuildOrder, mut builds: Query<(&GrassChunkVisual, &mut GrassChunkBuild)>) {
    let active = builds
        .iter()
        .filter(|(_, job)| job.task.is_some() || job.ready.is_some())
        .count();
    let free = GRASS_BUILD_MAX_ACTIVE.saturating_sub(active);
    if free == 0 {
        return;
    }
    let mut waiting: Vec<_> = builds
        .iter_mut()
        .filter(|(_, job)| job.task.is_none() && job.ready.is_none())
        .map(|(visual, job)| (order.rank(visual), visual, job))
        .collect();
    waiting.sort_by(|a, b| nearest_first(&a.0, &b.0));
    for (_, visual, mut job) in waiting.into_iter().take(free) {
        let input = visual.clone();
        job.revision = visual.revision;
        job.task = Some(AsyncComputeTaskPool::get().spawn(async move { grass_chunk_mesh(&input, &input.burns) }));
    }
}

pub fn grass_chunk_finish_system(
    mut commands: Commands,
    order: GrassBuildOrder,
    materials: Res<GrassMaterials>,
    mut builds: Query<(Entity, &GrassChunkVisual, &mut GrassChunkBuild, Option<&Mesh3d>)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let mut finished = Vec::new();
    for (entity, visual, mut job, _) in &mut builds {
        if let Some(task) = &mut job.task
            && let Some(mesh) = block_on(poll_once(task))
        {
            job.ready = Some(mesh);
            job.task = None;
        }
        if job.ready.is_none() {
            continue;
        }
        if job.revision != visual.revision {
            // Keep one queued job with the latest input instead of installing
            // an old burn state or accumulating one task per fade frame.
            job.ready = None;
            continue;
        }
        finished.push((order.rank(visual), entity));
    }
    finished.sort_by(|a, b| nearest_first(&a.0, &b.0));
    for (_, entity) in finished.into_iter().take(GRASS_BUILD_INSTALLS_PER_FRAME) {
        let Ok((_, visual, mut job, handle)) = builds.get_mut(entity) else {
            continue;
        };
        let Some(mesh) = job.ready.take() else {
            continue;
        };
        let mut chunk = commands.entity(entity);
        chunk.remove::<GrassChunkBuild>();
        if let Some(mesh) = mesh {
            let bounds = padded_grass_bounds(&mesh);
            let handle = if let Some(handle) = handle {
                meshes
                    .insert(handle.id(), mesh)
                    .expect("grass mesh handle is no longer valid");
                handle.clone()
            } else {
                Mesh3d(meshes.add(mesh))
            };
            chunk.insert((
                handle,
                MeshMaterial3d(materials.grass.clone()),
                NotShadowCaster,
                bounds,
                visual.lod.visibility_range(),
            ));
        } else {
            chunk.remove::<Mesh3d>();
        }
    }
}

#[cfg(test)]
#[path = "tests/jobs.rs"]
mod tests;
