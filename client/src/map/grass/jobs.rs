use bevy::{
    light::NotShadowCaster,
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
};

use super::{
    material::GrassMaterials,
    streaming::{GrassChunkVisual, grass_chunk_mesh, padded_grass_bounds},
};

const MAX_BUILDS: usize = 8;
const STARTS_PER_FRAME: usize = 2;
const INSTALLS_PER_FRAME: usize = 2;

#[derive(Component, Default)]
pub struct GrassChunkBuild {
    revision: u64,
    task: Option<Task<Option<Mesh>>>,
    ready: Option<Option<Mesh>>,
}

pub fn grass_chunk_build_system(mut builds: Query<(&GrassChunkVisual, &mut GrassChunkBuild)>) {
    let active = builds
        .iter()
        .filter(|(_, job)| job.task.is_some() || job.ready.is_some())
        .count();
    let mut starts = STARTS_PER_FRAME.min(MAX_BUILDS.saturating_sub(active));
    for (visual, mut job) in &mut builds {
        if starts == 0 {
            break;
        }
        if job.task.is_some() || job.ready.is_some() {
            continue;
        }
        let input = visual.clone();
        job.revision = visual.revision;
        job.task = Some(AsyncComputeTaskPool::get().spawn(async move { grass_chunk_mesh(&input, &input.burns) }));
        starts -= 1;
    }
}

pub fn grass_chunk_finish_system(
    mut commands: Commands,
    materials: Res<GrassMaterials>,
    mut builds: Query<(Entity, &GrassChunkVisual, &mut GrassChunkBuild, Option<&Mesh3d>)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let mut installs = INSTALLS_PER_FRAME;
    for (entity, visual, mut job, handle) in &mut builds {
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
        if installs == 0 {
            continue;
        }
        installs -= 1;
        let mesh = job.ready.take().expect("ready grass result");
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
