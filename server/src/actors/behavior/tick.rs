use bevy::prelude::*;
use rand::{Rng, rng};

use crate::{
    actors::{
        ActorInfo, ActorMap, ActorMode, ActorRoute, BeamState,
        navigation::{ActorTerritories, NavGraph},
    },
    config::{ActorAttackConfig, ActorKindServerConfig, ServerGameplayConfig},
    network::broadcast_to_all,
    players::PlayerMap,
};
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    constants::GRID_CELL_SIZE,
    physics::CollisionWorld,
    protocol::{ActorId, ActorMarker, PlayerId, PlayerMarker, Position, SActorBeam, ServerMessage},
};

use super::{
    controllers::{BeamStarted, decide_beam_actor, decide_contact_actor, decide_contact_beam_actor},
    perception::{PlayerState, update_awareness},
};

const AI_DECISION_INTERVAL_SECS: f32 = 0.1;
const EVADE_REPLAN_INTERVAL_SECS: f32 = 0.5;
const ROUTE_STALL_PROGRESS_DISTANCE: f32 = 0.5;
const ROUTE_STALL_TIMEOUT_SECS: f32 = 1.5;
const WAYPOINT_REACHED_DISTANCE: f32 = 0.5;
const DIRECT_ROUTE_CLEARANCE_MARGIN: f32 = 0.1;
const COVER_MIN_THREAT_DISTANCE: f32 = GRID_CELL_SIZE * 0.75;

pub fn actors_behavior_system(
    time: Res<Time>,
    players: Res<PlayerMap>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    nav_graph: Res<NavGraph>,
    territories: Res<ActorTerritories>,
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
                .filter(|info| info.logged_in && !info.is_dead())
                .map(|info| PlayerState {
                    id: *id,
                    pos: *pos,
                    support: info.fall_state.support(),
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
        tick_runtime_state(info, *pos, delta, kind_config, &player_states);
        if info.decision_timer > 0.0 {
            continue;
        }
        info.decision_timer += AI_DECISION_INTERVAL_SECS;

        let territory = territories.get(info.spawn_zone_index);
        update_awareness(
            info,
            *pos,
            actor_config.eye_height(),
            kind_config.vision_range,
            server_gameplay_config.actor_settings.threat_memory_secs,
            gameplay_config.player.physics(),
            &player_states,
            &collision_world,
        );

        let context = BehaviorContext {
            pos: *pos,
            actor_physics: actor_config.physics(),
            actor_eye_height: actor_config.eye_height(),
            player_physics: gameplay_config.player.physics(),
            nav_graph: &nav_graph,
            territory,
            collision_world: &collision_world,
            kind_config,
        };
        let beam_started = match kind_config.combat.attack {
            ActorAttackConfig::Contact(_) => {
                decide_contact_actor(info, &context, &mut rng);
                None
            }
            ActorAttackConfig::Beam(_) => decide_beam_actor(info, &context, &mut rng),
            ActorAttackConfig::ContactBeam(_) => decide_contact_beam_actor(info, &context, &mut rng),
        };
        if let Some(BeamStarted { target, duration_secs }) = beam_started {
            broadcast_to_all(
                &players,
                ServerMessage::ActorBeam(SActorBeam {
                    id: *id,
                    target,
                    duration_secs,
                }),
            );
        }
    }
}

pub(super) fn tick_runtime_state(
    info: &mut ActorInfo,
    pos: Position,
    delta: f32,
    kind_config: &ActorKindServerConfig,
    players: &[PlayerState],
) {
    info.decision_timer -= delta;
    for aware in &mut info.awareness {
        aware.forget_remaining_secs = (aware.forget_remaining_secs - delta).max(0.0);
    }
    info.evade_replan_remaining_secs = (info.evade_replan_remaining_secs - delta).max(0.0);
    advance_route(info, pos, WAYPOINT_REACHED_DISTANCE);
    tick_route_stall(info, pos, delta);
    tick_beam_state(info, delta, kind_config, players);
}

fn advance_route(info: &mut ActorInfo, pos: Position, reached_distance: f32) {
    let reached_sq = reached_distance * reached_distance;
    if let Some(route) = &mut info.route {
        while route
            .waypoints
            .front()
            .is_some_and(|next| pos.horizontal_distance_sq(next) <= reached_sq)
        {
            route.waypoints.pop_front();
        }
        if route.waypoints.is_empty() {
            info.set_route(None);
        }
    }
}

fn tick_route_stall(info: &mut ActorInfo, pos: Position, delta: f32) {
    if info.route.is_none() {
        info.route_stall_anchor = None;
        info.route_stall_secs = 0.0;
        return;
    }
    let Some(anchor) = info.route_stall_anchor else {
        info.route_stall_anchor = Some(pos);
        return;
    };
    if anchor.horizontal_distance_sq(&pos) >= ROUTE_STALL_PROGRESS_DISTANCE * ROUTE_STALL_PROGRESS_DISTANCE {
        info.route_stall_anchor = Some(pos);
        info.route_stall_secs = 0.0;
        return;
    }
    info.route_stall_secs += delta;
    if info.route_stall_secs >= ROUTE_STALL_TIMEOUT_SECS {
        info.set_route(None);
        info.decision_timer = 0.0;
    }
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
        BeamState::Firing { target, remaining_secs } => {
            *remaining_secs -= delta;
            match players.iter().find(|player| player.id == *target) {
                Some(player) => {
                    if matches!(info.mode, ActorMode::Engage { target: engaged, .. } if engaged == *target) {
                        info.mode = ActorMode::Engage {
                            target: *target,
                            target_pos: player.pos,
                        };
                    }
                }
                None => ended = true,
            }
            ended |= *remaining_secs <= 0.0;
        }
    }
    if ended {
        let cooldown_secs = kind_config
            .combat
            .attack
            .beam()
            .expect("firing state belongs to a beam actor")
            .cooldown_secs;
        info.beam = BeamState::Cooldown {
            remaining_secs: cooldown_secs,
        };
        // The controller decides what the cooldown looks like (zappers run
        // for cover, contact-beam kinds keep attacking) on this same tick.
        info.decision_timer = 0.0;
    }
}

pub(super) struct BehaviorContext<'a> {
    pub(super) pos: Position,
    pub(super) actor_physics: CharacterPhysicsConfig,
    pub(super) actor_eye_height: f32,
    pub(super) player_physics: CharacterPhysicsConfig,
    pub(super) nav_graph: &'a NavGraph,
    pub(super) territory: &'a crate::actors::navigation::ActorTerritory,
    pub(super) collision_world: &'a CollisionWorld,
    pub(super) kind_config: &'a ActorKindServerConfig,
}

impl BehaviorContext<'_> {
    pub(super) fn stable_cover(&self, pos: &Position, threats: &[Position]) -> bool {
        if threats
            .iter()
            .any(|threat| pos.horizontal_distance_sq(threat) < COVER_MIN_THREAT_DISTANCE * COVER_MIN_THREAT_DISTANCE)
        {
            return false;
        }
        let margin =
            self.actor_physics.collider.width.max(self.actor_physics.collider.depth) / 2.0 + WAYPOINT_REACHED_DISTANCE;
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
            let target = Vec3::new(threat.x, self.player_physics.collider_center_y(threat.y), threat.z);
            samples.iter().all(|sample| {
                let eye = Vec3::new(sample.x, sample.y + self.actor_eye_height, sample.z);
                !self.collision_world.line_of_sight_clear(eye, target)
            })
        })
    }
}

pub(super) fn enter_evade(info: &mut ActorInfo, context: &BehaviorContext<'_>) {
    let threats: Vec<_> = info.awareness.iter().map(|aware| aware.pos).collect();
    let continuing_evade = matches!(info.mode, ActorMode::Evade);
    info.mode = ActorMode::Evade;

    if continuing_evade
        && info
            .route
            .as_ref()
            .is_some_and(|route| context.stable_cover(&route.destination, &threats))
    {
        return;
    }

    info.set_route(None);
    if context.stable_cover(&context.pos, &threats) {
        info.evade_replan_remaining_secs = EVADE_REPLAN_INTERVAL_SECS;
        return;
    }
    if continuing_evade && info.evade_replan_remaining_secs > 0.0 {
        return;
    }
    let planned = context.nav_graph.safe_cover_route(&context.pos, &threats, |candidate| {
        context.stable_cover(candidate, &threats)
    });
    info.set_route(planned.and_then(ActorRoute::new));
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
        let route = context.nav_graph.roam_route(&context.pos, context.territory, rng);
        info.set_route(route.and_then(ActorRoute::new));
    } else {
        let continuing_return = matches!(info.mode, ActorMode::ReturnHome) && info.route.is_some();
        info.mode = ActorMode::ReturnHome;
        info.evade_replan_remaining_secs = 0.0;
        if continuing_return {
            return;
        }
        let route = context.nav_graph.return_route(&context.pos, context.territory);
        info.set_route(route.and_then(ActorRoute::new));
    }
}

pub(super) fn keep_or_install_engagement_route(
    info: &mut ActorInfo,
    context: &BehaviorContext<'_>,
    target: PlayerId,
    target_pos: Position,
) -> bool {
    let Some(target_node) = context.nav_graph.node_for_position(&target_pos) else {
        return false;
    };
    if matches!(info.mode, ActorMode::Engage { target: route_target, .. } if route_target == target)
        && let Some(route) = &mut info.route
        && route.destination_node == target_node
    {
        let final_leg_start = route.waypoints.iter().rev().nth(1).copied().unwrap_or(context.pos);
        if context.nav_graph.engagement_retarget_is_valid(
            &final_leg_start,
            &target_pos,
            context.actor_physics.collider.width / 2.0 + DIRECT_ROUTE_CLEARANCE_MARGIN,
            context.actor_physics.collider.depth / 2.0 + DIRECT_ROUTE_CLEARANCE_MARGIN,
        ) {
            route.retarget(target_pos);
            info.mode = ActorMode::Engage { target, target_pos };
            info.evade_replan_remaining_secs = 0.0;
            return true;
        }
    }
    let Some(planned) = context.nav_graph.engagement_route(
        &context.pos,
        &target_pos,
        context.actor_physics.collider.width / 2.0,
        context.actor_physics.collider.depth / 2.0,
    ) else {
        return false;
    };
    info.mode = ActorMode::Engage { target, target_pos };
    info.evade_replan_remaining_secs = 0.0;
    info.set_route(ActorRoute::new(planned));
    true
}
