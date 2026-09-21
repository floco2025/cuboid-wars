use std::collections::BTreeMap;

use bevy::prelude::*;
use common::{
    config::{ActorMovementConfig, CharacterPhysicsConfig},
    map::Carriers,
    physics::{CharacterSupport, CharacterVerticalVelocity, CollisionWorld, KnockbackVelocity},
    protocol::{ActorId, ActorMoveIntent, CarrierId, FaceYaw, MapSettings, Position, ServerTick, SwitchState},
};

use super::{
    roaming::route_stays_home,
    traversal::{TraversalAction, TraversalEnvironment, TraversalExecutor, TraversalStatus},
};
use crate::actors::{
    ActorCharacter, ActorCrushed, ActorLanding, ActorMap, ActorMode,
    navigation::{
        ActorTerritories, ActorTerritory,
        surface::{ROUTE_SEARCH_VISITS, RouteFailure, SurfaceNavigation},
    },
};

// Search, visibility, and funnel visits the ground actors share each tick;
// `ROUTE_SEARCH_VISITS` is the most one route may spend of them.
const TICK_SEARCH_VISITS: usize = 8192;
const ROUTE_RETRY_SECS: f32 = 0.5;
// How far a goal moves, or an idle actor stands from it, before a new route.
const GOAL_TOLERANCE: f32 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SurfaceGoal {
    pub carrier: CarrierId,
    pub position: Position,
}

#[derive(Component, Default)]
pub struct SurfaceAgent {
    pub(crate) goal: Option<SurfaceGoal>,
    pub(crate) executor: Option<TraversalExecutor>,
    pub(crate) failure: Option<RouteFailure>,
    pub(crate) decision_secs: f32,
    pub(crate) tactic_secs: f32,
    pub(crate) roam_index: usize,
    pub(crate) route_revision: Option<(CarrierId, u64)>,
    route_goal: Option<SurfaceGoal>,
    route_roaming: bool,
    retry_secs: f32,
    pending: bool,
    // The ladder this actor was admitted to last tick. The rotating order
    // would otherwise hand a ladder back and forth between two approaches.
    ladder_claim: Option<usize>,
}

impl SurfaceAgent {
    pub fn reached(&self) -> bool {
        self.executor
            .as_ref()
            .is_some_and(|e| e.status == TraversalStatus::Reached)
            && self.failure.is_none()
            && !self.pending
    }
}

struct RoutePlanner<'a> {
    navigation: &'a SurfaceNavigation,
    carriers: &'a Carriers,
    budget: usize,
}

impl RoutePlanner<'_> {
    fn update(
        &mut self,
        agent: &mut SurfaceAgent,
        executor: &mut TraversalExecutor,
        carrier: CarrierId,
        position: Position,
        physics: CharacterPhysicsConfig,
        ladders: bool,
        home: Option<&ActorTerritory>,
    ) {
        let Some(goal) = agent.goal else {
            executor.actions.clear();
            agent.route_goal = None;
            agent.failure = None;
            agent.pending = false;
            return;
        };
        // The carriers already advanced to this tick while the actor still
        // stands where the previous pose left it.
        let start = SurfaceGoal {
            carrier,
            position: self
                .carriers
                .previous_pose(carrier)
                .inverse_transform_position(&position),
        };
        if self.navigation.mesh_at(start, physics).is_none() {
            executor.actions.clear();
            agent.pending = self.navigation.rebuilding();
            agent.failure = (!agent.pending).then_some(RouteFailure::NavigationUnavailable);
            agent.route_revision = None;
            return;
        }
        let revision = Some((carrier, self.navigation.revision()));
        let policy_changed = agent.route_roaming != home.is_some();
        if policy_changed && home.is_some() {
            executor.actions.clear();
        }
        let stale = agent.route_revision != revision || policy_changed;
        let moved = agent.route_goal.is_none_or(|old| {
            old.carrier != goal.carrier || old.position.distance_sq(&goal.position) > GOAL_TOLERANCE.powi(2)
        });
        let interrupted = matches!(executor.status, TraversalStatus::Blocked | TraversalStatus::LostSupport);
        let short = executor.actions.is_empty()
            && position.distance_sq(&self.carriers.pose(goal.carrier).transform_position(&goal.position))
                > GOAL_TOLERANCE.powi(2);
        let retry = agent.retry_secs <= 0.0 && (agent.failure.is_some() || interrupted || short);
        if !(stale || moved || retry) {
            // A deferred retry whose cause cleared by itself requests nothing more.
            agent.pending = false;
            return;
        }
        if self.budget == 0 {
            // The route in hand serves until this actor's turn comes: the
            // motor's support checks guard what a newer mesh would change.
            agent.pending = true;
            return;
        }
        agent.pending = false;
        agent.retry_secs = ROUTE_RETRY_SECS;
        let limit = self.budget.min(ROUTE_SEARCH_VISITS);
        match self.navigation.route(start, goal, physics, ladders, limit) {
            Ok(route) => {
                self.budget -= route.expanded;
                if home.is_some_and(|home| !route_stays_home(&route, start, home, self.carriers)) {
                    executor.actions.clear();
                    agent.failure = Some(RouteFailure::OutsideTerritory);
                } else {
                    executor.set_route(route);
                    agent.failure = None;
                }
            }
            Err(RouteFailure::NavigationUnavailable) => {
                agent.pending = true;
                agent.failure = None;
                if stale {
                    executor.actions.clear();
                }
                return;
            }
            // The tick's leftover budget cut this search short, not the
            // route's own limit: it is deferred to the next tick, not failed.
            Err(RouteFailure::SearchLimit) if limit < ROUTE_SEARCH_VISITS => {
                self.budget = 0;
                agent.pending = true;
                agent.retry_secs = 0.0;
                return;
            }
            Err(reason) => {
                if reason == RouteFailure::SearchLimit {
                    self.budget -= limit;
                }
                executor.actions.clear();
                agent.failure = Some(reason);
            }
        }
        agent.route_revision = revision;
        agent.route_goal = Some(goal);
        agent.route_roaming = home.is_some();
    }
}

pub(crate) fn surface_actors_movement_system(
    time: Res<Time>,
    tick: Res<ServerTick>,
    collision: Res<CollisionWorld>,
    carriers: Res<Carriers>,
    settings: Res<MapSettings>,
    switches: Res<SwitchState>,
    navigation: Res<SurfaceNavigation>,
    territories: Res<ActorTerritories>,
    mut actors: ResMut<ActorMap>,
    mut query: Query<(
        &ActorId,
        &ActorCharacter,
        &ActorMovementConfig,
        &mut SurfaceAgent,
        &mut Position,
        &mut CharacterVerticalVelocity,
        &mut CharacterSupport,
        &mut ActorMoveIntent,
        &mut FaceYaw,
        &mut ActorCrushed,
        &mut ActorLanding,
        Option<&KnockbackVelocity>,
    )>,
) {
    let delta = time.delta_secs();
    if delta <= 0.0 {
        return;
    }
    let env = TraversalEnvironment {
        world: &collision,
        carriers: &carriers,
        settings: &settings,
        open: &switches.open_fields,
        delta,
    };
    let neighbors: Vec<_> = query
        .iter()
        .map(|(id, character, _, _, position, _, support, ..)| (*id, *position, character.0.physics(), *support))
        .collect();
    // Whoever is on a ladder holds it, then whoever was admitted last tick.
    let mut occupancy = BTreeMap::new();
    for (id, _, _, agent, _, _, support, ..) in &query {
        if *support == CharacterSupport::Ladder
            && let Some(ladder) = agent
                .executor
                .as_ref()
                .and_then(|executor| executor.actions.front())
                .and_then(|action| action.ladder())
        {
            occupancy.entry(ladder).or_insert(*id);
        }
    }
    for (id, _, _, agent, ..) in &query {
        if let Some(ladder) = agent.ladder_claim {
            occupancy.entry(ladder).or_insert(*id);
        }
    }
    let mut ordered: Vec<_> = query.iter_mut().collect();
    ordered.sort_by_key(|(id, ..)| id.0);
    if !ordered.is_empty() {
        let rotation = tick.0 as usize % ordered.len();
        ordered.rotate_left(rotation);
    }
    let mut planner = RoutePlanner {
        navigation: &navigation,
        carriers: &carriers,
        budget: TICK_SEARCH_VISITS,
    };
    for (
        id,
        character,
        speeds,
        mut agent,
        mut position,
        mut velocity,
        mut support,
        mut intent,
        mut facing,
        mut crushed,
        mut landing,
        knockback,
    ) in ordered
    {
        let Some(info) = actors.get_mut(id) else {
            continue;
        };
        let physics = character.0.physics();
        let home = matches!(info.mode, ActorMode::Roam).then(|| territories.get(info.spawn_zone_index));
        let speed = if matches!(info.mode, ActorMode::Engage { .. } | ActorMode::Evade { .. }) {
            speeds.active_speed
        } else {
            speeds.roam_speed
        };
        let mut executor = agent
            .executor
            .take()
            .unwrap_or_else(|| TraversalExecutor::new(*position, physics, speed, &env));
        executor.movement.position = *position;
        executor.movement.vertical_velocity = velocity.0;
        executor.movement.support = *support;
        executor.speed = speed;
        executor.facing = facing.0;
        agent.retry_secs = (agent.retry_secs - delta).max(0.0);
        let committed = executor.committed();
        if !committed {
            planner.update(
                &mut agent,
                &mut executor,
                info.carrier,
                *position,
                physics,
                character.0.can_use_ladders,
                home,
            );
        }
        let ladder = executor
            .actions
            .front()
            .and_then(|action| action.ladder())
            .or_else(|| approached_ladder(&executor, &carriers, *position, physics));
        let waiting = ladder.is_some_and(|ladder| *occupancy.entry(ladder).or_insert(*id) != *id);
        agent.ladder_claim = ladder.filter(|_| !waiting);
        let avoidance = if *support == CharacterSupport::Ground && !committed {
            crowd_velocity(*id, *position, physics, &neighbors).clamp_length_max(speed * 0.5)
        } else {
            Vec3::ZERO
        };
        executor.step_with_avoidance(
            &env,
            knockback.map_or(Vec3::ZERO, |v| v.step(delta)),
            avoidance,
            waiting,
            home,
        );
        if executor.status == TraversalStatus::OutsideTerritory {
            agent.failure = Some(RouteFailure::OutsideTerritory);
            agent.decision_secs = 0.0;
        }
        let movement = executor.movement;
        // Player bodies do not block these moves: current moving ground kinds
        // detonate on contact in contact_explosions_system later this tick.
        // Overlap in peaceful mode is accepted; revisit if a moving ground
        // kind without a contact attack is introduced.
        *position = movement.position;
        velocity.0 = movement.vertical_velocity;
        *support = movement.support;
        *intent = executor.intent;
        if let Some(direction) = intent.direction() {
            facing.0 = direction;
        }
        crushed.0 = movement.crushed;
        landing.0 = movement.impact_speed;
        info.carrier = movement
            .grounding
            .hit
            .filter(|_| movement.grounding.supported)
            .map_or(movement.carrier, |hit| hit.carrier);
        agent.executor = Some(executor);
    }
}

// A ladder the route mounts next, once the actor is close enough to contend for it.
fn approached_ladder(
    executor: &TraversalExecutor,
    carriers: &Carriers,
    position: Position,
    physics: CharacterPhysicsConfig,
) -> Option<usize> {
    executor.actions.iter().find_map(|action| {
        let TraversalAction::MountLadder {
            carrier,
            ladder,
            target,
        } = *action
        else {
            return None;
        };
        let mount = carriers.pose(carrier).transform_position(&target);
        (position.horizontal_distance_sq(&mount) < (physics.movement_collider.diameter + 1.0).powi(2)
            && (position.y - mount.y).abs() < 0.5)
            .then_some(ladder)
    })
}

fn crowd_velocity(
    id: ActorId,
    position: Position,
    physics: CharacterPhysicsConfig,
    neighbors: &[(ActorId, Position, CharacterPhysicsConfig, CharacterSupport)],
) -> Vec3 {
    let mut velocity = Vec3::ZERO;
    let radius = physics.movement_collider.radius();
    for &(other, point, body, support) in neighbors {
        if other == id || support != CharacterSupport::Ground {
            continue;
        }
        if position.y + physics.movement_collider.height <= point.y
            || point.y + body.movement_collider.height <= position.y
        {
            continue;
        }
        let offset = (Vec3::from(position) - Vec3::from(point)).with_y(0.0);
        let distance = offset.length();
        let clearance = radius + body.movement_collider.radius() + 0.2;
        if distance >= clearance {
            continue;
        }
        let direction = offset
            .try_normalize()
            .unwrap_or(if id.0 < other.0 { Vec3::X } else { Vec3::NEG_X });
        velocity += direction * (clearance - distance) * 2.0;
    }
    velocity
}

#[cfg(test)]
#[path = "tests/surface.rs"]
mod tests;
