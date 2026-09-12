use bevy::prelude::Vec3;
use rand::Rng;

use crate::{
    actors::{
        ActorInfo, ActorMode, ActorRoute,
        navigation::{ActorTerritory, GroundNavigation, NavGraph, NavGraphs, PlannedRoute},
    },
    config::ActorKindServerConfig,
};
use common::{
    config::CharacterPhysicsConfig,
    map::{CarrierPose, Carriers},
    physics::CollisionWorld,
    protocol::{BarrierId, CarrierId, PlayerId, Position},
};

pub(super) const EVADE_REPLAN_INTERVAL_SECS: f32 = 0.5;

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
    pub(super) open_barriers: &'a [BarrierId],
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
            open: self.open_barriers,
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
pub(super) fn enter_evade(info: &mut ActorInfo, context: &BehaviorContext<'_>, _rng: &mut impl Rng) {
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
    let safety = |pos: Position| super::geometry::threat_distance_sq(pos.into(), &threats);
    let minimum =
        safety(context.world_pos).min((context.actor_physics.movement_collider.radius() * 4.0).powi(2)) * 0.25;
    let planned = context.navigation(&info.spawn_kind).route(
        context.world_pos,
        |pos, _| context.stable_cover(&context.to_local(&pos), &threats).then_some(pos),
        |pos| safety(pos) >= minimum,
        256,
        Some(&safety),
    );
    let fleeing = planned
        .as_ref()
        .and_then(|route| route.waypoints.back())
        .is_none_or(|point| !context.stable_cover(&point.position, &threats));
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
        let route = context.navigation(&info.spawn_kind).roam(
            context.world_pos,
            |pos| context.territory.contains_position(context.to_local(&pos).into()),
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
        let route = context.navigation(&info.spawn_kind).route(
            context.world_pos,
            |pos, _| {
                context
                    .territory
                    .contains_position(context.to_local(&pos).into())
                    .then_some(pos)
            },
            |_| true,
            usize::MAX,
            None,
        );
        context.install_route(info, route);
    }
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
                context.open_barriers,
            )
        })
        && (super::geometry::attack_position(context.to_world(&route.destination), target_pos, beam)
            || !context.nav_graph.contains(&route.destination))
    {
        return true;
    }
    let planned = context.navigation(&info.spawn_kind).route(
        context.world_pos,
        |pos, cell_size| {
            if super::geometry::attack_position(pos, target_pos, beam) {
                return Some(pos);
            }
            let reach = (cell_size * 0.5 - context.actor_physics.movement_collider.radius()).max(0.0);
            let candidate = Position {
                x: target_pos.x.clamp(pos.x - reach, pos.x + reach),
                y: pos.y,
                z: target_pos.z.clamp(pos.z - reach, pos.z + reach),
            };
            super::geometry::attack_position(candidate, target_pos, beam).then_some(candidate)
        },
        |_| true,
        usize::MAX,
        None,
    );
    let Some(planned) = planned else {
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
