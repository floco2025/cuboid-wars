use super::{
    beam::{BeamContext, find_beam_target, retarget_beam, start_beam},
    geometry::{attack_position, covered, threat_distance_sq},
    perception::{PlayerState, update_awareness},
    tick::tick_beam_state,
};
use crate::{
    actors::{
        ActorCharacter, ActorInfo, ActorMap, ActorMode, BeamState,
        navigation::air::{AirHome, AirHomes, AirSearch, FlightState, FlightTask, SearchResult},
    },
    config::{ActorAttackConfig, ServerGameplayConfig},
    map::MapConfig,
    network::broadcast_to_all,
    players::PlayerMap,
};
use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    map::{CarrierPose, Carriers},
    physics::{CollisionWorld, character_hitbox_center, character_movement_center},
    protocol::{
        ActorId, ActorMarker, ItemType, MapItems, PlateState, PlayerId, PlayerMarker, Position, SActorBeam,
        ServerMessage, ServerTick,
    },
};
use rand::{Rng, RngExt, rng};

const AIR_WORK_PER_TICK: usize = 2048;
const FLIGHT_REACH: f32 = 0.08;
const FLIGHT_RETRY_SECS: f32 = 0.5;

pub(crate) fn flying_actors_behavior_system(
    time: Res<Time>,
    tick: Res<ServerTick>,
    players: Res<PlayerMap>,
    world: Res<CollisionWorld>,
    plates: Res<PlateState>,
    gameplay: Res<GameplayConfig>,
    config: Res<ServerGameplayConfig>,
    carriers: Res<Carriers>,
    map: Res<MapConfig>,
    items: Res<MapItems>,
    mut actors: ResMut<ActorMap>,
    mut homes: ResMut<AirHomes>,
    player_query: Query<(&PlayerId, &Position), With<PlayerMarker>>,
    actor_query: Query<(&ActorId, &Position, &ActorCharacter), With<ActorMarker>>,
) {
    let delta = time.delta_secs();
    let states: Vec<_> = player_query
        .iter()
        .filter_map(|(id, pos)| {
            players
                .get(id)
                .filter(|p| !actors.peaceful && p.connection.logged_in && !p.is_dead())
                .map(|p| PlayerState {
                    id: *id,
                    pos: *pos,
                    support: p.life.movement.support,
                })
        })
        .collect();
    let armed = [
        ItemType::SingleShotPowerUp,
        ItemType::MultiShotPowerUp,
        ItemType::MissilePack,
    ]
    .into_iter()
    .any(|item| items.contains(item));
    let count = actor_query.iter().filter(|(_, _, c)| c.0.flies()).count().max(1);

    let mut rng = rng();
    for (index, (id, pos, character)) in actor_query.iter().filter(|(_, _, c)| c.0.flies()).enumerate() {
        let share =
            AIR_WORK_PER_TICK / count + usize::from((index + tick.0 as usize) % count < AIR_WORK_PER_TICK % count);
        let Some(info) = actors.get_mut(id) else {
            continue;
        };
        let kind = config.expect_actor(&info.spawn_kind);
        let physics = character.0.physics();
        let zone = &map.actor_spawn_zones[info.spawn_zone_index];
        let pose = carriers.pose(zone.carrier);
        let range = zone.roam_distance;
        let home = homes.0.entry(info.spawn_zone_index).or_insert_with(|| {
            AirHome::new(
                zone,
                map.grid(zone.carrier),
                physics,
                range,
                pose,
                &plates.open_barriers,
            )
        });
        if home.last_tick != Some(tick.0) {
            home.age += delta;
        }
        home.last_tick = Some(tick.0);
        if home.stale(pose, &plates.open_barriers, !carriers.is_static()) || home.powered != plates.powered_bridges {
            *home = AirHome::new(
                zone,
                map.grid(zone.carrier),
                physics,
                range,
                pose,
                &plates.open_barriers,
            );
        }
        home.follow_pose(pose);
        home.powered.clone_from(&plates.powered_bridges);
        let mut home_budget = share / 2;
        home.advance(&world, physics, &mut home_budget);
        let mut budget = share - share / 2 + home_budget;
        let previous_beam = info.beam.snapshot().map(|b| (b.started_tick, b.target));
        tick_beam_state(info, delta, kind, &states);
        for aware in &mut info.awareness {
            aware.forget_remaining_secs -= delta;
        }
        update_awareness(
            info,
            *pos,
            character.0.eye_height(),
            kind.vision_range,
            config.actors.settings.threat_memory_secs,
            gameplay.player.physics(),
            &states,
            &world,
        );
        let context = BeamContext {
            tick: tick.0,
            world_pos: *pos,
            kind_config: kind,
            player_physics: gameplay.player.physics(),
            collision_world: &world,
            open_barriers: &plates.open_barriers,
        };
        retarget_beam(info, &context);
        let mut flight = info.flight.take().unwrap_or_default();
        flight.retry_secs = (flight.retry_secs - delta).max(0.0);
        flight.recovery_secs = (flight.recovery_secs - delta).max(0.0);
        flight.unreachable_secs -= delta;
        if flight.unreachable_secs <= 0.0 {
            flight.unreachable.clear();
            flight.unreachable_secs = 1.0;
        }
        advance_route(&mut flight, *pos, &context, home, pose);
        if !flight.route.is_empty() && info.watchdog.tick_3d(pos, delta, 0.25, 1.5) {
            let task = flight.task;
            flight.clear();
            for _ in 0..12 {
                let direction = Vec3::new(
                    rng.random_range(-1.0..1.0),
                    rng.random_range(-1.0..1.0),
                    rng.random_range(-1.0..1.0),
                )
                .normalize_or_zero();
                let end = Vec3::from(*pos) + direction * home.spacing * 2.0;
                if (task != Some(FlightTask::Roam) || home.path_contains(Vec3::from(*pos), end, pose))
                    && world.character_flight_path_clear(*pos, end.into(), physics, &plates.open_barriers)
                {
                    flight.route.push_back(end.into());
                    flight.task = task;
                    flight.recovery_secs = 0.6;
                    break;
                }
            }
            info.watchdog.reset();
        } else if flight.route.is_empty() {
            info.watchdog.reset();
        }
        info.decision_timer -= delta;
        if info.decision_timer <= 0.0 && flight.recovery_secs <= 0.0 {
            info.decision_timer = 0.1;
            decide_flight(info, &mut flight, &context, home, pose, armed, &mut rng);
        }
        advance_search(info, &mut flight, &context, home, pose, &mut budget);
        info.flight = Some(flight);
        let beam = info.beam.snapshot();
        if beam.map(|b| (b.started_tick, b.target)) != previous_beam {
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

fn advance_search(
    info: &mut ActorInfo,
    flight: &mut FlightState,
    context: &BeamContext<'_>,
    home: &AirHome,
    pose: CarrierPose,
    budget: &mut usize,
) {
    let Some(search) = &mut flight.search else {
        return;
    };
    let physics = context.kind_config.character.physics();
    let world = context.collision_world;
    let open = context.open_barriers;
    let task = flight.task;
    let result = match task {
        Some(FlightTask::Return) => {
            search.advance_to_goal(world, physics, open, budget, |_| true, |p| home.contains(p, pose))
        }
        Some(FlightTask::Pursue(target)) => {
            let aware = info.awareness.iter().find(|a| a.id == target);
            search.advance_to_goal(
                world,
                physics,
                open,
                budget,
                |_| true,
                |p| aware.is_some_and(|a| attack_position(p.into(), a.pos, context)),
            )
        }
        Some(FlightTask::Evade) => {
            let threats: Vec<_> = info.awareness.iter().map(|a| a.pos).collect();
            search.advance_escape(
                world,
                physics,
                open,
                budget,
                |p| covered(p, &threats, context),
                |p| threat_distance_sq(p, &threats),
            )
        }
        _ => search.advance(world, physics, open, budget, |p| {
            task != Some(FlightTask::Roam) || home.contains(p, pose)
        }),
    };
    match result {
        SearchResult::Pending => {}
        SearchResult::Found(route) => {
            if task == Some(FlightTask::Evade) {
                let threats: Vec<_> = info.awareness.iter().map(|a| a.pos).collect();
                info.mode = ActorMode::Evade {
                    fleeing: route.back().is_none_or(|p| !covered(Vec3::from(*p), &threats, context)),
                };
            }
            flight.route = route;
            flight.search = None;
        }
        SearchResult::Unreachable => {
            flight.search = None;
            flight.route.clear();
            flight.retry_secs = FLIGHT_RETRY_SECS;
            if let Some(FlightTask::Pursue(id)) = task
                && let Some(aware) = info.awareness.iter().find(|a| a.id == id)
            {
                flight.unreachable.insert(id, aware.pos);
            }
        }
    }
}

fn advance_route(
    flight: &mut FlightState,
    pos: Position,
    context: &BeamContext<'_>,
    home: &AirHome,
    pose: CarrierPose,
) {
    let physics = context.kind_config.character.physics();
    while flight
        .route
        .front()
        .is_some_and(|p| p.distance_sq(&pos) <= FLIGHT_REACH * FLIGHT_REACH)
    {
        flight.route.pop_front();
    }
    if flight.route.front().is_some_and(|p| {
        !context
            .collision_world
            .character_flight_path_clear(pos, *p, physics, context.open_barriers)
    }) {
        flight.route.clear();
        flight.search = None;
    }
    if flight.task == Some(FlightTask::Roam) && flight.route.iter().any(|p| !home.contains(Vec3::from(*p), pose)) {
        flight.clear();
    }
    if !matches!(flight.task, Some(FlightTask::Roam | FlightTask::Evade)) {
        for i in (1..flight.route.len().min(8)).rev() {
            if context
                .collision_world
                .character_flight_path_clear(pos, flight.route[i], physics, context.open_barriers)
            {
                flight.route.drain(..i);
                break;
            }
        }
    }
}

fn decide_flight(
    info: &mut ActorInfo,
    flight: &mut FlightState,
    context: &BeamContext<'_>,
    home: &AirHome,
    pose: CarrierPose,
    armed: bool,
    rng: &mut impl Rng,
) {
    let beam_only = matches!(context.kind_config.attack, ActorAttackConfig::Beam(_));
    if let Some(target) = find_beam_target(info, context) {
        start_beam(info, context, target);
        if beam_only {
            info.mode = ActorMode::Engage {
                target: target.id,
                target_pos: target.pos,
            };
        }
    }
    if beam_only && matches!(info.beam, BeamState::Firing { .. }) {
        flight.clear();
        return;
    }
    let cooling = beam_only && matches!(info.beam, BeamState::Cooldown { .. });
    if !cooling
        && let Some(aware) = info
            .awareness
            .iter()
            .filter(|a| {
                flight
                    .unreachable
                    .get(&a.id)
                    .is_none_or(|p| p.distance_sq(&a.pos) > home.spacing * home.spacing)
            })
            .min_by_key(|a| !matches!(info.mode, ActorMode::Engage { target, .. } if target == a.id))
            .copied()
    {
        let target = character_hitbox_center(aware.pos, context.player_physics);
        let physics = context.kind_config.character.physics();
        let actor_center = character_movement_center(context.world_pos, physics);
        let away = (actor_center - target).try_normalize().unwrap_or(Vec3::Z);
        let separation = physics.movement_collider.radius() + context.player_physics.movement_collider.radius() + 0.05;
        let desired = target + away * separation - character_movement_center(Position::default(), physics);
        info.mode = ActorMode::Engage {
            target: aware.id,
            target_pos: aware.pos,
        };
        request_route(
            flight,
            FlightTask::Pursue(aware.id),
            context.world_pos,
            desired.into(),
            home.spacing,
        );
        return;
    }
    if matches!(info.beam, BeamState::Firing { .. }) {
        flight.clear();
        return;
    }
    if !info.awareness.is_empty() && armed {
        let current = Vec3::from(context.world_pos);
        let threats: Vec<_> = info.awareness.iter().map(|a| a.pos).collect();
        if flight.task == Some(FlightTask::Evade) && (!flight.route.is_empty() || flight.search.is_some()) {
            let destination = flight
                .route
                .back()
                .copied()
                .or_else(|| flight.search.as_ref().map(AirSearch::destination));
            if matches!(info.mode, ActorMode::Evade { fleeing: true })
                || destination.is_some_and(|p| covered(Vec3::from(p), &threats, context))
            {
                return;
            }
            flight.clear();
        }
        if covered(current, &threats, context) {
            flight.clear();
            info.mode = ActorMode::Evade { fleeing: false };
            return;
        }
        let mut fallback = None;
        let mut best = threat_distance_sq(current, &threats);
        for _ in 0..24 {
            let direction = Vec3::new(
                rng.random_range(-1.0..1.0),
                rng.random_range(-1.0..1.0),
                rng.random_range(-1.0..1.0),
            )
            .normalize_or_zero();
            let candidate = current + direction * rng.random_range(home.spacing * 2.0..home.spacing * 12.0);
            if context.collision_world.character_overlaps_solid(
                &candidate.into(),
                context.kind_config.character.physics(),
                context.open_barriers,
            ) {
                continue;
            }
            let cover = covered(candidate, &threats, context);
            let score = threat_distance_sq(candidate, &threats);
            if cover || score > best {
                best = score;
                fallback = Some((candidate, cover));
            }
            if cover {
                break;
            }
        }
        if let Some((target, cover)) = fallback {
            info.mode = ActorMode::Evade { fleeing: !cover };
            request_route(
                flight,
                FlightTask::Evade,
                context.world_pos,
                target.into(),
                home.spacing,
            );
        }
        return;
    }
    if home.contains(Vec3::from(context.world_pos), pose) {
        info.mode = ActorMode::Roam;
        if flight.task == Some(FlightTask::Roam) && (!flight.route.is_empty() || flight.search.is_some()) {
            return;
        }
        if let Some(target) = home.destination(pose, rng) {
            request_route(flight, FlightTask::Roam, context.world_pos, target, home.spacing);
        } else {
            flight.clear();
        }
    } else {
        info.mode = ActorMode::ReturnHome;
        if flight.task == Some(FlightTask::Return)
            && flight.route.back().is_some_and(|p| home.contains(Vec3::from(*p), pose))
        {
            return;
        }
        if let Some(target) = home.nearest(Vec3::from(context.world_pos), pose) {
            request_route(flight, FlightTask::Return, context.world_pos, target, home.spacing);
        } else {
            flight.clear();
        }
    }
}

fn request_route(flight: &mut FlightState, task: FlightTask, start: Position, target: Position, spacing: f32) {
    if flight.task != Some(task) {
        flight.clear();
        flight.retry_secs = 0.0;
    }
    if flight.retry_secs > 0.0 {
        return;
    }
    flight.task = Some(task);
    if let Some(search) = &flight.search {
        if search.destination().distance_sq(&target) <= spacing * spacing * 16.0 {
            return;
        }
        flight.search = None;
    }
    if let Some(destination) = flight.route.back() {
        if destination.distance_sq(&target) <= spacing * spacing {
            return;
        }
        flight.route.clear();
    }
    flight.search = Some(AirSearch::new(start, target, spacing));
}

#[cfg(test)]
#[path = "tests/flight.rs"]
mod tests;
