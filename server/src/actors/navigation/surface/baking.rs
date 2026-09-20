use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

use anyhow::{Context, Result, anyhow};
use common::{
    config::CharacterPhysicsConfig,
    physics::CollisionMesh,
    protocol::{CarrierId, FieldId, Ladder},
};
use crossbeam_channel::{Receiver, Sender, TryRecvError, bounded};

use super::{SurfaceBounds, SurfaceMesh};

pub(super) struct BakeRequest {
    pub geometry: Arc<Vec<CollisionMesh>>,
    pub ladders: Arc<Vec<Ladder>>,
    pub carrier: CarrierId,
    pub physics: CharacterPhysicsConfig,
    pub bounds: SurfaceBounds,
    pub excluded: Vec<SurfaceBounds>,
    pub open: Vec<FieldId>,
}

pub(super) struct BakeWorker {
    input: Sender<BakeRequest>,
    output: Receiver<Result<SurfaceMesh>>,
}

impl BakeWorker {
    pub fn new() -> Result<Self> {
        let (input, jobs) = bounded::<BakeRequest>(1);
        let (results, output) = bounded(1);
        std::thread::Builder::new()
            .name("surface-baking".into())
            .spawn(move || {
                while let Ok(job) = jobs.recv() {
                    // A panic on degenerate geometry is one failed bake; the
                    // worker has to outlive it to serve the next request.
                    let result = catch_unwind(AssertUnwindSafe(|| bake(&job)))
                        .unwrap_or_else(|_| Err(anyhow!("navigation bake panicked")));
                    if results.send(result).is_err() {
                        break;
                    }
                }
            })
            .context("starting navigation bake worker")?;
        Ok(Self { input, output })
    }

    pub fn start(&self, request: BakeRequest) -> Result<()> {
        self.input.try_send(request).context("scheduling navigation bake")
    }

    pub fn poll(&self) -> BakeStatus {
        match self.output.try_recv() {
            Ok(mesh) => BakeStatus::Finished(Box::new(mesh)),
            Err(TryRecvError::Empty) => BakeStatus::Running,
            Err(TryRecvError::Disconnected) => BakeStatus::Stopped,
        }
    }
}

pub(super) enum BakeStatus {
    Running,
    Finished(Box<Result<SurfaceMesh>>),
    Stopped,
}

fn bake(job: &BakeRequest) -> Result<SurfaceMesh> {
    let mut mesh = SurfaceMesh::bake_in(
        &job.geometry,
        job.carrier,
        job.physics,
        &job.open,
        Some(job.bounds),
        &job.excluded,
    )?;
    mesh.add_ladders(&job.ladders, job.physics);
    Ok(mesh)
}
