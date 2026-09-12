use bevy_ecs::prelude::Resource;
use bevy_math::{Quat, Vec3};
use rapier3d::prelude::{
    BroadPhaseBvh, ColliderHandle, ColliderSet, Group, IntegrationParameters, NarrowPhase, Pose, QueryFilter,
    QueryPipeline, RigidBodySet, Shape,
};

use super::{
    bounds::WorldBounds,
    colliders::{
        BRIDGE_COLLISION_GROUP, ColliderKind, collider_interaction_groups, insert_barrier_collider,
        insert_bridge_collider, insert_floor_collider, insert_ramp_collider, insert_wall_collider, query_filter,
        surface_collision_groups,
    },
    erasers::EraserVolume,
    ladders::LadderVolume,
    surface_materials::MATERIAL_INDEX_SHIFT,
};
use crate::{
    map::Carriers,
    math::{rapier_pose, to_rapier},
    protocol::{Barrier, BarrierId, BarrierKindId, BridgeId, CarrierId, MapLayout},
};

#[derive(Resource)]
pub struct CollisionWorld {
    pub(super) bodies: RigidBodySet,
    pub(super) colliders: ColliderSet,
    pub(super) broad_phase: BroadPhaseBvh,
    pub(super) narrow_phase: NarrowPhase,
    pub(crate) barriers: Vec<Barrier>,
    // Every light bridge collider with its kind, for `set_powered_bridges`.
    bridge_colliders: Vec<(BridgeId, ColliderHandle)>,
    // Each carrier's colliders with their carrier-local poses, in layout
    // order, and the same handles flat, for `set_carrier_poses`.
    carrier_colliders: Vec<Vec<(ColliderHandle, Pose)>>,
    pub(super) bounds: WorldBounds,
    // Ladder and eraser volumes as built from the local records, and the
    // same posed into world space by `set_carrier_poses`, which is what
    // queries read.
    pub(super) ladder_locals: Vec<LadderVolume>,
    pub(super) ladder_volumes: Vec<LadderVolume>,
    eraser_locals: Vec<EraserVolume>,
    pub(super) eraser_volumes: Vec<EraserVolume>,
}

impl CollisionWorld {
    pub fn passable_barriers(&self, held_keys: &[BarrierKindId], open: &[BarrierId]) -> Vec<BarrierId> {
        crate::physics::passable_barriers(held_keys, open, &self.barriers)
    }

    #[must_use]
    pub fn from_map_layout(map_layout: &MapLayout) -> Self {
        let bodies = RigidBodySet::new();
        let mut colliders = ColliderSet::new();
        let mut collider_handles = Vec::new();
        let mut carrier_colliders: Vec<Vec<(ColliderHandle, Pose)>> = vec![Vec::new(); map_layout.carriers.len()];
        // Every collider is inserted at its carrier-local pose; the carried
        // ones are placed by `set_carrier_poses` below and every tick after.
        let mut carried = |colliders: &ColliderSet, handle: ColliderHandle, carrier: CarrierId| {
            if let Some(index) = carrier.carried_index() {
                carrier_colliders
                    .get_mut(index)
                    .expect("layout record names a carrier the layout does not have")
                    .push((handle, *colliders[handle].position()));
            }
            handle
        };

        for (index, wall) in map_layout.walls.iter().enumerate() {
            let handle = insert_wall_collider(&mut colliders, wall);
            colliders[handle].user_data |= (index as u128) << MATERIAL_INDEX_SHIFT;
            collider_handles.push(carried(&colliders, handle, wall.carrier));
        }

        for (index, floor) in map_layout.floors.iter().enumerate() {
            let handle = insert_floor_collider(&mut colliders, floor);
            colliders[handle].user_data |= (index as u128) << MATERIAL_INDEX_SHIFT;
            collider_handles.push(carried(&colliders, handle, floor.carrier));
        }

        for (index, ramp) in map_layout.ramps.iter().enumerate() {
            if let Some(handle) = insert_ramp_collider(&mut colliders, ramp) {
                colliders[handle].user_data |= (index as u128) << MATERIAL_INDEX_SHIFT;
                collider_handles.push(carried(&colliders, handle, ramp.carrier));
            }
        }

        for barrier in &map_layout.barriers {
            let handle = insert_barrier_collider(&mut colliders, barrier);
            collider_handles.push(carried(&colliders, handle, barrier.carrier));
        }

        let mut bridge_colliders = Vec::with_capacity(map_layout.light_bridges.len());
        for bridge in &map_layout.light_bridges {
            let handle = insert_bridge_collider(&mut colliders, bridge);
            collider_handles.push(carried(&colliders, handle, bridge.carrier));
            bridge_colliders.push((bridge.id, handle));
        }
        let bounds = WorldBounds::new(&colliders, map_layout.carriers.len());

        let mut broad_phase = BroadPhaseBvh::new();
        let narrow_phase = NarrowPhase::new();
        let mut events = Vec::new();
        broad_phase.update(
            &IntegrationParameters::default(),
            &colliders,
            &bodies,
            &collider_handles,
            &[],
            &mut events,
        );

        let ladder_locals: Vec<_> = map_layout.ladders.iter().map(LadderVolume::from_ladder).collect();
        let ladder_volumes = ladder_locals.clone();
        let eraser_locals: Vec<_> = map_layout.erasers.iter().map(EraserVolume::from_eraser).collect();
        let eraser_volumes = eraser_locals.clone();

        let mut world = Self {
            bodies,
            colliders,
            broad_phase,
            narrow_phase,
            barriers: map_layout.barriers.clone(),
            bridge_colliders,
            carrier_colliders,
            bounds,
            ladder_locals,
            ladder_volumes,
            eraser_locals,
            eraser_volumes,
        };
        world.set_carrier_poses(&Carriers::from_layout(map_layout));
        // Initial placement is not a sweep through the intervening space.
        world.bounds.finish_placement();
        world
    }

    // Carriers are the one geometry that moves. Each tick both sides put
    // every carried collider at its carrier's current pose
    // (`carriers_advance_system`) and refresh the broad phase for exactly
    // those handles, so every query that follows sees them where they are;
    // nothing else here ever moves, so the rest of the tree stays as built.
    pub fn set_carrier_poses(&mut self, carriers: &Carriers) {
        assert_eq!(
            carriers.carried_count(),
            self.carrier_colliders.len(),
            "carrier count differs between the runtime state and the collision world"
        );
        if self.carrier_colliders.is_empty() {
            return;
        }
        let mut changed_handles = Vec::new();
        for (carrier, handles) in carriers.carried_ids().zip(&self.carrier_colliders) {
            if !self.bounds.set_pose(carrier, carriers.pose(carrier).translation) {
                continue;
            }
            let pose = Pose::from_translation(to_rapier(carriers.pose(carrier).translation));
            for (handle, local) in handles {
                self.colliders[*handle].set_position(pose * *local);
                changed_handles.push(*handle);
            }
        }
        self.bounds.refresh();
        let mut events = Vec::new();
        self.broad_phase.update(
            &IntegrationParameters::default(),
            &self.colliders,
            &self.bodies,
            &changed_handles,
            &[],
            &mut events,
        );
        for (posed, local) in self.ladder_volumes.iter_mut().zip(&self.ladder_locals) {
            *posed = local.posed(&carriers.pose(local.carrier()));
        }
        for (posed, local) in self.eraser_volumes.iter_mut().zip(&self.eraser_locals) {
            *posed = local.posed(&carriers.pose(local.carrier()));
        }
    }

    pub(super) fn query_pipeline<'a>(&'a self, filter: QueryFilter<'a>) -> QueryPipeline<'a> {
        self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            filter,
        )
    }

    // Whether the shape at `pose` touches any collider the filter admits.
    pub(super) fn shape_overlaps(&self, pose: Pose, shape: &dyn Shape, filter: QueryFilter<'_>) -> bool {
        self.query_pipeline(filter)
            .intersect_shape(pose, shape)
            .next()
            .is_some()
    }

    #[must_use]
    pub(crate) fn carrier_of(&self, handle: ColliderHandle) -> CarrierId {
        ColliderKind::carrier_from_user_data(self.colliders[handle].user_data)
    }

    // Bridge power is world state, not per-query state: the powered bridges'
    // colliders join `BRIDGE_COLLISION_GROUP` and the rest leave every group,
    // so each surface query sees the current bridges without carrying the
    // powered set. Both sides apply `PlateState` here whenever it changes
    // (`powered_bridges_sync_system`).
    pub fn set_powered_bridges(&mut self, powered: &[BridgeId]) {
        for (kind, handle) in &self.bridge_colliders {
            let membership = if powered.contains(kind) {
                BRIDGE_COLLISION_GROUP
            } else {
                Group::empty()
            };
            if self.colliders[*handle].collision_groups().memberships != membership {
                self.bounds.changed(self.carrier_of(*handle));
                self.colliders[*handle].set_collision_groups(collider_interaction_groups(membership));
            }
        }
    }

    #[cfg(test)]
    #[must_use]
    pub(super) fn solid_count(&self) -> usize {
        self.colliders.len()
    }

    #[cfg(test)]
    #[must_use]
    pub(super) fn solid_kinds(&self) -> Vec<ColliderKind> {
        self.colliders
            .iter()
            .filter_map(|(_, collider)| ColliderKind::from_user_data(collider.user_data))
            .collect()
    }

    // Whether the oriented shape touches anything a body could stand on or
    // walk into right now: the static world plus the powered bridges.
    #[must_use]
    pub(crate) fn oriented_shape_overlaps_surface(&self, center: Vec3, rotation: Quat, shape: &dyn Shape) -> bool {
        self.shape_overlaps(
            rapier_pose(center, rotation),
            shape,
            query_filter(surface_collision_groups()),
        )
    }
}
