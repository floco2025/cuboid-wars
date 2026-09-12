use bevy::prelude::*;
use rand::{Rng, rng};

use crate::{
    actors::{
        ActorCharacter, ActorInfo, ActorMap, ActorMode, ActorRoute, BeamState,
        navigation::{ActorTerritories, NavGraph, NavGraphs, NavWaypoint, WALK_REACH_DISTANCE, WaypointKind},
    },
    config::{ActorAttackConfig, ActorKindServerConfig, ServerGameplayConfig},
    network::broadcast_to_all,
    players::PlayerMap,
};
use common::{
    config::GameplayConfig,
    map::Carriers,
    math::PHYSICS_EPSILON,
    physics::{CollisionWorld, grounding_diagnostics},
    protocol::{
        ActorId, ActorMarker, ItemType, MapItems, PlateState, PlayerId, PlayerMarker, Position, SActorBeam,
        ServerMessage, ServerTick,
    },
};

use super::{
    controllers::{
        decide_beam_actor, decide_contact_actor, decide_contact_beam_actor, decide_stationary_actor, retarget_beam,
    },
    perception::{PlayerState, update_awareness},
    transitions::BehaviorContext,
};

const AI_DECISION_INTERVAL_SECS: f32 = 0.1;
const ROUTE_STALL_PROGRESS_DISTANCE: f32 = 0.5;
const ROUTE_STALL_TIMEOUT_SECS: f32 = 1.5;
// How long the shake-loose hop owns the actor before the controller may
// rethink — roughly one cell at active speed.
const SHAKE_SECS: f32 = 0.6;

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
    actor_query: Query<(&ActorId, &Position, &ActorCharacter), (With<ActorMarker>, Without<PlayerMarker>)>,
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
                    support: info.life.movement.support,
                })
        })
        .collect();
    let mut rng = rng();

    for (id, pos, character) in &actor_query {
        let Some(info) = actors.get_mut(id) else {
            continue;
        };
        let character = &character.0;
        if character.flies() {
            continue;
        }
        let kind_config = server_gameplay_config.expect_actor(&info.spawn_kind);
        if !character.immovable {
            let grounding =
                grounding_diagnostics(&collision_world, pos, character.physics(), &plates.open_barriers, &[]);
            if grounding.supported
                && let Some(hit) = grounding.hit
                && hit.carrier != info.carrier
            {
                info.carrier = hit.carrier;
                info.set_route(None);
                info.decision_timer = 0.0;
            }
        }
        // Behaviour runs before the carriers advance, so this pose is the
        // one the actor's position was last resolved at.
        let pose = carriers.pose(info.carrier);
        let local_pos = pose.inverse_transform_position(pos);
        let nav_graph = nav_graphs.get(info.carrier);
        let previous_beam = info.beam.snapshot().map(|beam| (beam.started_tick, beam.target));
        let stalled = if character.immovable {
            tick_beam_state(info, delta, kind_config, &player_states);
            false
        } else {
            let stalled = tick_runtime_state(info, local_pos, delta, kind_config, &player_states);
            drop_route_onto_lost_bridge(info, nav_graph);
            if info.route.as_ref().and_then(ActorRoute::next).is_some_and(|next| {
                !collision_world.character_ground_route_clear(
                    *pos,
                    pose.transform_position(&next.position),
                    character.physics(),
                    &plates.open_barriers,
                )
            }) {
                info.set_route(None);
                info.decision_timer = 0.0;
            }
            stalled
        };
        if stalled {
            info.decision_timer = 0.0;
        }
        // An immovable actor decides every tick: with no route to follow its
        // only decision is whether to fire, and a turret that reacts the tick
        // a player is exposed is the intent.
        let decision_due = character.immovable || info.decision_timer <= 0.0;
        if !decision_due && info.beam.target().is_none() && previous_beam.is_none() {
            continue;
        }

        let home = territories.get(info.spawn_zone_index);
        let territory = home.in_frame(carriers.pose(home.carrier), pose);
        update_awareness(
            info,
            *pos,
            character.eye_height(),
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
            actor_physics: character.physics(),
            player_physics: gameplay_config.player.physics(),
            nav_graph,
            territory: &territory,
            nav_graphs: &nav_graphs,
            carriers: &carriers,
            carrier: info.carrier,
            collision_world: &collision_world,
            open_barriers: &plates.open_barriers,
            kind_config,
            players_armed: map_items.contains(ItemType::SingleShotPowerUp)
                || map_items.contains(ItemType::MultiShotPowerUp)
                || map_items.contains(ItemType::MissilePack),
        };
        retarget_beam(info, &context);
        // `retarget_beam` zeroes the timer when a burst loses every target,
        // so a decision can fall due after `decision_due` was computed.
        if decision_due || info.decision_timer <= 0.0 {
            if !character.immovable {
                info.decision_timer += AI_DECISION_INTERVAL_SECS;
            }
            if !info.route.as_ref().is_some_and(ActorRoute::traversing_ladder) {
                if stalled {
                    shake_loose(info, &context, &mut rng);
                } else if character.immovable {
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
    advance_route(info, pos);
    let stalled = tick_route_stall(info, pos, delta);
    tick_beam_state(info, delta, kind_config, players);
    stalled
}

// A bridge that lost power takes the route planned over it with it: an
// actor about to step onto it stops at the edge and decides afresh instead
// of walking off. One already on the slab is falling.
pub(super) fn drop_route_onto_lost_bridge(info: &mut ActorInfo, nav_graph: &NavGraph) {
    let next = info.route.as_ref().and_then(ActorRoute::next);
    if next.is_some_and(|next| next.is_walk() && nav_graph.position_over_unpowered_bridge(&next.position)) {
        info.set_route(None);
        info.decision_timer = 0.0;
    }
}

fn advance_route(info: &mut ActorInfo, pos: Position) {
    if let Some(route) = &mut info.route {
        while let Some(next) = route.waypoints.front() {
            let reached = next.reached(&pos);
            if route.waypoints.len() == 1 && matches!(next.kind, WaypointKind::Climb { .. }) {
                break;
            }
            let passed = route
                .waypoints
                .get(1)
                .is_some_and(|after| waypoint_passed(&pos, next, after));
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
fn waypoint_passed(pos: &Position, waypoint: &NavWaypoint, after: &NavWaypoint) -> bool {
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
    lateral_sq <= WALK_REACH_DISTANCE * WALK_REACH_DISTANCE
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
        if route.waypoints.len() == 1 && next.reached(&pos) {
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
    let planned = context.nav_graph.random_neighbor_route(
        context.nav_graph.ladder_links(&info.spawn_kind),
        &context.pos,
        |pos| !matches!(info.mode, ActorMode::Roam) || context.territory.contains_position(pos.into()),
        rng,
    );
    context.install_route(info, planned);
    info.decision_timer = SHAKE_SECS;
}

pub(super) fn tick_beam_state(
    info: &mut ActorInfo,
    delta: f32,
    kind_config: &ActorKindServerConfig,
    players: &[PlayerState],
) {
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
