use bevy_ecs::prelude::Resource;
use bevy_math::{Quat, Vec3};
use rapier3d::{
    control::{CharacterCollision, EffectiveCharacterMovement, KinematicCharacterController},
    parry::{
        query::{ShapeCastOptions, contact, intersection_test},
        shape::{Ball, Cuboid},
    },
    prelude::{
        BroadPhaseBvh, Collider, ColliderHandle, ColliderSet, Group, IntegrationParameters, NarrowPhase, Pose, Ray,
        RigidBodySet, Shape, Vector,
    },
};

use crate::{
    config::{CharacterPhysicsConfig, PortalShotSettings},
    constants::{CHARACTER_CONTACT_OFFSET, PHYSICS_EPSILON, PORTAL_BACKING_FLUSH_EPSILON},
    map::{CarrierPose, Carriers},
    physics::characters::{character_center, character_shape},
    protocol::{BarrierKindId, BarrierKindTable, BridgeKindId, CarrierId, MapLayout, Position},
};

use super::colliders::{
    BRIDGE_COLLISION_GROUP, ColliderKind, FLOOR_COLLISION_GROUP, WALL_COLLISION_GROUP, barrier_collision_group,
    character_collision_groups, collider_interaction_groups, ground_collision_groups, insert_barrier_collider,
    insert_bridge_collider, insert_floor_collider, insert_ramp_collider, insert_wall_collider, query_filter,
    surface_collision_groups, world_collision_groups,
};

use super::ladders::LadderVolume;
pub use super::shape_cast::ShapeCastHit;
use super::shape_cast::upward_surface_hit;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldSurfaceHit {
    pub point: Vec3,
    pub normal: Vec3,
    // Whose surface it is: what a portal placed here rides.
    pub carrier: CarrierId,
}

#[derive(Resource)]
pub struct CollisionWorld {
    bodies: RigidBodySet,
    pub(super) colliders: ColliderSet,
    broad_phase: BroadPhaseBvh,
    narrow_phase: NarrowPhase,
    // Cached union of every configured barrier kind's collision group.
    // Recomputed at construction (depends on `BarrierKindTable.len()`); used
    // by character filters and barrier-only shape casts so we don't loop
    // the table per query.
    all_barrier_groups: Group,
    // Every light bridge collider with its kind, for `set_powered_bridges`.
    bridge_colliders: Vec<(BridgeKindId, ColliderHandle)>,
    // Each carrier's colliders with their carrier-local poses, in layout
    // order, and the same handles flat, for `set_carrier_poses`.
    carrier_colliders: Vec<Vec<(ColliderHandle, Pose)>>,
    carried_handles: Vec<ColliderHandle>,
    // Ladder volumes as built from the local records, and the same posed
    // into world space by `set_carrier_poses`, which is what queries read.
    ladder_locals: Vec<(CarrierId, LadderVolume)>,
    ladder_volumes: Vec<LadderVolume>,
}

impl CollisionWorld {
    #[must_use]
    pub fn from_map_layout(map_layout: &MapLayout, kind_table: &BarrierKindTable) -> Self {
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

        for wall in &map_layout.walls {
            let handle = insert_wall_collider(&mut colliders, wall);
            collider_handles.push(carried(&colliders, handle, wall.carrier));
        }

        for floor in &map_layout.floors {
            let handle = insert_floor_collider(&mut colliders, floor);
            collider_handles.push(carried(&colliders, handle, floor.carrier));
        }

        for ramp in &map_layout.ramps {
            if let Some(handle) = insert_ramp_collider(&mut colliders, ramp) {
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
            bridge_colliders.push((bridge.kind, handle));
        }
        let carried_handles = carrier_colliders
            .iter()
            .flat_map(|handles| handles.iter().map(|(handle, _)| *handle))
            .collect();

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

        let mut all_barrier_groups = Group::empty();
        for idx in 0..kind_table.len() {
            all_barrier_groups |= barrier_collision_group(BarrierKindId(idx as u16));
        }

        let ladder_locals: Vec<_> = map_layout
            .ladders
            .iter()
            .map(|ladder| (ladder.carrier, LadderVolume::from_ladder(ladder)))
            .collect();
        let ladder_volumes = ladder_locals.iter().map(|(_, volume)| *volume).collect();

        let mut world = Self {
            bodies,
            colliders,
            broad_phase,
            narrow_phase,
            all_barrier_groups,
            bridge_colliders,
            carrier_colliders,
            carried_handles,
            ladder_locals,
            ladder_volumes,
        };
        world.set_carrier_poses(&Carriers::from_layout(map_layout));
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
        for (index, handles) in self.carrier_colliders.iter().enumerate() {
            let pose = rapier_pose(&carriers.pose(CarrierId(index as u16 + 1)));
            for (handle, local) in handles {
                self.colliders[*handle].set_position(pose * *local);
            }
        }
        let mut events = Vec::new();
        self.broad_phase.update(
            &IntegrationParameters::default(),
            &self.colliders,
            &self.bodies,
            &self.carried_handles,
            &[],
            &mut events,
        );
        for (index, (carrier, local)) in self.ladder_locals.iter().enumerate() {
            self.ladder_volumes[index] = local.posed(&carriers.pose(*carrier));
        }
    }

    #[must_use]
    pub(crate) fn carrier_of(&self, handle: ColliderHandle) -> CarrierId {
        ColliderKind::carrier_from_user_data(self.colliders[handle].user_data)
    }

    // Bridge power is world state, not per-query state: the powered kinds'
    // colliders join `BRIDGE_COLLISION_GROUP` and the rest leave every group,
    // so each surface query sees the current bridges without carrying the
    // powered set. Both sides apply `PlateState` here whenever it changes
    // (`powered_bridges_sync_system`).
    pub fn set_powered_bridges(&mut self, powered: &[BridgeKindId]) {
        for (kind, handle) in &self.bridge_colliders {
            let membership = if powered.contains(kind) {
                BRIDGE_COLLISION_GROUP
            } else {
                Group::empty()
            };
            self.colliders[*handle].set_collision_groups(collider_interaction_groups(membership));
        }
    }

    #[must_use]
    pub fn ladder_volume_at(&self, pos: &Position) -> Option<&LadderVolume> {
        self.ladder_volumes.iter().find(|volume| volume.contains(pos))
    }

    pub(crate) fn carried_ladder_at_previous_pose(
        &self,
        pos: &Position,
        carriers: &Carriers,
    ) -> Option<(CarrierId, LadderVolume)> {
        self.ladder_locals.iter().find_map(|(carrier, local)| {
            if carrier.is_world() {
                return None;
            }
            // The body has not received this tick's carry yet.
            let previous = local.posed(&carriers.previous_pose(*carrier));
            previous.contains(pos).then_some((*carrier, previous))
        })
    }

    #[must_use]
    pub fn ladder_band_at(&self, x: f32, z: f32, y: f32) -> Option<&LadderVolume> {
        self.ladder_volumes.iter().find(|volume| volume.band_contains(x, z, y))
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

    #[must_use]
    pub(crate) fn move_character(
        &self,
        dt: f32,
        controller: &KinematicCharacterController,
        character_shape: &dyn Shape,
        character_pos: &Pose,
        desired_translation: Vector,
        passable_kinds: &[BarrierKindId],
        excluded_colliders: &[ColliderHandle],
        events: impl FnMut(CharacterCollision),
    ) -> EffectiveCharacterMovement {
        // Portal transit: the aperture's backing colliders stop existing for
        // a body overlapping the aperture, which is what lets it pass
        // through the surface.
        let allow = |handle: ColliderHandle, _: &rapier3d::prelude::Collider| !excluded_colliders.contains(&handle);
        let mut filter = query_filter(character_collision_groups(passable_kinds, self.all_barrier_groups));
        if !excluded_colliders.is_empty() {
            filter.predicate = Some(&allow);
        }
        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            filter,
        );
        controller.move_shape(
            dt,
            &query_pipeline,
            character_shape,
            character_pos,
            desired_translation,
            events,
        )
    }

    // Cast a moving ball against walls/floors/ramps and the powered light
    // bridges (the "bouncy" world). Barriers terminate projectiles via
    // `cast_moving_ball_against_barriers` instead, so they're filtered out
    // here.
    #[must_use]
    pub fn cast_moving_ball(&self, position: Vec3, translation: Vec3, radius: f32) -> Option<ShapeCastHit> {
        self.cast_moving_ball_with_filter(position, translation, radius, surface_collision_groups())
    }

    #[must_use]
    pub fn projectile_path_clear(
        &self,
        position: Vec3,
        translation: Vec3,
        radius: f32,
        open_kinds: &[BarrierKindId],
    ) -> bool {
        let groups = character_collision_groups(open_kinds, self.all_barrier_groups);
        !self.ball_overlaps_groups(position, radius, groups)
            && self
                .cast_moving_ball_with_filter(position, translation, radius, groups)
                .is_none()
    }

    // Cast a moving ball against barrier colliders only. Used by projectiles
    // to detect termination on a barrier. Kinds in `open_kinds` (currently
    // held open by pressure plates) are dropped from the filter, so shots
    // fly through the gap a plate creates.
    #[must_use]
    pub fn cast_moving_ball_against_barriers(
        &self,
        position: Vec3,
        translation: Vec3,
        radius: f32,
        open_kinds: &[BarrierKindId],
    ) -> Option<ShapeCastHit> {
        let mut groups = self.all_barrier_groups;
        for kind in open_kinds {
            groups.remove(barrier_collision_group(*kind));
        }
        if groups.is_empty() {
            return None;
        }
        self.cast_moving_ball_with_filter(position, translation, radius, groups)
    }

    fn cast_moving_ball_with_filter(
        &self,
        position: Vec3,
        translation: Vec3,
        radius: f32,
        groups: Group,
    ) -> Option<ShapeCastHit> {
        if translation.length_squared() == 0.0 {
            return None;
        }

        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            query_filter(groups),
        );
        let shape = Ball::new(radius);
        let pose = Pose::translation(position.x, position.y, position.z);
        let velocity = Vector::new(translation.x, translation.y, translation.z);
        let options = ShapeCastOptions {
            max_time_of_impact: 1.0,
            stop_at_penetration: false,
            ..ShapeCastOptions::default()
        };

        query_pipeline
            .cast_shape(&pose, velocity, &shape, options)
            .map(|(handle, hit)| {
                let mut normal = Vec3::new(hit.normal2.x, hit.normal2.y, hit.normal2.z);
                if normal.dot(translation) > 0.0 {
                    normal = -normal;
                }
                ShapeCastHit {
                    normal,
                    contact: Vec3::new(hit.witness1.x, hit.witness1.y, hit.witness1.z),
                    t: hit.time_of_impact,
                    barrier_kind: ColliderKind::barrier_kind_from_user_data(self.colliders[handle].user_data),
                    carrier: self.carrier_of(handle),
                }
            })
    }

    // Line of sight is blocked by walls/floors/ramps only. Barriers don't
    // block sight — actors see through and pursue; the kinematic controller
    // stops them at the barrier surface, where normal wall-avoidance kicks in.
    // Light bridges don't either, powered or not: sight, beams, and blasts
    // reach through them, so nothing here has to follow the plate state.
    #[must_use]
    pub fn line_of_sight_clear(&self, from: Vec3, to: Vec3) -> bool {
        const SIGHT_RADIUS: f32 = 0.08;
        let translation = to - from;
        self.cast_moving_ball_with_filter(from, translation, SIGHT_RADIUS, world_collision_groups())
            .is_none()
    }

    #[must_use]
    pub fn ground_surface_below(&self, origin: Vec3, max_distance: f32) -> Option<WorldSurfaceHit> {
        let hit = self.surface_along_ray(origin, Vec3::NEG_Y, max_distance, ground_collision_groups())?;
        (hit.normal.y > 0.1).then_some(hit)
    }

    #[must_use]
    pub fn wall_surface_along_ray(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<WorldSurfaceHit> {
        let hit = self.surface_along_ray(origin, direction, max_distance, WALL_COLLISION_GROUP)?;
        (hit.normal.y.abs() < 0.1).then_some(hit)
    }

    // First world surface (wall/floor/ramp, on any carrier) along the ray —
    // the same filter as `line_of_sight_clear`, so a beam clipped at this
    // point stops exactly where sight does. Barriers and light bridges are excluded.
    #[must_use]
    pub fn world_surface_along_ray(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<WorldSurfaceHit> {
        self.surface_along_ray(origin, direction, max_distance, world_collision_groups())
    }

    // Only global plate state opens a shot path; held keys never enter this filter.
    #[must_use]
    pub fn portal_surface_along_ray(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
        settings: PortalShotSettings,
        open_barriers: &[BarrierKindId],
    ) -> Option<WorldSurfaceHit> {
        let mut groups = world_collision_groups();
        if settings.barriers_block {
            groups |= self.all_barrier_groups;
            for kind in open_barriers {
                groups.remove(barrier_collision_group(*kind));
            }
        }
        if settings.light_bridges_block {
            groups |= BRIDGE_COLLISION_GROUP;
        }
        self.surface_along_ray(origin, direction, max_distance, groups)
    }

    fn surface_along_ray(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
        groups: Group,
    ) -> Option<WorldSurfaceHit> {
        if !origin.is_finite() || !direction.is_finite() || !max_distance.is_finite() || max_distance <= 0.0 {
            return None;
        }
        let direction = direction.try_normalize()?;
        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            query_filter(groups),
        );
        let ray = Ray::new(
            Vector::new(origin.x, origin.y, origin.z),
            Vector::new(direction.x, direction.y, direction.z),
        );
        let (handle, hit) = query_pipeline.cast_ray_and_get_normal(&ray, max_distance, false)?;
        if matches!(
            ColliderKind::from_user_data(self.colliders[handle].user_data),
            Some(ColliderKind::Barrier | ColliderKind::Bridge)
        ) {
            return None;
        }
        let normal = Vec3::new(hit.normal.x, hit.normal.y, hit.normal.z).try_normalize()?;

        Some(WorldSurfaceHit {
            point: origin + direction * hit.time_of_impact,
            normal,
            carrier: self.carrier_of(handle),
        })
    }

    #[must_use]
    pub(crate) fn ground_hit(
        &self,
        character_shape: &dyn Shape,
        character_pos: &Pose,
        max_distance: f32,
        target_distance: f32,
        passable_kinds: &[BarrierKindId],
        excluded_colliders: &[ColliderHandle],
    ) -> Option<ShapeCastHit> {
        let allow = |handle: ColliderHandle, _: &Collider| !excluded_colliders.contains(&handle);
        let predicate =
            (!excluded_colliders.is_empty()).then_some(&allow as &dyn Fn(ColliderHandle, &Collider) -> bool);
        self.ground_hit_filtered(
            character_shape,
            character_pos,
            max_distance,
            target_distance,
            passable_kinds,
            predicate,
        )
    }

    #[must_use]
    pub(crate) fn ground_hit_on_carrier(
        &self,
        character_shape: &dyn Shape,
        character_pos: &Pose,
        max_distance: f32,
        passable_kinds: &[BarrierKindId],
        carrier: CarrierId,
    ) -> Option<ShapeCastHit> {
        let on_carrier = |_: ColliderHandle, collider: &Collider| {
            ColliderKind::carrier_from_user_data(collider.user_data) == carrier
        };
        self.ground_hit_filtered(
            character_shape,
            character_pos,
            max_distance,
            0.0,
            passable_kinds,
            Some(&on_carrier),
        )
    }

    fn ground_hit_filtered(
        &self,
        character_shape: &dyn Shape,
        character_pos: &Pose,
        max_distance: f32,
        target_distance: f32,
        passable_kinds: &[BarrierKindId],
        predicate: Option<&dyn Fn(ColliderHandle, &Collider) -> bool>,
    ) -> Option<ShapeCastHit> {
        let mut filter = query_filter(character_collision_groups(passable_kinds, self.all_barrier_groups));
        filter.predicate = predicate;
        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            filter,
        );
        let options = ShapeCastOptions {
            max_time_of_impact: max_distance,
            target_distance,
            stop_at_penetration: false,
            compute_impact_geometry_on_penetration: true,
        };

        query_pipeline
            .cast_shape(character_pos, Vector::NEG_Y, character_shape, options)
            .and_then(|(handle, hit)| upward_surface_hit(hit, self.carrier_of(handle)))
    }

    #[must_use]
    pub(crate) fn projectile_spawn_overlaps_blocker(
        &self,
        position: Vec3,
        radius: f32,
        open_kinds: &[BarrierKindId],
    ) -> bool {
        // Walls, floors, and powered bridges are always blockers. Barriers
        // block the muzzle unless the kind is currently open (pressure-plate
        // held) — those barriers are gone visually and shots pass through
        // them, so the muzzle clipping them is fine.
        let mut groups =
            WALL_COLLISION_GROUP | FLOOR_COLLISION_GROUP | BRIDGE_COLLISION_GROUP | self.all_barrier_groups;
        for kind in open_kinds {
            groups.remove(barrier_collision_group(*kind));
        }
        self.ball_overlaps_groups(position, radius, groups)
    }

    // Portal backing colliders: what the aperture's backing volume touches
    // that lies entirely behind the surface plane and belongs to the
    // portal's own carrier. An adjoining ramp, or the floor a wall portal
    // stands on, reaches in front of the plane and must remain solid while
    // the surface itself opens for transit; a stacked wall's trim strip is
    // flush with the wall faces and opens with them. Another carrier's
    // collider passing behind the plane is not backing: it moves on, and
    // the set is computed once.
    #[must_use]
    pub(crate) fn portal_backing_colliders(
        &self,
        surface_center: Vec3,
        surface_normal: Vec3,
        half_extents: Vec3,
        rotation: Quat,
        carrier: CarrierId,
    ) -> Vec<ColliderHandle> {
        let Some(surface_normal) = surface_normal.try_normalize() else {
            return Vec::new();
        };
        let plane_reach = surface_center.dot(surface_normal) + PORTAL_BACKING_FLUSH_EPSILON;
        let outward = Vector::new(surface_normal.x, surface_normal.y, surface_normal.z);
        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            query_filter(world_collision_groups()),
        );
        let shape = Cuboid::new(Vector::new(half_extents.x, half_extents.y, half_extents.z));
        let center = surface_center - surface_normal * half_extents.z;
        let axis_angle = rotation.to_scaled_axis();
        let pose = Pose::new(
            Vector::new(center.x, center.y, center.z),
            Vector::new(axis_angle.x, axis_angle.y, axis_angle.z),
        );
        query_pipeline
            .intersect_shape(pose, &shape)
            .filter_map(|(handle, collider)| {
                if ColliderKind::carrier_from_user_data(collider.user_data) != carrier {
                    return None;
                }
                let front = collider
                    .shape()
                    .as_support_map()?
                    .support_point(collider.position(), outward);
                (Vec3::new(front.x, front.y, front.z).dot(surface_normal) <= plane_reach).then_some(handle)
            })
            .collect()
    }

    // Whether the oriented shape touches anything a body could stand on or
    // walk into right now: the static world plus the powered bridges.
    #[must_use]
    pub(crate) fn oriented_shape_overlaps_surface(&self, center: Vec3, rotation: Quat, shape: &dyn Shape) -> bool {
        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            query_filter(surface_collision_groups()),
        );
        let axis_angle = rotation.to_scaled_axis();
        let pose = Pose::new(
            Vector::new(center.x, center.y, center.z),
            Vector::new(axis_angle.x, axis_angle.y, axis_angle.z),
        );
        query_pipeline.intersect_shape(pose, shape).next().is_some()
    }

    #[must_use]
    pub fn cuboid_overlaps_wall(&self, position: Vec3, half_extents: Vec3) -> bool {
        self.cuboid_overlaps_groups(position, half_extents, WALL_COLLISION_GROUP)
    }

    // Whether sliding a character's body box horizontally from `start` to
    // `target` drags it through a wall. Floors and ramps are ignored so a
    // leg onto a slope counts as clear; a body already touching a wall but
    // moving away from it is clear too.
    #[must_use]
    pub fn character_sweep_hits_wall(
        &self,
        start: &Position,
        target: &Position,
        physics: CharacterPhysicsConfig,
    ) -> bool {
        let translation = Vector::new(target.x - start.x, 0.0, target.z - start.z);
        if translation.length_squared() == 0.0 {
            return false;
        }
        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            query_filter(WALL_COLLISION_GROUP),
        );
        let center = character_center(*start, physics);
        let pose = Pose::translation(center.x, center.y, center.z);
        let options = ShapeCastOptions {
            max_time_of_impact: 1.0,
            stop_at_penetration: false,
            ..ShapeCastOptions::default()
        };
        query_pipeline
            .cast_shape(&pose, translation, &character_shape(physics), options)
            .is_some()
    }

    fn ball_overlaps_groups(&self, position: Vec3, radius: f32, groups: Group) -> bool {
        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            query_filter(groups),
        );
        let shape = Ball::new(radius);
        let pose = Pose::translation(position.x, position.y, position.z);

        query_pipeline.intersect_shape(pose, &shape).next().is_some()
    }

    pub(crate) fn push_character_from_carriers(
        &self,
        dt: f32,
        controller: &KinematicCharacterController,
        shape: &dyn Shape,
        start: &Pose,
        carriers: &Carriers,
        passable_kinds: &[BarrierKindId],
        excluded_colliders: &[ColliderHandle],
        mut events: impl FnMut(CharacterCollision),
    ) -> Vector {
        let allow = |handle: ColliderHandle, collider: &Collider| {
            !excluded_colliders.contains(&handle)
                && ColliderKind::from_user_data(collider.user_data) != Some(ColliderKind::Ramp)
                && carriers
                    .displacement(self.carrier_of(handle))
                    .with_y(0.0)
                    .length_squared()
                    > PHYSICS_EPSILON * PHYSICS_EPSILON
        };
        let mut filter = query_filter(character_collision_groups(passable_kinds, self.all_barrier_groups));
        filter.predicate = Some(&allow);
        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            filter,
        );
        let mut pose = *start;
        let mut overlaps: Vec<_> = query_pipeline
            .intersect_shape(pose, shape)
            .map(|(handle, _)| handle)
            .collect();
        overlaps.sort_unstable_by_key(|handle| handle.into_raw_parts());
        for handle in overlaps {
            let collider = &self.colliders[handle];
            let Ok(Some(hit)) = contact(collider.position(), collider.shape(), &pose, shape, 0.0) else {
                continue;
            };
            let normal = hit.normal1.with_y(0.0);
            let travel = carriers.displacement(self.carrier_of(handle));
            if hit.dist >= 0.0
                || hit.normal1.y.abs() > 0.5
                || normal.x * travel.x + normal.z * travel.z <= PHYSICS_EPSILON
            {
                continue;
            }
            // Only the incoming face pushes; tangential travel must not drag a bystander along the wall.
            let push = normal * ((CHARACTER_CONTACT_OFFSET - hit.dist) / normal.length_squared());
            let resolved = self.move_character(
                dt,
                controller,
                shape,
                &pose,
                push,
                passable_kinds,
                excluded_colliders,
                &mut events,
            );
            pose.translation += resolved.translation;
        }
        pose.translation - start.translation
    }

    // Leg-only contact permits boarding raised slabs. A carried collider must
    // penetrate the movement box before or after control movement to count; checking
    // the full height afterwards prevents escaping through a descending slab.
    // Vertical carry also checks static geometry, such as a lift's ceiling.
    pub(crate) fn character_crushed(
        &self,
        start: &Position,
        pos: &Position,
        physics: CharacterPhysicsConfig,
        passable_kinds: &[BarrierKindId],
        excluded_colliders: &[ColliderHandle],
        lifted: bool,
    ) -> bool {
        let collision_box = character_shape(physics);
        let center = character_center(*pos, physics);
        let start_center = character_center(*start, physics);
        let movement_box = inset_contact_shape(&collision_box);
        let movement_poses = [
            Pose::translation(start_center.x, start_center.y, start_center.z),
            Pose::translation(center.x, center.y, center.z),
        ];
        let head = center.y + collision_box.half_extents.y;
        let feet = pos.y.min(center.y - collision_box.half_extents.y);
        let body = Cuboid::new(Vector::new(
            collision_box.half_extents.x,
            (head - feet) / 2.0,
            collision_box.half_extents.z,
        ));
        let body_center = Vec3::new(center.x, (head + feet) / 2.0, center.z);
        let carried = |_: ColliderHandle, collider: &Collider| {
            !ColliderKind::carrier_from_user_data(collider.user_data).is_world()
                && movement_poses.iter().any(|pose| {
                    intersection_test(pose, &movement_box, collider.position(), collider.shape())
                        .is_ok_and(|overlaps| overlaps)
                })
        };
        let any = |_: ColliderHandle, _: &Collider| true;
        self.crushing_overlap(body_center, &body, passable_kinds, excluded_colliders, &carried)
            || (lifted && self.crushing_overlap(center, &collision_box, passable_kinds, excluded_colliders, &any))
    }

    fn crushing_overlap(
        &self,
        center: Vec3,
        shape: &Cuboid,
        passable_kinds: &[BarrierKindId],
        excluded_colliders: &[ColliderHandle],
        counts: &dyn Fn(ColliderHandle, &Collider) -> bool,
    ) -> bool {
        let allow = |handle: ColliderHandle, collider: &Collider| {
            !excluded_colliders.contains(&handle)
                && ColliderKind::from_user_data(collider.user_data) != Some(ColliderKind::Ramp)
                && counts(handle, collider)
        };
        let mut filter = query_filter(character_collision_groups(passable_kinds, self.all_barrier_groups));
        filter.predicate = Some(&allow);
        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            filter,
        );
        let pose = Pose::translation(center.x, center.y, center.z);
        query_pipeline
            .intersect_shape(pose, &inset_contact_shape(shape))
            .next()
            .is_some()
    }

    fn cuboid_overlaps_groups(&self, center: Vec3, half_extents: Vec3, groups: Group) -> bool {
        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            query_filter(groups),
        );
        let shape = Cuboid::new(Vector::new(half_extents.x, half_extents.y, half_extents.z));
        let pose = Pose::translation(center.x, center.y, center.z);

        query_pipeline.intersect_shape(pose, &shape).next().is_some()
    }
}

// Contact margins tolerate resting touches; only penetration counts as crushing.
fn inset_contact_shape(shape: &Cuboid) -> Cuboid {
    let inset = CHARACTER_CONTACT_OFFSET * 2.0;
    Cuboid::new(Vector::new(
        (shape.half_extents.x - inset).max(PHYSICS_EPSILON),
        (shape.half_extents.y - inset).max(PHYSICS_EPSILON),
        (shape.half_extents.z - inset).max(PHYSICS_EPSILON),
    ))
}

fn rapier_pose(pose: &CarrierPose) -> Pose {
    Pose::translation(pose.translation.x, pose.translation.y, pose.translation.z)
}
