use bevy::prelude::*;
use common::{
    physics::ProgressWatchdog,
    protocol::{HomingTarget, PlayerId, Position},
};
use std::collections::VecDeque;

pub struct MissileFlight {
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
    // Set by guidance (lifetime, stall, proximity fuse): where to detonate
    // this tick. The fuse sets the closest-approach point so the blast core
    // covers the target even when the fuse trips mid-travel.
    pub detonate_at: Option<Position>,
}

impl MissileFlight {
    #[must_use]
    pub fn new(shooter: PlayerId, target: Option<HomingTarget>, weave_phase: f32, lifetime_secs: f32) -> Self {
        Self {
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
            detonate_at: None,
        }
    }
}
