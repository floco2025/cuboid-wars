use bevy::{ecs::system::SystemParam, prelude::*};
use std::collections::VecDeque;

use super::steering::{
    closest_point_on_segment, lead_point, pick_clear_direction, steer_clear, sweep_clear, target_velocity_estimate,
    weave_direction,
};
use crate::{
    actors::ActorMap,
    config::{MissilesServerConfig, ServerGameplayConfig},
    missiles::{AirGraph, MissileInfo, MissileMap, MissileVelocity},
    players::PlayerMap,
};
use common::{
    config::GameplayConfig,
    constants::MISSILE_RADIUS,
    map::Carriers,
    physics::{CollisionWorld, character_center},
    protocol::{
        ActorMarker, BarrierKindId, FaceYaw, HomingTarget, MapSettings, MissileId, MissileMarker, PlateState,
        PlayerMarker, Position,
    },
};

// Net-displacement watchdog distance, 3D (missiles fly): a missile that
// doesn't displace this far within `stall_secs` is orbiting or wedged and
// self-detonates.
const MISSILE_STALL_PROGRESS_DISTANCE: f32 = 1.0;
// How long a picked avoidance direction is flown before re-deciding, so the
// missile doesn't dither between candidates every tick.
const MISSILE_AVOID_COMMIT_SECS: f32 = 0.25;
// How far ahead (in seconds of travel) a candidate direction must be clear.
const MISSILE_AVOID_LOOKAHEAD_SECS: f32 = 0.6;
// Waypoint following along the air-graph route.
const MISSILE_TURN_LOOKAHEAD_SECS: f32 = 0.35;
const MISSILE_PATH_RETRY_SECS: f32 = 0.5;
// How far (in grid cells) the target may drift before the route is replanned.
const MISSILE_PATH_TARGET_MOVED_CELLS: f32 = 1.0;

type MissileGuidanceQuery<'w, 's> =
    Query<'w, 's, (&'static MissileId, &'static Position, &'static mut MissileVelocity), With<MissileMarker>>;

type TargetPositionQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Position, &'static FaceYaw),
    (Or<(With<PlayerMarker>, With<ActorMarker>)>, Without<MissileMarker>),
>;

#[derive(SystemParam)]
pub struct MissileGuidanceParams<'w, 's> {
    missiles: ResMut<'w, MissileMap>,
    missile_query: MissileGuidanceQuery<'w, 's>,
    players: Res<'w, PlayerMap>,
    actors: Res<'w, ActorMap>,
    target_data: TargetPositionQuery<'w, 's>,
    air_graph: Res<'w, AirGraph>,
    carriers: Res<'w, Carriers>,
    collision_world: Res<'w, CollisionWorld>,
    plates: Res<'w, PlateState>,
    map_settings: Res<'w, MapSettings>,
    gameplay_config: Res<'w, GameplayConfig>,
    server_gameplay_config: Res<'w, ServerGameplayConfig>,
}

pub fn missiles_guidance_system(time: Res<Time>, mut params: MissileGuidanceParams) {
    let delta = time.delta_secs();
    let config = params.server_gameplay_config.weapons.missiles;
    let missile_speed = params.map_settings.movement.missile_speed;

    for (id, pos, mut velocity) in &mut params.missile_query {
        let Some(info) = params.missiles.get_mut(id) else {
            continue;
        };

        info.lifetime_timer -= delta;
        if info.lifetime_timer <= 0.0 {
            info.detonate_at = Some(*pos);
            continue;
        }

        let resolved = info.target.and_then(|target| {
            resolve_target(
                target,
                &params.players,
                &params.actors,
                &params.target_data,
                &params.gameplay_config,
            )
        });
        if resolved.is_none() {
            // Dead/vanished target: fly straight on the last heading and let
            // contact or lifetime finish the flight.
            info.target = None;
        }

        if let Some((_, target_center)) = resolved {
            let origin = Vec3::from(*pos);

            velocity.0 = guided_velocity(
                info,
                &config,
                &params.air_graph,
                &params.carriers,
                &params.collision_world,
                &params.plates.open_barrier_kinds,
                origin,
                target_center,
                velocity.0,
                missile_speed,
                delta,
            );

            // Proximity fuse on this tick's travel SEGMENT, not a point
            // sample — at 0.4 m per tick a point check can alias straight
            // past the closest approach. Detonating at the closest point
            // keeps the target inside the full-damage blast core.
            let fuse = config.proximity_fuse_distance;
            let closest = closest_point_on_segment(origin, velocity.0 * delta, target_center);
            if closest.distance_squared(target_center) <= fuse * fuse
                && sweep_clear(
                    &params.collision_world,
                    &params.plates.open_barrier_kinds,
                    origin,
                    closest - origin,
                    MISSILE_RADIUS,
                )
            {
                info.detonate_at = Some(closest.into());
                continue;
            }
        }

        if info
            .watchdog
            .tick_3d(pos, delta, MISSILE_STALL_PROGRESS_DISTANCE, config.stall_secs)
        {
            info.detonate_at = Some(*pos);
        }
    }
}

fn guided_velocity(
    info: &mut MissileInfo,
    config: &MissilesServerConfig,
    air_graph: &AirGraph,
    carriers: &Carriers,
    world: &CollisionWorld,
    open_kinds: &[BarrierKindId],
    origin: Vec3,
    target: Vec3,
    velocity: Vec3,
    speed: f32,
    delta: f32,
) -> Vec3 {
    let target_velocity = target_velocity_estimate(info.last_target_center, target, delta);
    info.last_target_center = Some(target);
    let aim = lead_point(origin, target, target_velocity, speed);
    let objective = if sweep_clear(world, open_kinds, origin, target - origin, MISSILE_RADIUS) {
        info.path.clear();
        info.path_target = None;
        info.path_retry_timer = 0.0;
        let aim = if sweep_clear(world, open_kinds, origin, aim - origin, MISSILE_RADIUS) {
            aim
        } else {
            target
        };
        let woven = homing_objective(info, config, origin, aim);
        if sweep_clear(world, open_kinds, origin, woven, MISSILE_RADIUS) {
            woven
        } else {
            aim - origin
        }
    } else if let Some(direction) = route_objective(
        info,
        air_graph,
        carriers,
        world,
        open_kinds,
        origin,
        target,
        MISSILE_RADIUS,
        delta,
    ) {
        direction
    } else {
        dodge_objective(info, world, open_kinds, origin, target, speed, delta)
    };
    let lookahead = ((origin.distance(target) - config.proximity_fuse_distance).max(0.0) / speed.max(f32::EPSILON))
        .clamp(delta, MISSILE_TURN_LOOKAHEAD_SECS.max(delta));
    steer_clear(
        world,
        open_kinds,
        origin,
        velocity,
        objective,
        config.turn_radius,
        delta,
        lookahead,
        MISSILE_RADIUS,
    )
}

// Clear sight line: fly at the (lead-pursuit) aim point with the cosmetic
// weave layered on.
fn homing_objective(info: &mut MissileInfo, config: &MissilesServerConfig, origin: Vec3, aim_point: Vec3) -> Vec3 {
    info.avoid_dir = None;
    let elapsed = config.lifetime_secs - info.lifetime_timer;
    weave_direction(aim_point - origin, elapsed, info.weave_phase, config.weave_strength)
}

// No line of sight: route through the 3D airspace graph. `None` when the
// graph has no route (sealed target, off-graph edge case).
#[expect(
    clippy::too_many_arguments,
    reason = "route following reads world, graph, and per-missile state"
)]
fn route_objective(
    info: &mut MissileInfo,
    air_graph: &AirGraph,
    carriers: &Carriers,
    collision_world: &CollisionWorld,
    open_kinds: &[BarrierKindId],
    origin: Vec3,
    target_center: Vec3,
    radius: f32,
    delta: f32,
) -> Option<Vec3> {
    info.path_retry_timer -= delta;
    let moved_threshold = MISSILE_PATH_TARGET_MOVED_CELLS * air_graph.cell_size();
    let target_moved = info
        .path_target
        .is_some_and(|prev| prev.distance_squared(target_center) > moved_threshold * moved_threshold);
    if target_moved
        || info.path_retry_timer <= 0.0
        || !route_clear(&info.path, origin, collision_world, open_kinds, radius)
    {
        match air_graph.path(carriers, collision_world, open_kinds, origin, target_center, radius) {
            Some(path) => {
                info.path = path;
                info.path_target = Some(target_center);
            }
            None => {
                info.path.clear();
                info.path_target = Some(target_center);
            }
        }
        info.path_retry_timer = MISSILE_PATH_RETRY_SECS;
    }
    advance_waypoints(&mut info.path, origin, collision_world, open_kinds, radius);
    let waypoint = info.path.front()?;
    info.avoid_dir = None;
    Some(*waypoint - origin)
}

// Last resort with no air route: local dodge fan. A committed direction is
// flown while its lookahead sweep stays clear; otherwise re-pick. Nothing
// clear: press at the target anyway — the stall watchdog detonates a wedged
// missile.
fn dodge_objective(
    info: &mut MissileInfo,
    collision_world: &CollisionWorld,
    open_kinds: &[BarrierKindId],
    origin: Vec3,
    aim_point: Vec3,
    missile_speed: f32,
    delta: f32,
) -> Vec3 {
    info.avoid_timer -= delta;
    let lookahead = missile_speed * MISSILE_AVOID_LOOKAHEAD_SECS;
    let desired = (aim_point - origin).normalize_or_zero();
    let committed = info.avoid_dir.filter(|dir| {
        info.avoid_timer > 0.0 && sweep_clear(collision_world, open_kinds, origin, *dir * lookahead, MISSILE_RADIUS)
    });
    let chosen = committed.or_else(|| {
        let picked = pick_clear_direction(collision_world, open_kinds, origin, desired, lookahead, MISSILE_RADIUS);
        info.avoid_dir = picked;
        info.avoid_timer = MISSILE_AVOID_COMMIT_SECS;
        picked
    });
    chosen.unwrap_or(desired)
}

fn resolve_target(
    target: HomingTarget,
    players: &PlayerMap,
    actors: &ActorMap,
    target_data: &TargetPositionQuery,
    gameplay_config: &GameplayConfig,
) -> Option<(Position, Vec3)> {
    match target {
        HomingTarget::Player(id) => {
            let info = players.get(&id)?;
            if info.is_dead() {
                return None;
            }
            let (pos, _) = target_data.get(info.entity()?).ok()?;
            Some((*pos, character_center(*pos, gameplay_config.player.physics())))
        }
        HomingTarget::Actor(id) => {
            let info = actors.get(&id)?;
            let (pos, _) = target_data.get(info.entity).ok()?;
            Some((
                *pos,
                character_center(*pos, gameplay_config.expect_actor(&info.spawn_kind).physics()),
            ))
        }
    }
}

fn route_clear(
    path: &VecDeque<Vec3>,
    origin: Vec3,
    world: &CollisionWorld,
    open_kinds: &[BarrierKindId],
    radius: f32,
) -> bool {
    let mut previous = origin;
    path.iter().all(|point| {
        let clear = sweep_clear(world, open_kinds, previous, *point - previous, radius);
        previous = *point;
        clear
    })
}

fn advance_waypoints(
    path: &mut VecDeque<Vec3>,
    origin: Vec3,
    world: &CollisionWorld,
    open_kinds: &[BarrierKindId],
    radius: f32,
) {
    // Near a corner is not past it: skip only waypoints with a clear shortcut.
    if let Some(index) = path
        .iter()
        .rposition(|point| sweep_clear(world, open_kinds, origin, *point - origin, radius))
    {
        path.drain(..index);
    } else {
        path.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        map::{CellGrid, EdgeGrid, LevelGrid, MapConfig},
        test_geometry::{FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_HEIGHT, WALL_THICKNESS, geometry},
    };
    use common::{
        config::MissilesConfig,
        constants::TICK_SECS,
        protocol::{BarrierKindTable, Carrier, CarrierId, Floor, MapLayout, PlayerId, Wall},
    };

    fn info() -> MissileInfo {
        MissileInfo::new(Entity::PLACEHOLDER, PlayerId(0), None, Vec3::Z, 0.0, 10.0)
    }

    fn config() -> MissilesServerConfig {
        MissilesServerConfig {
            gameplay: MissilesConfig {
                lock_range: 100.0,
                lock_assist_radius: 1.2,
                require_lock: true,
                max_missiles: 3,
            },
            turn_radius: 1.7,
            lifetime_secs: 10.0,
            launch_spread_degrees: 45.0,
            weave_strength: 0.1,
            proximity_fuse_distance: 1.0,
            stall_secs: 2.0,
            missiles_per_pack: 1,
        }
    }

    fn map(cols: i32, rows: i32, levels: usize) -> MapConfig {
        MapConfig::for_grid(
            (0..levels)
                .map(|_| LevelGrid {
                    cells: CellGrid::new(cols, rows),
                    edges: EdgeGrid::new(cols, rows),
                    barrier_edges: EdgeGrid::new(cols, rows),
                })
                .collect(),
            geometry(cols, rows),
        )
    }

    fn wall(x1: f32, z1: f32, x2: f32, z2: f32) -> Wall {
        Wall {
            x1,
            z1,
            x2,
            z2,
            width: WALL_THICKNESS,
            y: 0.0,
            height: WALL_HEIGHT,
            level: 0,
            carrier: CarrierId::WORLD,
        }
    }

    fn world(layout: &MapLayout) -> CollisionWorld {
        CollisionWorld::from_map_layout(layout, &BarrierKindTable::default())
    }

    #[test]
    fn a_nearby_corner_is_not_skipped_when_the_next_leg_is_blocked() {
        let world = world(&MapLayout {
            walls: vec![wall(1.0, -1.0, 1.0, 1.6)],
            ..default()
        });
        let corner = Vec3::new(0.0, 2.0, 1.0);
        let mut path = VecDeque::from([corner, Vec3::new(2.0, 2.0, 2.0)]);
        advance_waypoints(&mut path, Vec3::new(0.0, 2.0, 0.0), &world, &[], MISSILE_RADIUS);
        assert_eq!(path.len(), 2);
        assert_eq!(path.front(), Some(&corner));
    }

    #[test]
    fn a_wall_moving_across_a_cached_route_triggers_an_immediate_replan() {
        let graph = AirGraph::new(&map(4, 4, 2));
        let layout = MapLayout {
            walls: vec![Wall {
                carrier: CarrierId(1),
                ..wall(0.0, -5.0, 0.0, 5.0)
            }],
            carriers: vec![Carrier {
                parent: CarrierId::WORLD,
                level: 0,
                levels: 0,
                from: Position::from(Vec3::X * 20.0),
                to: Position::default(),
                travel_ticks: 60,
                pause_ticks: 30,
                phase_ticks: 0,
            }],
            ..default()
        };
        let mut carriers = Carriers::from_layout(&layout);
        let mut world = world(&layout);
        let origin = Vec3::new(-3.0, 1.5, 0.0);
        let target = Vec3::new(3.0, 1.5, 0.0);
        let mut info = info();
        info.path = graph
            .path(&carriers, &world, &[], origin, target, MISSILE_RADIUS)
            .expect("initial route missing");
        info.path_target = Some(target);
        info.path_retry_timer = 0.4;
        carriers.advance(60);
        world.set_carrier_poses(&carriers);
        assert!(!route_clear(&info.path, origin, &world, &[], MISSILE_RADIUS));
        assert!(
            route_objective(
                &mut info,
                &graph,
                &carriers,
                &world,
                &[],
                origin,
                target,
                MISSILE_RADIUS,
                TICK_SECS
            )
            .is_some()
        );
        assert!(route_clear(&info.path, origin, &world, &[], MISSILE_RADIUS));
        assert_eq!(info.path_retry_timer, MISSILE_PATH_RETRY_SECS);
    }

    #[test]
    fn failed_routes_obey_the_retry_timer() {
        let graph = AirGraph::new(&map(2, 1, 1));
        let world = world(&MapLayout::default());
        let mut info = info();
        let target = Vec3::X;
        info.path_target = Some(target);
        info.path_retry_timer = 0.4;
        assert!(
            route_objective(
                &mut info,
                &graph,
                &Carriers::default(),
                &world,
                &[],
                Vec3::ZERO,
                target,
                MISSILE_RADIUS,
                TICK_SECS
            )
            .is_none()
        );
        assert!(info.path_retry_timer < 0.4);
        assert!(info.path_retry_timer > 0.3);
    }

    #[test]
    fn lead_pursuit_does_not_aim_through_a_wall_beside_a_visible_target() {
        let graph = AirGraph::new(&map(8, 8, 2));
        let world = world(&MapLayout {
            walls: vec![wall(2.0, 1.0, 2.0, 12.0)],
            ..default()
        });
        let origin = Vec3::new(0.0, 2.0, 0.0);
        let target = Vec3::new(0.0, 2.0, 10.0);
        let mut info = info();
        info.last_target_center = Some(target - Vec3::X * (10.0 * TICK_SECS));
        let config = MissilesServerConfig {
            weave_strength: 0.0,
            ..config()
        };
        let velocity = guided_velocity(
            &mut info,
            &config,
            &graph,
            &Carriers::default(),
            &world,
            &[],
            origin,
            target,
            Vec3::Z * 16.0,
            16.0,
            TICK_SECS,
        );
        assert!(velocity.abs_diff_eq(Vec3::Z * 16.0, 1e-4));
    }

    #[test]
    fn missiles_reach_targets_inside_a_moving_room_without_clipping_its_shell() {
        let mut map = map(12, 12, 3);
        let room_size = geometry(3, 3);
        let mut room_grid = self::map(3, 3, 2).grids.remove(0);
        room_grid.carrier = CarrierId(1);
        map.grids.push(room_grid);
        let graph = AirGraph::new(&map);
        let half = room_size.width() / 2.0;
        let door = room_size.cell_size() / 2.0;
        let layout = MapLayout {
            walls: [
                wall(-half, -half, half, -half),
                wall(-half, half, half, half),
                wall(half, -half, half, half),
                wall(-half, -half, -half, -door),
                wall(-half, door, -half, half),
                wall(0.0, -half, 0.0, door),
            ]
            .into_iter()
            .map(|wall| Wall {
                carrier: CarrierId(1),
                ..wall
            })
            .collect(),
            floors: [0.0, LEVEL_HEIGHT]
                .into_iter()
                .map(|y| Floor {
                    x1: -half,
                    z1: -half,
                    x2: half,
                    z2: half,
                    y,
                    thickness: FLOOR_THICKNESS,
                    level: 0,
                    carrier: CarrierId(1),
                })
                .collect(),
            carriers: vec![Carrier {
                parent: CarrierId::WORLD,
                level: 0,
                levels: 1,
                from: Position::from(Vec3::new(-0.75, 0.4, -0.45)),
                to: Position::from(Vec3::new(0.75, 2.0, 0.65)),
                travel_ticks: 180,
                pause_ticks: 30,
                phase_ticks: 0,
            }],
            ..default()
        };
        let config = config();
        for first_tick in [0, 90, 210] {
            for side in [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z] {
                let mut carriers = Carriers::from_layout(&layout);
                carriers.advance(first_tick);
                let mut world = world(&layout);
                let local_target = Vec3::new(2.0, 1.1, 1.0);
                let target = carriers.pose(CarrierId(1)).transform_point(local_target);
                let mut origin = target + side * 10.0;
                let mut velocity = -side * 16.0;
                let mut info = info();
                let mut reached = false;
                for tick in first_tick..first_tick + 300 {
                    carriers.advance(tick);
                    world.set_carrier_poses(&carriers);
                    let target = carriers.pose(CarrierId(1)).transform_point(local_target);
                    info.lifetime_timer -= TICK_SECS;
                    velocity = guided_velocity(
                        &mut info,
                        &config,
                        &graph,
                        &carriers,
                        &world,
                        &[],
                        origin,
                        target,
                        velocity,
                        16.0,
                        TICK_SECS,
                    );
                    let closest = closest_point_on_segment(origin, velocity * TICK_SECS, target);
                    if closest.distance(target) <= config.proximity_fuse_distance {
                        assert!(sweep_clear(&world, &[], origin, closest - origin, MISSILE_RADIUS));
                        reached = true;
                        break;
                    }
                    assert!(
                        sweep_clear(&world, &[], origin, velocity * TICK_SECS, MISSILE_RADIUS),
                        "first tick {first_tick}, side {side}, tick {tick}: collision at {origin}, velocity {velocity}, route {:?}",
                        info.path
                    );
                    origin += velocity * TICK_SECS;
                    assert!(
                        !info.watchdog.tick_3d(
                            &Position::from(origin),
                            TICK_SECS,
                            MISSILE_STALL_PROGRESS_DISTANCE,
                            config.stall_secs
                        ),
                        "missile stalled at {origin}"
                    );
                }
                assert!(
                    reached,
                    "first tick {first_tick}, side {side}: failed to reach target, ended at {origin}"
                );
            }
        }
    }
}
