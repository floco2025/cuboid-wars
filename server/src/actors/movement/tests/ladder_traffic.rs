use std::collections::VecDeque;

use super::*;
use crate::{actors::navigation::WaypointKind, config::ServerGameplayConfig};
use common::{
    constants::{LADDER_RAIL_INSET, LADDER_STANDOFF_CLEARANCE, TICK_SECS},
    physics::{blocking_character_move_plan, character_paths_intersect},
    protocol::{BarrierKindTable, Ladder},
};

#[test]
fn mines_approaching_opposite_ladder_sides_both_reach_the_landing() {
    for transpose in [false, true] {
        ladder_traffic(transpose, Traffic::OppositeApproaches);
    }
}

#[test]
fn side_by_side_mines_descend_without_jamming() {
    for transpose in [false, true] {
        ladder_traffic(transpose, Traffic::SideBySideDescent);
    }
}

#[derive(Clone, Copy)]
enum Traffic {
    OppositeApproaches,
    SideBySideDescent,
}

fn ladder_traffic(transpose: bool, traffic: Traffic) {
    let orient = |mut pos: Position| {
        if transpose {
            (pos.x, pos.z) = (pos.z, pos.x);
        }
        pos
    };
    let physics = ServerGameplayConfig::load_default()
        .expect("default gameplay missing")
        .gameplay_config()
        .expect_actor("mine")
        .physics();
    let mount = orient(Position {
        x: 0.0,
        y: 0.0,
        z: -(LADDER_RAIL_INSET + physics.movement_collider.radius + LADDER_STANDOFF_CLEARANCE),
    });
    let world = CollisionWorld::from_map_layout(
        &MapLayout {
            floors: vec![
                floor(),
                Floor {
                    x1: if transpose { 0.0 } else { -4.0 },
                    z1: if transpose { -4.0 } else { 0.0 },
                    y: 4.4,
                    level: 1,
                    ..floor()
                },
            ],
            ladders: vec![Ladder {
                x1: if transpose { 0.0 } else { -0.6 },
                x2: if transpose { 0.0 } else { 0.6 },
                z1: if transpose { -0.6 } else { 0.0 },
                z2: if transpose { 0.6 } else { 0.0 },
                nx: if transpose { -1.0 } else { 0.0 },
                nz: if transpose { 0.0 } else { -1.0 },
                y: 0.0,
                height: 4.4,
                level: 0,
                levels: 1,
                carrier: CarrierId::WORLD,
            }],
            ..Default::default()
        },
        &BarrierKindTable::default(),
    );

    let descending = matches!(traffic, Traffic::SideBySideDescent);
    let landing_y = if descending { 0.0 } else { 4.4 };
    let exit_z = if descending { -2.0 } else { 2.0 };
    for order in [[0, 1], [1, 0]] {
        let mut positions = [
            Position {
                z: -1.2,
                ..Position::default()
            },
            Position::default(),
        ]
        .map(orient);
        if descending {
            positions = [-0.51, 0.51].map(|x| {
                let offset = orient(Position { x, y: 2.0, z: 0.0 });
                Position {
                    x: mount.x + offset.x,
                    y: offset.y,
                    z: mount.z + offset.z,
                }
            });
        }
        let mut velocities = [0.0; 2];
        let mut routes: [VecDeque<_>; 2] = [-2.0, 2.0].map(|x| {
            [
                NavWaypoint {
                    position: mount,
                    kind: WaypointKind::Mount,
                },
                NavWaypoint {
                    position: Position {
                        y: landing_y + 0.05,
                        ..mount
                    },
                    kind: WaypointKind::Climb {
                        normal_x: if transpose { -1.0 } else { 0.0 },
                        normal_z: if transpose { 0.0 } else { -1.0 },
                        ascending: !descending,
                    },
                },
                NavWaypoint {
                    position: orient(Position {
                        x: 0.0,
                        y: landing_y,
                        z: exit_z,
                    }),
                    kind: WaypointKind::Exit,
                },
                NavWaypoint::walk(orient(Position {
                    x,
                    y: landing_y,
                    z: exit_z,
                })),
            ]
            .into()
        });
        if descending {
            for route in &mut routes {
                route.pop_front();
            }
        }
        for _ in 0..600 {
            let starts = [0, 1].map(|i| (test_entity(i as u64 + 1), positions[i], physics));
            let mut plans = Vec::new();
            for i in order {
                while routes[i]
                    .front()
                    .is_some_and(|waypoint| waypoint.reached(&positions[i], 0.5))
                {
                    routes[i].pop_front();
                }
                let mut context = context(starts[i].0, &positions[i], &world, &plans, &starts);
                context.actor_physics = physics;
                context.can_use_ladders = true;
                context.delta = TICK_SECS;
                context.vertical_velocity = velocities[i];
                let selected = routes[i].front().map_or_else(
                    || context.idle_move(),
                    |waypoint| {
                        select_route_move(
                            &context,
                            waypoint.movement_intent(&positions[i], 5.0),
                            &waypoint.position,
                        )
                    },
                );
                plans.push(CharacterMovePlan::from_movement_result(
                    starts[i].0,
                    positions[i],
                    selected.step,
                    physics,
                ));
            }
            for (index, &i) in order.iter().enumerate() {
                let plan = &plans[index];
                if blocking_character_move_plan(plan, &plans).is_some() {
                    positions[i].y = plan.target.y;
                } else {
                    positions[i] = plan.target;
                }
                velocities[i] = plan.target_vertical_velocity;
            }
            assert!(!character_paths_intersect(
                &positions[0],
                &positions[0],
                physics,
                &positions[1],
                &positions[1],
                physics
            ));
            if routes.iter().all(VecDeque::is_empty) {
                break;
            }
        }
        assert!(
            routes.iter().all(VecDeque::is_empty),
            "mines stalled in order {order:?}: {positions:?}; routes: {routes:?}"
        );
        assert!(positions.iter().all(|pos| (pos.y - landing_y).abs() < 0.1));
    }
}
