use super::*;
use crate::{actors::navigation::surface::fixtures, config::ServerGameplayConfig};

const START: Position = Position {
    x: -13.5,
    y: 0.0,
    z: -10.0,
};

struct Scene {
    config: ServerGameplayConfig,
    world: CollisionWorld,
    carriers: Carriers,
    navigation: SurfaceNavigation,
    physics: CharacterPhysicsConfig,
}

fn scene() -> Scene {
    let config = fixtures::config();
    let generated = fixtures::generate("fixture", 30, &config.settings).expect("authored scene");
    let world = CollisionWorld::from_map_layout(&generated.layout);
    let carriers = Carriers::from_layout(&generated.layout);
    let navigation =
        SurfaceNavigation::build(&generated.config, &generated.layout, &config, &world, &[], &[]).expect("navigation");
    let physics = config.expect_actor("scuttler").character.physics();
    Scene {
        config,
        world,
        carriers,
        navigation,
        physics,
    }
}

fn goal(x: f32) -> SurfaceGoal {
    SurfaceGoal {
        carrier: CarrierId::WORLD,
        position: Position { x, y: 3.0, z: 0.0 },
    }
}

#[test]
fn a_search_cut_short_by_the_ticks_leftover_budget_is_deferred_and_keeps_its_route() {
    let scene = scene();
    let physics = scene.physics;
    let mut executor = TraversalExecutor::new(physics, 3.0);
    let mut agent = SurfaceAgent {
        goal: Some(goal(4.5)),
        ..Default::default()
    };
    let mut planner = RoutePlanner {
        navigation: &scene.navigation,
        carriers: &scene.carriers,
        budget: TICK_SEARCH_VISITS,
    };
    planner.update(&mut agent, &mut executor, CarrierId::WORLD, START, physics, false, None);
    assert!(agent.failure.is_none() && !agent.pending, "{:?}", agent.failure);
    let route = executor.actions.clone();
    assert!(!route.is_empty());

    agent.goal = Some(goal(6.0));
    planner.budget = 1;
    planner.update(&mut agent, &mut executor, CarrierId::WORLD, START, physics, false, None);
    assert!(agent.pending && agent.failure.is_none(), "{:?}", agent.failure);
    assert_eq!(executor.actions, route);
    assert_eq!(planner.budget, 0);

    planner.budget = TICK_SEARCH_VISITS;
    planner.update(&mut agent, &mut executor, CarrierId::WORLD, START, physics, false, None);
    assert!(!agent.pending && agent.failure.is_none(), "{:?}", agent.failure);
    assert_ne!(executor.actions, route);
}

#[test]
fn a_deferred_retry_stops_pending_once_its_cause_clears() {
    let scene = scene();
    let physics = scene.physics;
    let env = fixtures::env(&scene.world, &scene.carriers, &scene.config);
    for status in [TraversalStatus::Blocked, TraversalStatus::LostSupport] {
        for budget in [0, 1] {
            let mut executor = TraversalExecutor::new(physics, 3.0);
            let mut body = ActorBody::standing(START);
            let mut agent = SurfaceAgent {
                goal: Some(goal(4.5)),
                ..Default::default()
            };
            let mut planner = RoutePlanner {
                navigation: &scene.navigation,
                carriers: &scene.carriers,
                budget: TICK_SEARCH_VISITS,
            };
            planner.update(&mut agent, &mut executor, CarrierId::WORLD, START, physics, false, None);
            assert!(!agent.pending && !executor.actions.is_empty());
            let route = executor.actions.clone();

            executor.status = status;
            agent.retry_secs = 0.0;
            planner.budget = budget;
            planner.update(&mut agent, &mut executor, CarrierId::WORLD, START, physics, false, None);
            assert!(agent.pending && agent.failure.is_none(), "a retry waits for its turn");
            assert_eq!(executor.actions, route);

            // The retained route recovers before another search gets a turn.
            executor.step(&env, &mut body);
            assert_eq!(executor.status, TraversalStatus::Moving);
            let position = body.position;
            planner.update(
                &mut agent,
                &mut executor,
                CarrierId::WORLD,
                position,
                physics,
                false,
                None,
            );
            assert!(!agent.pending, "nothing is requested once the route moves again");
            for _ in 0..1800 {
                planner.budget = TICK_SEARCH_VISITS;
                let position = body.position;
                planner.update(
                    &mut agent,
                    &mut executor,
                    CarrierId::WORLD,
                    position,
                    physics,
                    false,
                    None,
                );
                executor.step(&env, &mut body);
                if executor.status == TraversalStatus::Reached {
                    break;
                }
            }
            assert_eq!(executor.status, TraversalStatus::Reached);
            agent.executor = Some(executor);
            assert!(agent.reached(), "a completed route must allow the next roaming goal");
        }
    }
}
