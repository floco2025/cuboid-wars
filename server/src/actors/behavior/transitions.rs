use bevy::prelude::Vec3;
use rand::Rng;

use crate::{
    actors::{
        ActorInfo, ActorMode, ActorRoute,
        navigation::{ActorTerritory, NavGraph, PlannedRoute, WALK_REACH_DISTANCE},
    },
    config::ActorKindServerConfig,
};
use common::{
    config::CharacterPhysicsConfig,
    map::CarrierPose,
    physics::CollisionWorld,
    protocol::{BarrierKindId, PlayerId, Position},
};

pub(super) const EVADE_REPLAN_INTERVAL_SECS: f32 = 0.5;
// Cover closer than this to a threat (in grid cells) is not cover.
const COVER_MIN_THREAT_DISTANCE_CELLS: f32 = 0.75;

// Navigation happens in the actor's carrier frame and everything physical
// in the world: `pos` and every route position are carrier-local, the
// awareness and `world_pos` are world, and `pose` converts between them.
pub(super) struct BehaviorContext<'a> {
    pub(super) tick: u32,
    pub(super) pos: Position,
    pub(super) world_pos: Position,
    pub(super) pose: CarrierPose,
    pub(super) actor_physics: CharacterPhysicsConfig,
    pub(super) actor_eye_height: f32,
    pub(super) player_physics: CharacterPhysicsConfig,
    pub(super) nav_graph: &'a NavGraph,
    pub(super) territory: &'a ActorTerritory,
    pub(super) collision_world: &'a CollisionWorld,
    pub(super) open_barriers: &'a [BarrierKindId],
    pub(super) kind_config: &'a ActorKindServerConfig,
    // Whether the map lets players hurt actors at all.
    pub(super) players_armed: bool,
}

impl BehaviorContext<'_> {
    pub(super) fn to_world(&self, local: &Position) -> Position {
        self.pose.transform_position(local)
    }

    pub(super) fn to_local(&self, world: &Position) -> Position {
        self.pose.inverse_transform_position(world)
    }

    pub(super) fn install_route(&self, info: &mut ActorInfo, planned: Option<PlannedRoute>) {
        let planned = planned.or_else(|| {
            self.nav_graph
                .ladder_exit_route(&self.pos, self.nav_graph.ladder_links(&info.spawn_kind))
        });
        let route = planned.and_then(|mut planned| {
            self.nav_graph.anchor_route_start(
                self.nav_graph.ladder_links(&info.spawn_kind),
                &self.pos,
                &mut planned,
                self.collision_world,
                self.actor_physics,
                self.pose,
            );
            ActorRoute::new(planned)
        });
        info.set_route(route);
    }

    // `candidate` is carrier-local; the threats are world positions.
    pub(super) fn stable_cover(&self, candidate: &Position, threats: &[Position]) -> bool {
        let pos = &self.to_world(candidate);
        let min_threat_distance = COVER_MIN_THREAT_DISTANCE_CELLS * self.nav_graph.cell_size();
        if threats
            .iter()
            .any(|threat| pos.horizontal_distance_sq(threat) < min_threat_distance * min_threat_distance)
        {
            return false;
        }
        let margin = self.actor_physics.movement_collider.radius() + WALK_REACH_DISTANCE;
        let samples = [
            *pos,
            Position {
                x: pos.x + margin,
                ..*pos
            },
            Position {
                x: pos.x - margin,
                ..*pos
            },
            Position {
                z: pos.z + margin,
                ..*pos
            },
            Position {
                z: pos.z - margin,
                ..*pos
            },
        ];
        threats.iter().all(|threat| {
            let target = Vec3::new(threat.x, self.player_physics.hitbox_center_y(threat.y), threat.z);
            samples.iter().all(|sample| {
                let eye = Vec3::new(sample.x, sample.y + self.actor_eye_height, sample.z);
                !self.collision_world.line_of_sight_clear(eye, target)
            })
        })
    }
}

// Hiding beats running: a stable cover route is kept while its destination
// still hides, and a fresh search after the replan interval replaces one
// that got exposed. With no cover in reach the actor flees to a random cell
// instead, and that leg is kept until it ends, since a flight re-rolled
// every decision is a jitter, not a flight.
pub(super) fn enter_evade(info: &mut ActorInfo, context: &BehaviorContext<'_>, rng: &mut impl Rng) {
    let threats: Vec<_> = info.awareness.iter().map(|aware| aware.pos).collect();
    let evading = match info.mode {
        ActorMode::Evade { fleeing } => Some(fleeing),
        _ => None,
    };
    if let Some(fleeing) = evading
        && info
            .route
            .as_ref()
            .is_some_and(|route| fleeing || context.stable_cover(&route.destination, &threats))
    {
        return;
    }

    if context.stable_cover(&context.pos, &threats) {
        info.mode = ActorMode::Evade { fleeing: false };
        context.install_route(info, None);
        info.evade_replan_remaining_secs = EVADE_REPLAN_INTERVAL_SECS;
        return;
    }
    if evading == Some(false) && info.evade_replan_remaining_secs > 0.0 {
        return;
    }
    // The searches measure cell distances in the graph's frame.
    let local_threats: Vec<_> = threats.iter().map(|threat| context.to_local(threat)).collect();
    let cover = context.nav_graph.safe_cover_route(
        context.nav_graph.ladder_links(&info.spawn_kind),
        &context.pos,
        &local_threats,
        |candidate| context.stable_cover(candidate, &threats),
    );
    let fleeing = cover.is_none();
    let planned = cover.or_else(|| {
        context.nav_graph.flee_route(
            context.nav_graph.ladder_links(&info.spawn_kind),
            &context.pos,
            &local_threats,
            rng,
        )
    });
    info.mode = ActorMode::Evade { fleeing };
    context.install_route(info, planned);
    info.evade_replan_remaining_secs = EVADE_REPLAN_INTERVAL_SECS;
}

pub(super) fn enter_roam_or_return(info: &mut ActorInfo, context: &BehaviorContext<'_>, rng: &mut impl Rng) {
    if context
        .nav_graph
        .position_in_roam_region(&context.pos, context.territory)
    {
        let continuing_roam = matches!(info.mode, ActorMode::Roam) && info.route.is_some();
        info.mode = ActorMode::Roam;
        info.evade_replan_remaining_secs = 0.0;
        if continuing_roam {
            return;
        }
        let route = context.nav_graph.roam_route(
            context.nav_graph.ladder_links(&info.spawn_kind),
            &context.pos,
            context.territory,
            rng,
        );
        context.install_route(info, route);
    } else {
        let continuing_return = matches!(info.mode, ActorMode::ReturnHome) && info.route.is_some();
        info.mode = ActorMode::ReturnHome;
        info.evade_replan_remaining_secs = 0.0;
        if continuing_return {
            return;
        }
        let route = context.nav_graph.return_route(
            context.nav_graph.ladder_links(&info.spawn_kind),
            &context.pos,
            context.territory,
        );
        context.install_route(info, route);
    }
}

// `target_pos` is the world anchor; the route is planned toward it in the
// actor's carrier frame, and a target off the actor's map is unreachable.
pub(super) fn keep_or_install_engagement_route(
    info: &mut ActorInfo,
    context: &BehaviorContext<'_>,
    target: PlayerId,
    target_pos: Position,
) -> bool {
    let anchor = context.to_local(&target_pos);
    if !context.nav_graph.contains(&anchor) {
        return false;
    }
    let Some(target_node) = context.nav_graph.nearest_node_for_position(&anchor) else {
        return false;
    };
    if matches!(info.mode, ActorMode::Engage { target: route_target, .. } if route_target == target)
        && let Some(route) = &mut info.route
        && route.destination_node == target_node
        && route.waypoints.back().is_some_and(|point| point.is_walk())
    {
        let final_leg_start = route
            .waypoints
            .iter()
            .rev()
            .nth(1)
            .map(|point| point.position)
            .unwrap_or(context.pos);
        if context.nav_graph.engagement_retarget_is_valid(
            &final_leg_start,
            &anchor,
            context.actor_physics.movement_collider.radius(),
            context.actor_physics.movement_collider.radius(),
        ) {
            route.retarget(anchor);
            info.mode = ActorMode::Engage { target, target_pos };
            info.evade_replan_remaining_secs = 0.0;
            return true;
        }
    }
    let Some(planned) = context.nav_graph.engagement_route(
        context.nav_graph.ladder_links(&info.spawn_kind),
        &context.pos,
        &anchor,
        context.actor_physics.movement_collider.radius(),
        context.actor_physics.movement_collider.radius(),
    ) else {
        return false;
    };
    info.mode = ActorMode::Engage { target, target_pos };
    info.evade_replan_remaining_secs = 0.0;
    context.install_route(info, Some(planned));
    true
}

pub(super) fn install_ladder_engagement(
    info: &mut ActorInfo,
    context: &BehaviorContext<'_>,
    target: PlayerId,
    target_pos: Position,
) -> bool {
    if !context.kind_config.character.can_use_ladders {
        return false;
    }
    let local_target = context.to_local(&target_pos);
    let Some(planned) = context.nav_graph.ladder_target_route(
        &context.pos,
        &local_target,
        context.nav_graph.ladder_links(&info.spawn_kind),
        context.actor_physics,
    ) else {
        return false;
    };
    info.mode = ActorMode::Engage { target, target_pos };
    info.evade_replan_remaining_secs = 0.0;
    context.install_route(info, Some(planned));
    true
}
