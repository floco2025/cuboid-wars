use std::collections::VecDeque;

use bevy::prelude::{Entity, Vec3};
use common::{
    config::CharacterPhysicsConfig,
    constants::{CHARACTER_GROUND_SNAP_DISTANCE, CHARACTER_MAX_SLOPE, CHARACTER_STEP_HEIGHT},
    map::Carriers,
    physics::{CharacterMovementResult, CharacterSupport, CollisionWorld, grounding_diagnostics},
    protocol::{ActorMoveIntent, CarrierId, FieldId, MapSettings, Position},
};

use super::steering::steer;
pub use crate::actors::navigation::surface::TraversalAction;
use crate::actors::navigation::{
    ActorTerritory,
    surface::{CarrierDock, SurfaceRoute},
};
use crate::actors::{ActorMovementStep, step_actor_movement};

const ARRIVAL_DISTANCE: f32 = 0.15;
const STALL_SECONDS: f32 = 2.0;
// How long two movers block each other without progress before the caller
// lets them pass; shorter than a stall, so a standoff ends before its routes fail.
const STANDOFF_SECONDS: f32 = 1.0;
// How far past its own body a walker going around another still treats that
// body as in its way.
const SIDESTEP_LOOKAHEAD: f32 = 1.0;
// The approach that counts as progress. A blocked walker re-approaches its
// blocker in whole steps, so a finer measure keeps finding new records.
const PROGRESS_DISTANCE: f32 = 0.05;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraversalStatus {
    Moving,
    Waiting,
    Reached,
    Blocked,
    LostSupport,
    OutsideTerritory,
    Crushed,
}

// The body that rejected a move and where it stands from the mover.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BodyBlocker {
    pub entity: Entity,
    pub offset: Vec3,
}

// The body the executor moves, as the last step left it. The ECS components
// own it; the executor only reads it for one step and returns the result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActorBody {
    pub position: Position,
    pub vertical_velocity: f32,
    pub support: CharacterSupport,
}

impl ActorBody {
    #[cfg(test)]
    pub const fn standing(position: Position) -> Self {
        Self {
            position,
            vertical_velocity: 0.0,
            support: CharacterSupport::Ground,
        }
    }
}

impl From<&CharacterMovementResult> for ActorBody {
    fn from(movement: &CharacterMovementResult) -> Self {
        Self {
            position: movement.position,
            vertical_velocity: movement.vertical_velocity,
            support: movement.support,
        }
    }
}

pub struct TraversalEnvironment<'a> {
    pub world: &'a CollisionWorld,
    pub carriers: &'a Carriers,
    pub settings: &'a MapSettings,
    pub open: &'a [FieldId],
    pub delta: f32,
}

// The route in progress and how its execution is going.
#[derive(Debug, Clone)]
pub struct TraversalExecutor {
    pub actions: VecDeque<TraversalAction>,
    pub status: TraversalStatus,
    pub intent: ActorMoveIntent,
    pub physics: CharacterPhysicsConfig,
    pub speed: f32,
    pub(crate) facing: f32,
    pub(crate) blocked_by: Option<Entity>,
    // The hand a blocked walker turns to, kept while the blockage lasts so an
    // opening on the other side cannot turn it back.
    sidestep_hand: Option<f32>,
    // The action being timed and the closest the actor has come to its target.
    progress: Option<(TraversalAction, f32)>,
    stalled_secs: f32,
}

// What the front of the route asks of this tick.
#[derive(Debug, Clone, Copy)]
enum Pending {
    Idle,
    Wait,
    Walk { target: Position },
    Mount { target: Position },
    Climb { target: Position, intent: ActorMoveIntent },
    Exit { target: Position },
    Board { target: Position },
}

impl Pending {
    // The point a horizontal move heads for.
    const fn target(self) -> Option<Position> {
        match self {
            Self::Walk { target } | Self::Mount { target } | Self::Exit { target } | Self::Board { target } => {
                Some(target)
            }
            Self::Idle | Self::Wait | Self::Climb { .. } => None,
        }
    }

    const fn destination(self) -> Option<Position> {
        match self {
            Self::Climb { target, .. } => Some(target),
            _ => self.target(),
        }
    }

    const fn walking(self) -> bool {
        matches!(self, Self::Walk { .. })
    }
}

impl TraversalExecutor {
    #[must_use]
    pub fn new(physics: CharacterPhysicsConfig, speed: f32) -> Self {
        Self {
            actions: VecDeque::new(),
            status: TraversalStatus::Reached,
            intent: ActorMoveIntent::Idle,
            physics,
            speed,
            facing: 0.0,
            blocked_by: None,
            sidestep_hand: None,
            progress: None,
            stalled_secs: 0.0,
        }
    }

    pub fn set_route(&mut self, route: SurfaceRoute) {
        self.actions = route.actions;
        self.status = TraversalStatus::Moving;
        self.stalled_secs = 0.0;
    }

    // A ladder, a boarding, or a ride runs to its end through ordinary goal
    // changes; only a failed one is open to replanning.
    pub(crate) fn committed(&self) -> bool {
        self.actions.front().is_some_and(|action| action.committed())
            && !matches!(
                self.status,
                TraversalStatus::Blocked | TraversalStatus::LostSupport | TraversalStatus::OutsideTerritory
            )
    }

    // The body this actor has made no progress against for a standoff's length.
    pub(crate) fn standoff(&self) -> Option<Entity> {
        self.blocked_by.filter(|_| self.stalled_secs >= STANDOFF_SECONDS)
    }

    // Steps `body` in place with nothing in the way.
    #[cfg(test)]
    pub fn step(&mut self, env: &TraversalEnvironment, body: &mut ActorBody) -> CharacterMovementResult {
        let movement = self.step_with_avoidance(env, *body, Vec3::ZERO, Vec3::ZERO, false, None, |_| None);
        *body = ActorBody::from(&movement);
        movement
    }

    // One tick of the route: pops the actions the body has completed, moves
    // it toward the next, and works around the body `blocking_actor` reports
    // at a candidate target.
    pub(crate) fn step_with_avoidance(
        &mut self,
        env: &TraversalEnvironment,
        body: ActorBody,
        knockback_displacement: Vec3,
        avoidance: Vec3,
        waiting: bool,
        home: Option<&ActorTerritory>,
        blocking_actor: impl Fn(Position) -> Option<BodyBlocker>,
    ) -> CharacterMovementResult {
        let start = body.position;
        let physics = self.physics;
        let pending = if waiting {
            Pending::Idle
        } else {
            self.advance_route(env, &body)
        };
        let intent = self.travel_intent(env, &body, pending, avoidance, waiting);
        self.intent = intent;
        let step = |intent: ActorMoveIntent, knockback_displacement: Vec3| {
            step_actor_movement(ActorMovementStep {
                start,
                vertical_velocity: body.vertical_velocity,
                intent,
                knockback_displacement,
                delta: env.delta,
                can_use_ladders: intent.uses_ladders(),
                physics,
                open_fields: env.open,
                collision_world: env.world,
                map_settings: env.settings,
                carriers: env.carriers,
            })
        };
        let inside_home = |position: Position| {
            home.is_none_or(|home| {
                home.contains_position(env.carriers.pose(home.carrier).inverse_transform_point(position.into()))
            })
        };
        let mut movement = step(intent, knockback_displacement);
        if !inside_home(movement.position) {
            // Steering can leave a valid route's territory. Rerun the complete
            // motor without voluntary travel, retaining gravity, impulses and
            // carrier motion; clamping the result would corrupt its support.
            self.intent = stopped_intent(intent);
            movement = step(self.intent, knockback_displacement);
            self.actions.clear();
            self.status = TraversalStatus::OutsideTerritory;
        }
        let blocker = self.probe_blocker(env, &body, pending, &movement, &blocking_actor);
        self.blocked_by = blocker.map(|blocker| blocker.entity);
        if let Some(blocker) = blocker {
            movement = self.resolve_blocker(
                env,
                &body,
                pending,
                blocker,
                knockback_displacement,
                &step,
                &inside_home,
                &blocking_actor,
            );
        } else {
            self.sidestep_hand = None;
        }
        if let Some(direction) = self.intent.direction() {
            self.facing = direction;
        }
        if movement.crushed {
            self.status = TraversalStatus::Crushed;
        }
        self.record_progress(env, pending, &movement);
        movement
    }

    // Pops every action the body has completed and returns what the next one
    // asks for; a dock not yet in place or a lost ride sets the status itself.
    fn advance_route(&mut self, env: &TraversalEnvironment, body: &ActorBody) -> Pending {
        let start = body.position;
        self.status = TraversalStatus::Reached;
        while let Some(action) = self.actions.front().copied() {
            match action {
                TraversalAction::Walk { carrier, target: local } => {
                    let destination = env.carriers.pose(carrier).transform_position(&local);
                    if start.horizontal_distance_sq(&destination) <= ARRIVAL_DISTANCE.powi(2)
                        && (start.y - destination.y).abs() <= 0.25 + slope_clearance(self.physics)
                    {
                        self.actions.pop_front();
                        continue;
                    }
                    return Pending::Walk { target: destination };
                }
                TraversalAction::MountLadder {
                    carrier, target: local, ..
                } => {
                    let destination = env.carriers.pose(carrier).transform_position(&local);
                    if start.horizontal_distance_sq(&destination) <= ARRIVAL_DISTANCE.powi(2) {
                        self.actions.pop_front();
                        continue;
                    }
                    return Pending::Mount { target: destination };
                }
                TraversalAction::Climb {
                    carrier,
                    target: local,
                    normal,
                    ascending,
                    ..
                } => {
                    let destination = env.carriers.pose(carrier).transform_position(&local);
                    if (ascending && start.y >= destination.y) || (!ascending && start.y <= destination.y) {
                        self.actions.pop_front();
                        continue;
                    }
                    let sign = if ascending { -1.0 } else { 1.0 };
                    self.status = TraversalStatus::Moving;
                    return Pending::Climb {
                        target: destination,
                        intent: ActorMoveIntent::Climbing {
                            direction: (normal[0] * sign).atan2(normal[1] * sign),
                            speed: self.speed,
                        },
                    };
                }
                TraversalAction::ExitLadder {
                    carrier, target: local, ..
                } => {
                    let destination = env.carriers.pose(carrier).transform_position(&local);
                    if start.horizontal_distance_sq(&destination) <= ARRIVAL_DISTANCE.powi(2)
                        && (start.y - destination.y).abs() <= 0.25
                    {
                        self.actions.pop_front();
                        continue;
                    }
                    return Pending::Exit { target: destination };
                }
                TraversalAction::WaitForDock { carrier, dock } => {
                    if at_dock(env.carriers, carrier, dock) {
                        self.actions.pop_front();
                        continue;
                    }
                    self.status = TraversalStatus::Waiting;
                    return Pending::Wait;
                }
                TraversalAction::Board {
                    carrier,
                    target: local,
                    dock,
                } => {
                    let destination = env.carriers.pose(carrier).transform_position(&local);
                    let support = grounding_diagnostics(env.world, &start, self.physics, env.open, &[]);
                    let on_board = support.supported && support.hit.is_some_and(|hit| hit.carrier == carrier);
                    if on_board && start.horizontal_distance_sq(&destination) < ARRIVAL_DISTANCE.powi(2) {
                        self.actions.pop_front();
                        continue;
                    }
                    if !on_board && !at_dock(env.carriers, carrier, dock) {
                        self.status = TraversalStatus::Waiting;
                        return Pending::Wait;
                    }
                    return Pending::Board { target: destination };
                }
                TraversalAction::Ride { carrier, dock } => {
                    let shifted = Position::from(Vec3::from(start) + env.carriers.displacement(carrier));
                    let support = grounding_diagnostics(env.world, &shifted, self.physics, env.open, &[]);
                    if !support.supported || support.hit.is_none_or(|hit| hit.carrier != carrier) {
                        self.status = TraversalStatus::LostSupport;
                    } else if at_dock(env.carriers, carrier, dock) {
                        self.actions.pop_front();
                        continue;
                    } else {
                        self.status = TraversalStatus::Waiting;
                    }
                    return Pending::Wait;
                }
            }
        }
        Pending::Idle
    }

    // The move this tick makes toward the pending action, steered and held to
    // supported ground; `waiting` holds whatever the body is on.
    fn travel_intent(
        &mut self,
        env: &TraversalEnvironment,
        body: &ActorBody,
        pending: Pending,
        avoidance: Vec3,
        waiting: bool,
    ) -> ActorMoveIntent {
        if waiting {
            self.status = TraversalStatus::Waiting;
            return self.intent.holding_ladder();
        }
        let start = body.position;
        let Some(target) = pending.target() else {
            return match pending {
                Pending::Climb { intent, .. } => intent,
                _ => ActorMoveIntent::Idle,
            };
        };
        let offset = Vec3::from(target) - Vec3::from(start);
        let distance = offset.x.hypot(offset.z);
        let speed = self.speed.min(distance / env.delta);
        let direction = offset.x.atan2(offset.z);
        let mut movement = match pending {
            Pending::Mount { .. } => ActorMoveIntent::Climbing { direction, speed },
            Pending::Exit { .. } => ActorMoveIntent::ExitingLadder { direction, speed },
            _ => ActorMoveIntent::Moving { direction, speed },
        };
        if pending.walking() && avoidance.length_squared() > 1e-6 {
            let steered = (movement.to_horizontal_velocity() + avoidance).clamp_length_max(self.speed);
            if supported_at(env, Vec3::from(start) + steered * env.delta, self.physics) {
                movement = ActorMoveIntent::Moving {
                    direction: steered.x.atan2(steered.z),
                    speed: steered.length(),
                };
            }
        }
        if pending.walking() {
            movement = steer(movement, self.facing, distance, env.delta);
        }
        let next = Vec3::from(start) + movement.to_horizontal_velocity() * env.delta;
        let ladder_move = matches!(pending, Pending::Mount { .. } | Pending::Exit { .. });
        if !ladder_move && body.support == CharacterSupport::Ground && !supported_at(env, next, self.physics) {
            self.status = TraversalStatus::LostSupport;
            // Keep turning toward safe ground even when the current
            // heading would step off a ledge.
            return ActorMoveIntent::Moving {
                direction: movement.direction().expect("direction missing from walking intent"),
                speed: 0.0,
            };
        }
        self.status = TraversalStatus::Moving;
        movement
    }

    // The body in the way of this tick's move. A pivot has no sweep, so it
    // probes one tick of its intended travel. A walker already going around
    // a body looks further: a single step reads as clear the moment it backs
    // off, and the turn around the body would start over on the next approach.
    fn probe_blocker(
        &self,
        env: &TraversalEnvironment,
        body: &ActorBody,
        pending: Pending,
        movement: &CharacterMovementResult,
        blocking_actor: &impl Fn(Position) -> Option<BodyBlocker>,
    ) -> Option<BodyBlocker> {
        if let Some(blocker) = blocking_actor(movement.position) {
            return Some(blocker);
        }
        let probing = pending.walking()
            && self.status == TraversalStatus::Moving
            && (self.intent.speed() == Some(0.0) || self.sidestep_hand.is_some());
        if !probing {
            return None;
        }
        let reach = if self.sidestep_hand.is_some() {
            self.physics.movement_collider.diameter + SIDESTEP_LOOKAHEAD
        } else {
            self.speed * env.delta
        };
        let target = pending.target().expect("target missing from walking action");
        let offset = Vec3::from(target) - Vec3::from(body.position);
        blocking_actor((Vec3::from(body.position) + offset.clamp_length_max(reach)).into())
    }

    // A walker turns around the body in its way, keeping the hand it chose;
    // anything else stops where it stands.
    #[allow(clippy::too_many_arguments)]
    fn resolve_blocker(
        &mut self,
        env: &TraversalEnvironment,
        body: &ActorBody,
        pending: Pending,
        blocker: BodyBlocker,
        knockback_displacement: Vec3,
        step: &impl Fn(ActorMoveIntent, Vec3) -> CharacterMovementResult,
        inside_home: &impl Fn(Position) -> bool,
        blocking_actor: &impl Fn(Position) -> Option<BodyBlocker>,
    ) -> CharacterMovementResult {
        let start = body.position;
        let mut sidestep = None;
        let mut turning = None;
        if pending.walking() && self.status == TraversalStatus::Moving && body.support == CharacterSupport::Ground {
            let target = pending.target().expect("target missing from walking action");
            let distance = start.horizontal_distance_sq(&target).sqrt();
            let toward = blocker.offset.with_y(0.0).try_normalize().unwrap_or(Vec3::X);
            let preferred = self.sidestep_hand.unwrap_or(1.0);
            for hand in [preferred, -preferred] {
                let tangent = Vec3::new(toward.z, 0.0, -toward.x) * hand - toward * 0.25;
                let intent = steer(
                    ActorMoveIntent::Moving {
                        direction: tangent.x.atan2(tangent.z),
                        speed: self.speed,
                    },
                    self.facing,
                    distance,
                    env.delta,
                );
                turning.get_or_insert(intent);
                let travel = intent.to_horizontal_velocity() * env.delta;
                if !supported_at(env, Vec3::from(start) + travel, self.physics) {
                    continue;
                }
                let alternative = step(intent, knockback_displacement);
                let carried = env.carriers.displacement(alternative.carrier);
                let travelled = (Vec3::from(alternative.position) - Vec3::from(start) - carried).with_y(0.0);
                // A wall that swallows the sidestep leaves only its
                // back-off, which would repeat forever: turn the other way.
                if travelled.length_squared() < (travel * 0.5).length_squared() {
                    continue;
                }
                // Only the ground chooses the hand. A body in the way of
                // this turn clears as the actor keeps turning; trying the
                // other hand then would swing it back and forth.
                self.sidestep_hand = Some(hand);
                turning = Some(intent);
                if blocking_actor(alternative.position).is_none() && inside_home(alternative.position) {
                    sidestep = Some(alternative);
                }
                break;
            }
        }
        self.intent = turning.unwrap_or(self.intent);
        if let Some(alternative) = sidestep {
            return alternative;
        }
        // Soft separation cannot stop crossing paths or blast impulses.
        // Retry the motor without voluntary travel, and without the
        // impulse too when that alone still collides, so gravity,
        // support, landing and carrier state all describe the
        // accepted position.
        self.intent = stopped_intent(self.intent);
        let movement = step(self.intent, knockback_displacement);
        if blocking_actor(movement.position).is_some() {
            return step(self.intent, knockback_displacement * Vec3::Y);
        }
        movement
    }

    // Progress is a new closest approach to the action's target: a blocked
    // walker's sidesteps and back-offs travel without getting anywhere.
    fn record_progress(&mut self, env: &TraversalEnvironment, pending: Pending, movement: &CharacterMovementResult) {
        if self.status == TraversalStatus::Moving
            && let (Some(action), Some(destination)) = (self.actions.front().copied(), pending.destination())
        {
            let remaining = movement.position.distance_sq(&destination).sqrt();
            let closest = self
                .progress
                .filter(|(timed, _)| *timed == action)
                .map_or(f32::INFINITY, |(_, closest)| closest);
            if remaining + PROGRESS_DISTANCE < closest {
                self.progress = Some((action, remaining));
                self.stalled_secs = 0.0;
            } else {
                self.stalled_secs += env.delta;
            }
            if self.stalled_secs >= STALL_SECONDS {
                self.status = TraversalStatus::Blocked;
            }
        } else {
            self.progress = None;
            self.stalled_secs = 0.0;
        }
    }
}

fn stopped_intent(intent: ActorMoveIntent) -> ActorMoveIntent {
    match intent {
        ActorMoveIntent::Moving { direction, .. } => ActorMoveIntent::Moving { direction, speed: 0.0 },
        _ => intent.holding_ladder(),
    }
}

fn slope_clearance(physics: CharacterPhysicsConfig) -> f32 {
    // A capsule's bottom rides above the surface below its center on a
    // slope or rounded landing transition. A point probe must include that
    // clearance or it reports missing support while the motor is grounded.
    physics.movement_collider.radius() * (1.0 / CHARACTER_MAX_SLOPE.cos() - 1.0)
}

fn supported_at(env: &TraversalEnvironment, point: Vec3, physics: CharacterPhysicsConfig) -> bool {
    let rise = CHARACTER_STEP_HEIGHT + 0.05;
    env.world
        .support_surface(
            point + Vec3::Y * rise,
            rise + CHARACTER_GROUND_SNAP_DISTANCE + slope_clearance(physics),
            env.open,
        )
        .is_some_and(|hit| hit.normal.y >= CHARACTER_MAX_SLOPE.cos())
}

fn at_dock(carriers: &Carriers, carrier: CarrierId, dock: CarrierDock) -> bool {
    carriers
        .pose(carrier)
        .transform_position(&Position::default())
        .distance_sq(&carriers.pose(dock.parent).transform_position(&dock.position))
        < 0.01
        && (carriers.displacement(carrier) - carriers.displacement(dock.parent)).length_squared() < 1e-8
}

#[cfg(test)]
#[path = "tests/traversal.rs"]
mod tests;
