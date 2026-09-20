use std::{collections::BTreeMap, sync::Arc};

use anyhow::{Context, Result, anyhow};
use bevy::prelude::*;
use common::{
    config::CharacterPhysicsConfig,
    physics::{CollisionMesh, CollisionWorld},
    protocol::{Carrier, CarrierId, FieldId, Ladder, MapLayout, SwitchId, SwitchState},
};

use super::{
    RouteFailure, SurfaceBounds, SurfaceMesh, SurfaceRoute,
    baking::{BakeRequest, BakeStatus, BakeWorker},
    transfers::{self, DockLink},
};
use crate::{
    actors::SurfaceGoal,
    config::ServerGameplayConfig,
    map::{CarrierGrid, MapConfig},
    quests::QuestBoard,
};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct MeshKey {
    pub(super) carrier: u16,
    pub(super) diameter: u32,
    pub(super) height: u32,
    pub(super) window: Option<(i32, i32)>,
}

impl MeshKey {
    pub(super) fn new(carrier: CarrierId, physics: CharacterPhysicsConfig) -> Self {
        Self {
            carrier: carrier.0,
            diameter: physics.movement_collider.diameter.to_bits(),
            height: physics.movement_collider.height.to_bits(),
            window: None,
        }
    }
}

pub(super) struct SurfaceRegion {
    pub(super) bounds: SurfaceBounds,
    pub(super) excluded: Vec<SurfaceBounds>,
}

pub(super) struct BakedSurface {
    pub(super) physics: CharacterPhysicsConfig,
    pub(super) region: SurfaceRegion,
    // A changed field leaves this mesh in service until its replacement
    // arrives: a rebake takes seconds, and the motor's support and collision
    // checks cover what the old mesh no longer describes.
    pub(super) mesh: Option<SurfaceMesh>,
    // The field state `mesh` was baked for; `None` once the geometry changed
    // under it. Returning to this state needs no bake.
    pub(super) baked: Option<Vec<FieldId>>,
    pub(super) revision: u64,
    pub(super) open: Vec<FieldId>,
    pub(super) dirty: bool,
    pub(super) last_used: u64,
}

#[derive(Resource, Default)]
pub(crate) struct SurfaceNavigation {
    pub(super) geometry: Arc<Vec<CollisionMesh>>,
    pub(super) meshes: BTreeMap<MeshKey, BakedSurface>,
    locked_plates: Vec<SwitchId>,
    pub(super) open_fields: Vec<FieldId>,
    ladders: Arc<Vec<Ladder>>,
    worker: Option<BakeWorker>,
    pub(super) running: Option<(MeshKey, u64)>,
    motions: Vec<Carrier>,
    connections: BTreeMap<(u32, u32), Vec<DockLink>>,
    revision: u64,
    pub(super) clock: u64,
    pub(super) collision_bounds: BTreeMap<CarrierId, SurfaceBounds>,
}

impl SurfaceNavigation {
    pub(crate) fn build(
        map: &MapConfig,
        layout: &MapLayout,
        config: &ServerGameplayConfig,
        world: &CollisionWorld,
        open: &[FieldId],
        locked: &[SwitchId],
    ) -> Result<Self> {
        let profiles: Vec<_> = map
            .actor_spawn_zones
            .iter()
            .filter_map(|zone| {
                let actor = config.expect_actor(&zone.kind);
                (!actor.character.immovable && !actor.character.flies()).then_some(actor.character.physics())
            })
            .collect();
        if profiles.is_empty() {
            return Ok(Self::default());
        }
        let geometry = world
            .collision_meshes()
            .context("exporting surface navigation geometry")?;
        let mut collision_bounds = BTreeMap::<CarrierId, SurfaceBounds>::new();
        for mesh in &geometry {
            for &point in &mesh.vertices {
                let bounds = collision_bounds
                    .entry(mesh.carrier)
                    .or_insert(SurfaceBounds { min: point, max: point });
                bounds.min = bounds.min.min(point);
                bounds.max = bounds.max.max(point);
            }
        }
        let mut meshes = BTreeMap::new();
        for grid in &map.grids {
            let roam = map
                .actor_spawn_zones
                .iter()
                .filter(|zone| zone.carrier == grid.carrier)
                .map(|zone| zone.roam_distance)
                .fold(0.0, f32::max);
            for &physics in &profiles {
                let key = MeshKey::new(grid.carrier, physics);
                if meshes.contains_key(&key) {
                    continue;
                }
                let mut region = region(grid, roam, physics);
                if let Some(bounds) = collision_bounds.get(&grid.carrier) {
                    region.bounds.min.y = region.bounds.min.y.min(bounds.min.y - 1.0);
                    region.bounds.max.y = region.bounds.max.y.max(bounds.max.y + physics.movement_collider.height);
                }
                let fields = fields_in(&geometry, grid.carrier, open, region.bounds);
                let mut mesh = SurfaceMesh::bake_in(
                    &geometry,
                    grid.carrier,
                    physics,
                    &fields,
                    Some(region.bounds),
                    &region.excluded,
                )
                .with_context(|| {
                    format!(
                        "baking navigation on carrier {} for diameter {} height {}, over its grid extended by its \
                         largest roam_distance ({roam} m)",
                        key.carrier, physics.movement_collider.diameter, physics.movement_collider.height
                    )
                })?;
                mesh.add_ladders(&layout.ladders, physics);
                meshes.insert(
                    key,
                    BakedSurface {
                        physics,
                        region,
                        mesh: Some(mesh),
                        baked: Some(fields.clone()),
                        revision: 1,
                        open: fields,
                        dirty: false,
                        last_used: 0,
                    },
                );
            }
        }
        let mut navigation = Self {
            geometry: Arc::new(geometry),
            meshes,
            locked_plates: locked.to_vec(),
            open_fields: open.to_vec(),
            ladders: Arc::new(layout.ladders.clone()),
            worker: None,
            running: None,
            motions: layout.carriers.clone(),
            connections: BTreeMap::new(),
            revision: 0,
            clock: 0,
            collision_bounds,
        };
        navigation.refresh_connections();
        Ok(navigation)
    }

    pub(crate) fn mesh(&self, carrier: CarrierId, physics: CharacterPhysicsConfig) -> Option<(&SurfaceMesh, u64)> {
        let entry = self.meshes.get(&MeshKey::new(carrier, physics))?;
        Some((entry.mesh.as_ref()?, entry.revision))
    }

    pub(crate) fn rebuilding(&self) -> bool {
        self.running.is_some() || self.meshes.values().any(|entry| entry.dirty)
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    // Docks join base meshes only, so a streamed window leaves them alone.
    fn refresh_connections(&mut self) {
        let profiles: BTreeMap<_, _> = self
            .meshes
            .iter()
            .map(|(key, entry)| ((key.diameter, key.height), entry.physics))
            .collect();
        self.connections = profiles
            .into_iter()
            .map(|(key, physics)| (key, transfers::dock_links(self, physics, &self.motions)))
            .collect();
    }

    pub(crate) fn can_route(
        &self,
        from: SurfaceGoal,
        to: SurfaceGoal,
        physics: CharacterPhysicsConfig,
        ladders: bool,
    ) -> bool {
        if self.walking_mesh(from, to, physics, ladders).is_some() {
            return true;
        }
        let key = MeshKey::new(from.carrier, physics);
        let links = self
            .connections
            .get(&(key.diameter, key.height))
            .map_or(&[][..], Vec::as_slice);
        transfers::reachable(self, links, from, to, physics, ladders)
    }

    pub(crate) fn route(
        &self,
        from: SurfaceGoal,
        to: SurfaceGoal,
        physics: CharacterPhysicsConfig,
        ladders: bool,
        limit: usize,
    ) -> Result<SurfaceRoute, RouteFailure> {
        if let Some((mesh, start, goal)) = self.walking_mesh(from, to, physics, ladders) {
            return mesh.route_for(start.position, goal.position, 0.1, limit, ladders);
        }
        let key = MeshKey::new(from.carrier, physics);
        let links = self
            .connections
            .get(&(key.diameter, key.height))
            .map_or(&[][..], Vec::as_slice);
        transfers::route(self, links, from, to, physics, ladders, limit).map_err(|error| {
            if self.pending_region(from, to, physics) {
                RouteFailure::NavigationUnavailable
            } else {
                error
            }
        })
    }

    pub(crate) fn refresh(&mut self, world: &CollisionWorld, open: &[FieldId], locked: &[SwitchId]) -> Result<()> {
        if self.meshes.is_empty() {
            return Ok(());
        }
        self.clock += 1;
        let fields_changed = self.open_fields != open;
        if fields_changed {
            self.open_fields = open.to_vec();
        }
        let geometry_changed = self.locked_plates != locked;
        if geometry_changed {
            self.geometry = Arc::new(world.collision_meshes()?);
            self.locked_plates = locked.to_vec();
        }
        if fields_changed || geometry_changed {
            for (key, entry) in &mut self.meshes {
                let fields = fields_in(&self.geometry, CarrierId(key.carrier), open, entry.region.bounds);
                if geometry_changed {
                    entry.baked = None;
                } else if entry.open == fields {
                    continue;
                }
                entry.open = fields;
                entry.revision += 1;
                entry.dirty = entry.baked.as_ref() != Some(&entry.open);
            }
        }
        if let Some((key, revision)) = self.running {
            let finished = match self.worker.as_ref().map(BakeWorker::poll) {
                Some(BakeStatus::Running) => None,
                Some(BakeStatus::Finished(result)) => Some(*result),
                Some(BakeStatus::Stopped) | None => {
                    self.worker = None;
                    Some(Err(anyhow!("navigation bake worker stopped")))
                }
            };
            if let Some(result) = finished {
                self.running = None;
                // A bake superseded while it ran describes a state that no longer holds.
                if let Some(entry) = self.meshes.get_mut(&key).filter(|entry| entry.revision == revision) {
                    match result {
                        Ok(mesh) => {
                            entry.mesh = Some(mesh);
                            entry.baked = Some(entry.open.clone());
                        }
                        Err(error) => {
                            entry.mesh = None;
                            entry.baked = None;
                            error!(
                                "surface navigation rebuild failed on carrier {}: {error:#}",
                                key.carrier
                            );
                        }
                    }
                    if key.window.is_none() {
                        self.refresh_connections();
                    }
                    self.revision += 1;
                }
            }
        }
        if self.running.is_none()
            && let Some((key, entry)) = self
                .meshes
                .iter_mut()
                .filter(|(_, entry)| entry.dirty)
                .min_by_key(|(key, entry)| (key.window.is_some(), entry.last_used))
        {
            let worker = match self.worker.take() {
                Some(worker) => worker,
                None => BakeWorker::new()?,
            };
            worker.start(BakeRequest {
                geometry: Arc::clone(&self.geometry),
                ladders: Arc::clone(&self.ladders),
                carrier: CarrierId(key.carrier),
                physics: entry.physics,
                bounds: entry.region.bounds,
                excluded: entry.region.excluded.clone(),
                open: entry.open.clone(),
            })?;
            self.worker = Some(worker);
            entry.dirty = false;
            self.running = Some((*key, entry.revision));
        }
        Ok(())
    }
}

pub(super) fn fields_in(
    geometry: &[CollisionMesh],
    carrier: CarrierId,
    open: &[FieldId],
    bounds: SurfaceBounds,
) -> Vec<FieldId> {
    let mut fields: Vec<_> = geometry
        .iter()
        .filter(|mesh| mesh.carrier == carrier && mesh.field.is_some_and(|field| open.contains(&field)))
        .filter(|mesh| {
            let min = mesh
                .vertices
                .iter()
                .copied()
                .fold(Vec3::splat(f32::INFINITY), Vec3::min);
            let max = mesh
                .vertices
                .iter()
                .copied()
                .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
            min.cmple(bounds.max).all() && max.cmpge(bounds.min).all()
        })
        .filter_map(|mesh| mesh.field)
        .collect();
    fields.sort_by_key(|field| field.0);
    fields.dedup();
    fields
}

fn region(grid: &CarrierGrid, roam: f32, physics: CharacterPhysicsConfig) -> SurfaceRegion {
    let geometry = grid.geometry;
    let margin = roam + physics.movement_collider.diameter;
    let bounds = SurfaceBounds {
        min: Vec3::new(
            -geometry.width() / 2.0 - margin,
            -geometry.level_height(),
            -geometry.depth() / 2.0 - margin,
        ),
        max: Vec3::new(
            geometry.width() / 2.0 + margin,
            grid.levels.len() as f32 * geometry.level_height() + physics.movement_collider.height,
            geometry.depth() / 2.0 + margin,
        ),
    };
    let mut excluded = Vec::new();
    for (level, tier) in grid.levels.iter().enumerate() {
        for (row, cells) in tier.cells.rows.iter().enumerate() {
            for (col, cell) in cells.iter().enumerate() {
                if !cell.has_floor_slab || cell.has_floor {
                    continue;
                }
                let x = geometry.cell_to_world_x(col as i32);
                let z = geometry.cell_to_world_z(row as i32);
                let y = level as f32 * geometry.level_height();
                excluded.push(SurfaceBounds {
                    min: Vec3::new(x, y - 0.2, z),
                    max: Vec3::new(x + geometry.cell_size(), y + 0.2, z + geometry.cell_size()),
                });
            }
        }
    }
    SurfaceRegion { bounds, excluded }
}

pub(crate) fn surface_navigation_sync_system(
    mut navigation: ResMut<SurfaceNavigation>,
    world: Res<CollisionWorld>,
    switches: Res<SwitchState>,
    quests: Res<QuestBoard>,
) {
    if let Err(error) = navigation.refresh(&world, &switches.open_fields, quests.locked_switches()) {
        for entry in navigation.meshes.values_mut() {
            entry.mesh = None;
            entry.baked = None;
            entry.revision += 1;
        }
        error!("surface navigation refresh failed: {error:#}");
    }
}

#[cfg(test)]
#[path = "tests/world.rs"]
mod tests;
