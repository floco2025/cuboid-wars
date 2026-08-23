use std::collections::{HashMap, VecDeque};

use bevy::prelude::*;

use common::protocol::{HomingTarget, MissileId, PlayerId, Position};

use crate::watchdog::ProgressWatchdog;

// Server-side flight velocity. The wire form is `MissileMovementState`
// (yaw/pitch/speed); this keeps the integrated Vec3 between ticks.
#[derive(Component, Debug, Clone, Copy)]
pub struct MissileVelocity(pub Vec3);

// Per-missile guidance state, kept outside the ECS like `ActorInfo`.
pub struct MissileInfo {
    pub entity: Entity,
    pub shooter: PlayerId,
    // `None` once the target dies/despawns — the missile flies straight on.
    pub target: Option<HomingTarget>,
    // Air-graph waypoints toward the target while sight is blocked, plus
    // the target position the path was computed for.
    pub path: VecDeque<Vec3>,
    pub path_target: Option<Vec3>,
    pub path_retry_timer: f32,
    // Committed local-dodge direction, used only when the air graph has no
    // route; flown for a short window so the pick doesn't dither.
    pub avoid_dir: Option<Vec3>,
    pub avoid_timer: f32,
    // Target center last tick, for the lead-pursuit velocity estimate.
    pub last_target_center: Option<Vec3>,
    // Random phase so simultaneous missiles don't weave in lockstep.
    pub weave_phase: f32,
    pub lifetime_timer: f32,
    pub watchdog: ProgressWatchdog,
    // Self-hit gate, armed once the missile has cleared the shooter's collider.
    pub armed: bool,
    // Direction of the last `SMissileMove`; the next broadcast fires
    // when the steered direction drifts past an epsilon from this.
    pub last_broadcast_dir: Vec3,
    // Set by guidance (lifetime, stall, proximity fuse): where to detonate
    // this tick. The fuse sets the closest-approach point so the blast core
    // covers the target even when the fuse trips mid-travel.
    pub detonate_at: Option<Position>,
}

impl MissileInfo {
    // Fresh launch state; every timer/steering field starts at its inert
    // default so new guidance fields have one construction site (plus the
    // test fixture mirroring this).
    #[must_use]
    pub fn new(
        entity: Entity,
        shooter: PlayerId,
        target: Option<HomingTarget>,
        launch_dir: Vec3,
        weave_phase: f32,
        lifetime_secs: f32,
    ) -> Self {
        Self {
            entity,
            shooter,
            target,
            path: VecDeque::new(),
            path_target: None,
            path_retry_timer: 0.0,
            avoid_dir: None,
            avoid_timer: 0.0,
            last_target_center: None,
            weave_phase,
            lifetime_timer: lifetime_secs,
            watchdog: ProgressWatchdog::default(),
            armed: false,
            last_broadcast_dir: launch_dir,
            detonate_at: None,
        }
    }
}

#[derive(Resource, Default)]
pub struct MissileMap {
    map: HashMap<MissileId, MissileInfo>,
    next_id: u32,
}

impl MissileMap {
    pub fn allocate(&mut self) -> MissileId {
        let id = MissileId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        id
    }

    pub fn insert(&mut self, id: MissileId, info: MissileInfo) -> Option<MissileInfo> {
        self.map.insert(id, info)
    }

    pub fn remove(&mut self, id: &MissileId) -> Option<MissileInfo> {
        self.map.remove(id)
    }

    #[must_use]
    pub fn get(&self, id: &MissileId) -> Option<&MissileInfo> {
        self.map.get(id)
    }

    pub fn get_mut(&mut self, id: &MissileId) -> Option<&mut MissileInfo> {
        self.map.get_mut(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&MissileId, &MissileInfo)> {
        self.map.iter()
    }
}
