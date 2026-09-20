use bevy::prelude::{Vec2, Vec3};
use rand::{Rng, RngExt};

use crate::{
    actors::{
        ActorInfo, ActorMode, ActorRoute,
        navigation::{
            ActorTerritory, GroundNavigation, GroundSearchOptions, GroundSearchResult, GroundTask, NavGraph, NavGraphs,
            PlannedRoute, evade_clearance, segment_threat_distance_sq,
        },
    },
    config::ActorKindServerConfig,
};
use common::{
    config::CharacterPhysicsConfig,
    map::{CarrierPose, Carriers},
    physics::CollisionWorld,
    protocol::{CarrierId, FieldId, PlayerId, Position},
};

pub(super) const EVADE_REPLAN_INTERVAL_SECS: f32 = 0.5;
// Expansions a pursuit may spend before the target counts as unreachable.
// The grounds continue the graph to the horizon, so a target no node can
// attack would otherwise keep the search, and the actor, waiting for good.
const PURSUIT_SEARCH_LIMIT: usize = 4096;

// Navigation happens in the actor's carrier frame and everything physical
// in the world: `pos` and every route position are carrier-local, the
// awareness and `world_pos` are world, and `pose` converts between them.
pub(super) struct BehaviorContext<'a> {
    pub(super) tick: u32,
    pub(super) pos: Position,
    pub(super) world_pos: Position,
    pub(super) pose: CarrierPose,
    pub(super) actor_physics: CharacterPhysicsConfig,
    pub(super) player_physics: CharacterPhysicsConfig,
    pub(super) nav_graph: &'a NavGraph,
    pub(super) nav_graphs: &'a NavGraphs,
    pub(super) carriers: &'a Carriers,
    pub(super) carrier: CarrierId,
    pub(super) territory: &'a ActorTerritory,
    pub(super) collision_world: &'a CollisionWorld,
    pub(super) open_fields: &'a [FieldId],
    pub(super) kind_config: &'a ActorKindServerConfig,
    // Whether the map lets players hurt actors at all.
    pub(super) players_armed: bool,
}

impl BehaviorContext<'_> {
    fn navigation<'a>(&'a self, kind: &'a str) -> GroundNavigation<'a> {
        GroundNavigation {
            graphs: self.nav_graphs,
            carriers: self.carriers,
            carrier: self.carrier,
            kind,
            world: self.collision_world,
            physics: self.actor_physics,
            open: self.open_fields,
        }
    }

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
            if matches!(info.mode, ActorMode::Roam)
                && planned
                    .waypoints
                    .iter()
                    .any(|point| !self.territory.contains_position(point.position.into()))
            {
                return None;
            }
            ActorRoute::new(planned)
        });
        info.set_route(route);
    }

    // `candidate` is carrier-local; the threats are world positions.
    pub(super) fn stable_cover(&self, candidate: &Position, threats: &[Position]) -> bool {
        super::geometry::covered(Vec3::from(self.to_world(candidate)), threats, &self.into())
    }
}

// Hiding beats running: a stable cover route is kept while its destination
// still hides, and a fresh search after the replan interval replaces one
// that got exposed. With no cover in reach the actor takes a reachable retreat
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
    if !matches!(info.mode, ActorMode::Evade { .. }) {
        info.ground.evade_tier = 0;
        info.ground.seed = rng.random();
    }
    let safety = |pos: Position| super::geometry::threat_distance_sq(pos.into(), &threats);
    let clearance =
        context.actor_physics.movement_collider.radius() + context.player_physics.movement_collider.radius();
    let tier = info.ground.evade_tier;
    let minimum = evade_clearance(safety(context.world_pos), clearance, tier);
    let seed = info.ground.seed;
    let retreat = |pos: Position| {
        let distance = safety(pos);
        if distance <= safety(context.world_pos) {
            f32::NEG_INFINITY
        } else {
            distance.sqrt() + destination_variation(pos, seed) * clearance
        }
    };
    let task = GroundTask::Evade(tier);
    let target = threats.first().copied().unwrap_or(context.world_pos);
    let result = info.ground.route(
        &context.navigation(&info.spawn_kind),
        task,
        context.world_pos,
        target,
        |pos, _| context.stable_cover(&context.to_local(&pos), &threats).then_some(pos),
        |from, to| segment_threat_distance_sq(from.into(), to.into(), &threats) + 0.00001 >= minimum,
        GroundSearchOptions {
            fallback: Some(&retreat),
            expansion_limit: Some(256),
            ..Default::default()
        },
    );
    info.mode = ActorMode::Evade { fleeing: true };
    match result {
        GroundSearchResult::Pending => info.decision_timer = 0.0,
        GroundSearchResult::Found(planned) => {
            let fleeing = planned
                .waypoints
                .back()
                .is_none_or(|point| !context.stable_cover(&point.position, &threats));
            info.mode = ActorMode::Evade { fleeing };
            context.install_route(info, Some(planned));
            info.evade_replan_remaining_secs = EVADE_REPLAN_INTERVAL_SECS;
        }
        GroundSearchResult::Unreachable => {
            info.ground.evade_tier = (tier + 1).min(2);
            info.ground.seed = rng.random();
            info.decision_timer = if tier < 2 { 0.0 } else { EVADE_REPLAN_INTERVAL_SECS };
        }
    }
}

pub(super) fn enter_roam_or_return(info: &mut ActorInfo, context: &BehaviorContext<'_>, rng: &mut impl Rng) {
    let roaming = context.territory.contains_position(context.pos.into());
    let mode = if roaming {
        ActorMode::Roam
    } else {
        ActorMode::ReturnHome
    };
    if info.mode == mode && info.route.is_some() {
        return;
    }
    if info.mode != mode {
        info.set_route(None);
    }
    info.mode = mode;
    info.evade_replan_remaining_secs = 0.0;
    let task = if roaming { GroundTask::Roam } else { GroundTask::Return };
    if !info.ground.pending(task) {
        info.ground.seed = rng.random();
    }
    let seed = info.ground.seed;
    let score = |pos: Position| {
        if pos.distance_sq(&context.world_pos) < context.actor_physics.movement_collider.radius().powi(2) {
            f32::NEG_INFINITY
        } else {
            destination_variation(pos, seed)
        }
    };
    let travel_home = |pos: Position| context.territory.distance_outside(context.to_local(&pos).into());
    let result = info.ground.route(
        &context.navigation(&info.spawn_kind),
        task,
        context.world_pos,
        context.to_world(&context.territory.volume.min.into()),
        |pos, _| (!roaming && context.territory.contains_position(context.to_local(&pos).into())).then_some(pos),
        |from, to| {
            !roaming
                || context
                    .territory
                    .path_contains(context.to_local(&from).into(), context.to_local(&to).into())
        },
        GroundSearchOptions {
            heuristic: (!roaming).then_some(&travel_home as &dyn Fn(Position) -> f32),
            fallback: roaming.then_some(&score as &dyn Fn(Position) -> f32),
            expansion_limit: roaming.then_some(128),
        },
    );
    match result {
        GroundSearchResult::Pending => info.decision_timer = 0.0,
        GroundSearchResult::Found(route) => context.install_route(info, Some(route)),
        GroundSearchResult::Unreachable => {
            let route = if roaming {
                context.navigation(&info.spawn_kind).local_roam(
                    context.world_pos,
                    |pos| context.territory.contains_position(context.to_local(&pos).into()),
                    &mut info.ground.work,
                    rng,
                )
            } else {
                None
            };
            if route.is_some() {
                context.install_route(info, route);
            } else {
                info.decision_timer = EVADE_REPLAN_INTERVAL_SECS;
            }
        }
    }
}

fn destination_variation(pos: Position, seed: u32) -> f32 {
    let mut hash = seed ^ pos.x.to_bits().rotate_left(7) ^ pos.y.to_bits().rotate_left(13) ^ pos.z.to_bits();
    hash ^= hash >> 16;
    hash = hash.wrapping_mul(0x7feb_352d);
    hash ^= hash >> 15;
    hash = hash.wrapping_mul(0x846c_a68b);
    (hash ^ (hash >> 16)) as f32 / u32::MAX as f32
}

pub(super) fn keep_or_install_engagement_route(
    info: &mut ActorInfo,
    context: &BehaviorContext<'_>,
    target: PlayerId,
    target_pos: Position,
) -> bool {
    let beam = &context.into();
    if matches!(info.mode, ActorMode::Engage { target: current, target_pos: previous } if current == target && previous.distance_sq(&target_pos) < 0.04)
        && let Some(route) = &info.route
        && route.waypoints.front().is_some_and(|point| {
            context.collision_world.character_ground_route_clear(
                context.world_pos,
                context.to_world(&point.position),
                context.actor_physics,
                context.open_fields,
            )
        })
        && (super::geometry::attack_position(context.to_world(&route.destination), target_pos, beam)
            || !context.nav_graph.contains(&route.destination))
    {
        return true;
    }
    // Routes walk the grid one axis at a time, so the walk left to the
    // target is at least its distance along both.
    let travel_to_target = |pos: Position| (target_pos.x - pos.x).abs() + (target_pos.z - pos.z).abs();
    // A contact attacker aims for the touching point on its own side of the
    // target, not the target itself: a body placed on the target overlaps
    // whatever the target stands against, and a wall-hugging player would
    // then have no attack position at all.
    let standoff = context.kind_config.attack.contact_trigger_gap().map(|gap| {
        context.actor_physics.movement_collider.radius() + context.player_physics.movement_collider.radius() + gap * 0.5
    });
    let goal = |pos: Position, cell_size: f32| {
        if super::geometry::attack_position(pos, target_pos, beam) {
            return Some(pos);
        }
        let reach = (cell_size * 0.5 - context.actor_physics.movement_collider.radius()).max(0.0);
        let mut aim = Vec2::new(target_pos.x, target_pos.z);
        if let Some(standoff) = standoff {
            let toward = aim - Vec2::new(pos.x, pos.z);
            let distance = toward.length();
            if distance > standoff {
                aim -= toward * (standoff / distance);
            }
        }
        let candidate = Position {
            x: aim.x.clamp(pos.x - reach, pos.x + reach),
            y: pos.y,
            z: aim.y.clamp(pos.z - reach, pos.z + reach),
        };
        super::geometry::attack_position(candidate, target_pos, beam).then_some(candidate)
    };
    let navigation = context.navigation(&info.spawn_kind);
    let task = GroundTask::Pursue(target);
    // An airborne target with no nearby attack position must not trigger a search across the grounds.
    if let Some(gap) = context.kind_config.attack.contact_trigger_gap()
        && !navigation.has_goal_near(
            context.world_pos,
            target_pos,
            context.actor_physics.movement_collider.radius() + context.player_physics.movement_collider.radius() + gap,
            goal,
        )
    {
        info.ground.cancel(task);
        return false;
    }
    let result = info.ground.route(
        &navigation,
        task,
        context.world_pos,
        target_pos,
        goal,
        |_, _| true,
        GroundSearchOptions {
            heuristic: Some(&travel_to_target),
            expansion_limit: Some(PURSUIT_SEARCH_LIMIT),
            ..Default::default()
        },
    );
    let planned = match result {
        GroundSearchResult::Pending => {
            info.mode = ActorMode::Engage { target, target_pos };
            info.decision_timer = 0.0;
            return true;
        }
        GroundSearchResult::Unreachable => return false,
        GroundSearchResult::Found(route) => route,
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
