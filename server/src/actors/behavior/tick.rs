use bevy::prelude::*;
use rand::{Rng, rng};

use crate::{
    actors::{
        ActorInfo, ActorMap, ActorMode, ActorRoute, BeamState,
        navigation::{ActorTerritories, NavGraph, NavGraphs, NavWaypoint, PlannedRoute, WaypointKind},
    },
    config::{ActorAttackConfig, ActorKindServerConfig, ServerGameplayConfig},
    network::broadcast_to_all,
    players::PlayerMap,
};
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    map::{CarrierPose, Carriers},
    math::PHYSICS_EPSILON,
    physics::CollisionWorld,
    protocol::{
        ActorId, ActorMarker, BarrierKindId, ItemType, MapItems, PlateState, PlayerId, PlayerMarker, Position,
        SActorBeam, ServerMessage, ServerTick,
    },
};

use super::{
    controllers::{
        decide_beam_actor, decide_contact_actor, decide_contact_beam_actor, decide_stationary_actor, retarget_beam,
    },
    perception::{PlayerState, update_awareness},
};

const AI_DECISION_INTERVAL_SECS: f32 = 0.1;
pub(super) const EVADE_REPLAN_INTERVAL_SECS: f32 = 0.5;
const ROUTE_STALL_PROGRESS_DISTANCE: f32 = 0.5;
const ROUTE_STALL_TIMEOUT_SECS: f32 = 1.5;
const WAYPOINT_REACHED_DISTANCE: f32 = 0.5;
// How long the shake-loose hop owns the actor before the controller may
// rethink — roughly one cell at active speed.
const SHAKE_SECS: f32 = 0.6;
const DIRECT_ROUTE_CLEARANCE_MARGIN: f32 = 0.1;
// Cover closer than this to a threat (in grid cells) is not cover.
const COVER_MIN_THREAT_DISTANCE_CELLS: f32 = 0.75;

pub fn actors_behavior_system(
    time: Res<Time>,
    tick: Res<ServerTick>,
    players: Res<PlayerMap>,
    collision_world: Res<CollisionWorld>,
    plates: Res<PlateState>,
    gameplay_config: Res<GameplayConfig>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    nav_graphs: Res<NavGraphs>,
    territories: Res<ActorTerritories>,
    carriers: Res<Carriers>,
    map_items: Res<MapItems>,
    mut actors: ResMut<ActorMap>,
    player_query: Query<(&PlayerId, &Position), With<PlayerMarker>>,
    actor_query: Query<(&ActorId, &Position), (With<ActorMarker>, Without<PlayerMarker>)>,
) {
    let delta = time.delta_secs();
    let player_states: Vec<_> = player_query
        .iter()
        .filter_map(|(id, pos)| {
            players
                .get(id)
                .filter(|info| !actors.peaceful && info.connection.logged_in && !info.is_dead())
                .map(|info| PlayerState {
                    id: *id,
                    pos: *pos,
                    support: info.life.fall_state.support(),
                })
        })
        .collect();
    let mut rng = rng();

    for (id, pos) in &actor_query {
        let Some(info) = actors.get_mut(id) else {
            continue;
        };
        let actor_config = gameplay_config.expect_actor(&info.spawn_kind);
        let kind_config = server_gameplay_config.expect_actor(&info.spawn_kind);
        // Behaviour runs before the carriers advance, so this pose is the
        // one the actor's position was last resolved at.
        let pose = carriers.pose(info.carrier);
        let local_pos = pose.inverse_transform_position(pos);
        let previous_beam = info.beam.snapshot().map(|beam| (beam.started_tick, beam.target));
        let stalled = if actor_config.immovable {
            tick_beam_state(info, delta, kind_config, &player_states);
            false
        } else {
            tick_runtime_state(info, local_pos, delta, kind_config, &player_states)
        };
        if stalled {
            info.decision_timer = 0.0;
        }
        let decision_due = actor_config.immovable || info.decision_timer <= 0.0;
        if !decision_due && info.beam.target().is_none() && previous_beam.is_none() {
            continue;
        }

        let territory = territories.get(info.spawn_zone_index);
        update_awareness(
            info,
            *pos,
            actor_config.eye_height(),
            kind_config.vision_range,
            server_gameplay_config.actors.settings.threat_memory_secs,
            gameplay_config.player.physics(),
            &player_states,
            &collision_world,
        );

        let context = BehaviorContext {
            tick: tick.0,
            pos: local_pos,
            world_pos: *pos,
            pose,
            actor_physics: actor_config.physics(),
            actor_eye_height: actor_config.eye_height(),
            player_physics: gameplay_config.player.physics(),
            nav_graph: nav_graphs.get(info.carrier),
            territory,
            collision_world: &collision_world,
            open_barriers: &plates.open_barrier_kinds,
            kind_config,
            players_armed: map_items.contains(ItemType::SingleShotPowerUp)
                || map_items.contains(ItemType::MultiShotPowerUp)
                || map_items.contains(ItemType::MissilePack),
        };
        retarget_beam(info, &context);
        if decision_due || info.decision_timer <= 0.0 {
            if !actor_config.immovable {
                info.decision_timer += AI_DECISION_INTERVAL_SECS;
            }
            if !info.route.as_ref().is_some_and(ActorRoute::traversing_ladder) {
                if stalled {
                    shake_loose(info, &context, &mut rng);
                } else if actor_config.immovable {
                    decide_stationary_actor(info, &context);
                } else {
                    match kind_config.attack {
                        ActorAttackConfig::Contact(_) => decide_contact_actor(info, &context, &mut rng),
                        ActorAttackConfig::Beam(_) => {
                            decide_beam_actor(info, &context, &mut rng);
                        }
                        ActorAttackConfig::ContactBeam(_) => {
                            decide_contact_beam_actor(info, &context, &mut rng);
                        }
                    }
                }
            }
        }
        let beam = info.beam.snapshot();
        if beam.map(|beam| (beam.started_tick, beam.target)) != previous_beam {
            broadcast_to_all(
                &players,
                ServerMessage::ActorBeam(SActorBeam {
                    id: *id,
                    tick: tick.0,
                    beam,
                }),
            );
        }
    }
}

// `pos` is in the actor's carrier frame, like its route: a ride is not
// progress along the route, and walking against the carrier is.
pub(super) fn tick_runtime_state(
    info: &mut ActorInfo,
    pos: Position,
    delta: f32,
    kind_config: &ActorKindServerConfig,
    players: &[PlayerState],
) -> bool {
    info.decision_timer -= delta;
    for aware in &mut info.awareness {
        aware.forget_remaining_secs = (aware.forget_remaining_secs - delta).max(0.0);
    }
    info.evade_replan_remaining_secs = (info.evade_replan_remaining_secs - delta).max(0.0);
    advance_route(info, pos, WAYPOINT_REACHED_DISTANCE);
    let stalled = tick_route_stall(info, pos, delta);
    tick_beam_state(info, delta, kind_config, players);
    stalled
}

fn advance_route(info: &mut ActorInfo, pos: Position, reached_distance: f32) {
    if let Some(route) = &mut info.route {
        while let Some(next) = route.waypoints.front() {
            let reached = next.reached(&pos, reached_distance);
            if route.waypoints.len() == 1 && matches!(next.kind, WaypointKind::Climb { .. }) {
                break;
            }
            let passed = route
                .waypoints
                .get(1)
                .is_some_and(|after| waypoint_passed(&pos, next, after, reached_distance));
            if !reached && !passed {
                break;
            }
            route.waypoints.pop_front();
        }
        if route.waypoints.is_empty() {
            info.set_route(None);
        }
    }
}

// An actor nudged beyond a waypoint — by a sidestep, or by climbing past a
// ramp-top transition that sits at its own cell centre — must not walk back
// for it: with another actor behind it in a one-cell trench that is a
// permanent jam. Count the waypoint as passed when the actor is beyond it
// along the following leg and within reach of that leg's line.
fn waypoint_passed(pos: &Position, waypoint: &NavWaypoint, after: &NavWaypoint, reached_distance: f32) -> bool {
    if !waypoint.is_walk() || !after.is_walk() {
        return false;
    }
    let waypoint = &waypoint.position;
    let after = &after.position;
    let leg = Vec2::new(after.x - waypoint.x, after.z - waypoint.z);
    let leg_length = leg.length();
    if leg_length <= PHYSICS_EPSILON {
        return false;
    }
    let offset = Vec2::new(pos.x - waypoint.x, pos.z - waypoint.z);
    let along = offset.dot(leg) / leg_length;
    if along <= 0.0 {
        return false;
    }
    let lateral_sq = offset.length_squared() - along * along;
    lateral_sq <= reached_distance * reached_distance
}

// Armed only while a route exists — an intentionally idle actor is not
// stalled. The trip is surfaced to the behavior loop, which shakes loose.
fn tick_route_stall(info: &mut ActorInfo, pos: Position, delta: f32) -> bool {
    if info.route.is_none() {
        info.watchdog.reset();
        return false;
    }
    if let Some(route) = &info.route
        && let Some(next) = route.next()
        && !next.is_walk()
    {
        if route.waypoints.len() == 1 && next.reached(&pos, 0.15) {
            info.watchdog.reset();
            return false;
        }
        return info
            .watchdog
            .tick_3d(&pos, delta, ROUTE_STALL_PROGRESS_DISTANCE, ROUTE_STALL_TIMEOUT_SECS);
    }
    info.watchdog
        .tick_horizontal(&pos, delta, ROUTE_STALL_PROGRESS_DISTANCE, ROUTE_STALL_TIMEOUT_SECS)
}

// A stalled actor is wedged against something the planners cannot see —
// usually another actor. Hop to a random neighboring cell before the next
// decision: replanning from a new position is what breaks deterministic
// jam loops (two evaders re-planning the same routes into each other
// forever). A failed hop just trips the watchdog again and re-rolls.
pub(super) fn shake_loose(info: &mut ActorInfo, context: &BehaviorContext<'_>, rng: &mut impl Rng) {
    let planned =
        context
            .nav_graph
            .random_neighbor_route(context.nav_graph.ladder_links(&info.spawn_kind), &context.pos, rng);
    context.install_route(info, planned);
    info.decision_timer = SHAKE_SECS;
}

fn tick_beam_state(info: &mut ActorInfo, delta: f32, kind_config: &ActorKindServerConfig, players: &[PlayerState]) {
    let mut ended = false;
    match &mut info.beam {
        BeamState::Ready => {}
        BeamState::Cooldown { remaining_secs } => {
            *remaining_secs = (*remaining_secs - delta).max(0.0);
            if *remaining_secs <= 0.0 {
                info.beam = BeamState::Ready;
            }
        }
        BeamState::Firing {
            target, remaining_secs, ..
        } => {
            *remaining_secs -= delta;
            if let Some(player) = players.iter().find(|player| player.id == *target)
                && matches!(info.mode, ActorMode::Engage { target: engaged, .. } if engaged == *target)
            {
                info.mode = ActorMode::Engage {
                    target: *target,
                    target_pos: player.pos,
                };
            }
            ended |= *remaining_secs <= 0.0;
        }
    }
    if ended {
        let cooldown_secs = kind_config
            .attack
            .beam()
            .expect("beam attack config missing from firing actor")
            .cooldown_secs;
        info.beam = BeamState::Cooldown {
            remaining_secs: cooldown_secs,
        };
        // The controller decides what the cooldown looks like (zappers run
        // for cover, contact-beam kinds keep attacking) on this same tick.
        info.decision_timer = 0.0;
    }
}

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
    pub(super) territory: &'a crate::actors::navigation::ActorTerritory,
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

    fn install_route(&self, info: &mut ActorInfo, planned: Option<PlannedRoute>) {
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
        let margin = self.actor_physics.movement_collider.radius() + WAYPOINT_REACHED_DISTANCE;
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
    let Some(target_node) = context.nav_graph.node_for_position(&anchor) else {
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
            context.actor_physics.movement_collider.radius() + DIRECT_ROUTE_CLEARANCE_MARGIN,
            context.actor_physics.movement_collider.radius() + DIRECT_ROUTE_CLEARANCE_MARGIN,
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
