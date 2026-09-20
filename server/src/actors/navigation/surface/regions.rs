use super::{
    SurfaceBounds, SurfaceMesh, SurfaceNavigation,
    world::{BakedSurface, MeshKey, SurfaceRegion, fields_in},
};
use crate::actors::SurfaceGoal;
use bevy::prelude::*;
use common::{
    config::CharacterPhysicsConfig,
    physics::CollisionWorld,
    protocol::{CarrierId, FieldId, Position},
};

// Overlapping local windows bound bake cost without imposing a world-space
// leash. Nearby actors share windows; unused windows can be baked again later.
fn window_size(physics: CharacterPhysicsConfig) -> f32 {
    // Small capsules require finer voxels; keep their windows within the
    // same column budget instead of silently making the bake fail.
    128.0_f32.min(SurfaceMesh::voxel_size(physics).0 * 1800.0)
}
const WINDOW_LIMIT: usize = 32;

impl SurfaceBounds {
    fn clearance(self, point: Position) -> f32 {
        (point.x - self.min.x)
            .min(self.max.x - point.x)
            .min(point.z - self.min.z)
            .min(self.max.z - point.z)
    }
}

impl MeshKey {
    fn matches(self, carrier: CarrierId, physics: CharacterPhysicsConfig) -> bool {
        self.carrier == carrier.0
            && self.diameter == physics.movement_collider.diameter.to_bits()
            && self.height == physics.movement_collider.height.to_bits()
    }
}

impl SurfaceNavigation {
    pub(crate) fn mesh_at(&self, from: SurfaceGoal, physics: CharacterPhysicsConfig) -> Option<&SurfaceMesh> {
        self.meshes
            .iter()
            .filter(|(key, entry)| {
                key.matches(from.carrier, physics) && entry.region.bounds.clearance(from.position) >= 0.0
            })
            .filter_map(|(_, entry)| {
                let mesh = entry.mesh.as_ref()?;
                mesh.locate(from.position, 1.0)?;
                Some((entry.region.bounds.clearance(from.position), mesh))
            })
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, mesh)| mesh)
    }

    pub(super) fn walking_mesh(
        &self,
        from: SurfaceGoal,
        to: SurfaceGoal,
        physics: CharacterPhysicsConfig,
        ladders: bool,
    ) -> Option<&SurfaceMesh> {
        if from.carrier != to.carrier {
            return None;
        }
        self.meshes
            .iter()
            .filter(|(key, _)| key.matches(from.carrier, physics))
            .find_map(|(_, entry)| {
                let mesh = entry.mesh.as_ref()?;
                let start = mesh.locate(from.position, 1.0)?;
                let goal = mesh.locate(to.position, 0.7)?;
                mesh.connected(start, goal, ladders).then_some(mesh)
            })
    }

    pub(super) fn pending_region(&self, from: SurfaceGoal, to: SurfaceGoal, physics: CharacterPhysicsConfig) -> bool {
        from.carrier == to.carrier
            && self.meshes.iter().any(|(key, entry)| {
                key.matches(from.carrier, physics)
                    && (entry.dirty || self.running.is_some_and(|(running, _)| running == *key))
                    && entry.region.bounds.clearance(from.position) >= 0.0
                    && entry.region.bounds.clearance(to.position) >= 0.0
            })
    }

    pub(crate) fn prepare_area(&mut self, from: SurfaceGoal, physics: CharacterPhysicsConfig) {
        self.request_region(from, from, physics, window_size(physics) * 3.0 / 32.0);
    }

    // None means navigation is still being prepared, not that the target is
    // unreachable. Keep pursuing while a window loads instead of fleeing.
    pub(crate) fn prepare_route(
        &mut self,
        from: SurfaceGoal,
        to: SurfaceGoal,
        physics: CharacterPhysicsConfig,
        ladders: bool,
    ) -> Option<bool> {
        if self.can_route(from, to, physics, ladders) {
            self.touch(from, to, physics);
            return Some(true);
        }
        if from.carrier == to.carrier
            && self.request_region(from, to, physics, 2.0_f32.min(window_size(physics) / 16.0))
        {
            return None;
        }
        Some(false)
    }

    fn touch(&mut self, from: SurfaceGoal, to: SurfaceGoal, physics: CharacterPhysicsConfig) {
        for (key, entry) in &mut self.meshes {
            if key.matches(from.carrier, physics)
                && entry.region.bounds.clearance(from.position) >= 0.0
                && entry.region.bounds.clearance(to.position) >= 0.0
            {
                entry.last_used = self.clock;
            }
        }
    }

    fn request_region(
        &mut self,
        from: SurfaceGoal,
        to: SurfaceGoal,
        physics: CharacterPhysicsConfig,
        margin: f32,
    ) -> bool {
        if from.carrier != to.carrier {
            return false;
        }
        let Some(extent) = self.collision_bounds.get(&from.carrier) else {
            return false;
        };
        if extent.clearance(from.position) < 0.0 || extent.clearance(to.position) < 0.0 {
            return false;
        }
        if let Some((key, entry)) = self.meshes.iter_mut().find(|(key, entry)| {
            key.matches(from.carrier, physics)
                && entry.region.bounds.clearance(from.position) >= margin
                && entry.region.bounds.clearance(to.position) >= margin
        }) {
            entry.last_used = self.clock;
            return entry.dirty || self.running.is_some_and(|(running, _)| running == *key);
        }
        let size = window_size(physics);
        let stride = size / 4.0;
        let center = (Vec3::from(from.position) + Vec3::from(to.position)) * 0.5;
        let window = ((center.x / stride).round() as i32, (center.z / stride).round() as i32);
        let mut key = MeshKey::new(from.carrier, physics);
        let Some(base) = self.meshes.get(&key) else {
            return false;
        };
        let center = Vec3::new(window.0 as f32 * stride, 0.0, window.1 as f32 * stride);
        let bounds = SurfaceBounds {
            min: Vec3::new(center.x - size / 2.0, base.region.bounds.min.y, center.z - size / 2.0),
            max: Vec3::new(center.x + size / 2.0, base.region.bounds.max.y, center.z + size / 2.0),
        };
        if bounds.clearance(from.position) < margin || bounds.clearance(to.position) < margin {
            return false;
        }
        let excluded = base.region.excluded.clone();
        let open = fields_in(&self.geometry, from.carrier, &self.open_fields, bounds);
        key.window = Some(window);
        if let Some(entry) = self.meshes.get_mut(&key) {
            entry.last_used = self.clock;
            return entry.dirty || self.running.is_some_and(|(running, _)| running == key);
        }
        if self.meshes.keys().filter(|key| key.window.is_some()).count() >= WINDOW_LIMIT {
            let oldest = self
                .meshes
                .iter()
                .filter(|(key, entry)| {
                    key.window.is_some()
                        && entry.last_used + 2 < self.clock
                        && self.running.is_none_or(|(running, _)| running != **key)
                })
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| *key);
            let Some(oldest) = oldest else {
                return true;
            };
            self.meshes.remove(&oldest);
        }
        self.meshes.insert(
            key,
            BakedSurface {
                physics,
                region: SurfaceRegion { bounds, excluded },
                mesh: None,
                revision: 1,
                open,
                dirty: true,
                last_used: self.clock,
            },
        );
        true
    }
}

impl SurfaceNavigation {
    pub(crate) fn approach_goal(
        &self,
        from: SurfaceGoal,
        to: SurfaceGoal,
        physics: CharacterPhysicsConfig,
        ladders: bool,
        world: &CollisionWorld,
        carriers: &common::map::Carriers,
        open: &[FieldId],
    ) -> SurfaceGoal {
        let authored_coverage = self.mesh(from.carrier, physics).is_some_and(|(mesh, _)| {
            mesh.locate(from.position, 1.0).is_some() && mesh.locate(to.position, 0.7).is_some()
        });
        if from.carrier != to.carrier || (authored_coverage && self.can_route(from, to, physics, ladders)) {
            return to;
        }
        // Keep streamed journeys local even when a loaded window happens to
        // contain both endpoints. Searching from one far edge to the other
        // can exhaust the budget on detailed terrain and stall a return home.
        let offset = Vec3::from(to.position) - Vec3::from(from.position);
        let distance = offset.with_y(0.0).length();
        let leg = window_size(physics) * 3.0 / 8.0;
        if distance <= leg {
            return to;
        }
        let desired = Vec3::from(from.position) + offset * (leg / distance);
        let local = self
            .meshes
            .iter()
            .filter(|(key, _)| key.matches(from.carrier, physics))
            .filter_map(|(_, entry)| {
                let mesh = entry.mesh.as_ref()?;
                let start = mesh.locate(from.position, 1.0)?;
                let next = mesh.locate(desired.into(), window_size(physics) / 8.0)?;
                (mesh.connected(start, next, ladders)
                    && next.position.horizontal_distance_sq(&to.position) + 1.0
                        < from.position.horizontal_distance_sq(&to.position))
                .then_some(next.position)
            })
            .min_by(|a, b| {
                a.distance_sq(&desired.into())
                    .total_cmp(&b.distance_sq(&desired.into()))
            });
        if let Some(position) = local {
            return SurfaceGoal {
                carrier: from.carrier,
                position,
            };
        }
        let Some(bounds) = self.collision_bounds.get(&from.carrier) else {
            return to;
        };
        let pose = carriers.pose(from.carrier);
        let probe = desired.with_y(bounds.max.y + 1.0);
        world
            .support_surface_on_carrier(
                pose.transform_point(probe),
                bounds.max.y - bounds.min.y + 2.0,
                from.carrier,
                open,
            )
            .map_or(to, |surface| SurfaceGoal {
                carrier: from.carrier,
                position: pose.inverse_transform_point(surface.point).into(),
            })
    }
}
