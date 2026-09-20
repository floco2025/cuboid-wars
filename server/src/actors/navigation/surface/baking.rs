use std::sync::Arc;

use anyhow::{Context, Result};
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
                    let result = SurfaceMesh::bake_in(
                        &job.geometry,
                        job.carrier,
                        job.physics,
                        &job.open,
                        Some(job.bounds),
                        &job.excluded,
                    )
                    .map(|mut mesh| {
                        mesh.add_ladders(&job.ladders, job.physics);
                        mesh
                    });
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

    pub fn poll(&self) -> Result<Option<Result<SurfaceMesh>>> {
        match self.output.try_recv() {
            Ok(mesh) => Ok(Some(mesh)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => anyhow::bail!("navigation bake worker stopped"),
        }
    }
}
