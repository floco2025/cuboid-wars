use super::*;
use crate::actors::navigation::surface::fixtures;

#[test]
fn a_search_cut_short_by_the_ticks_leftover_budget_is_deferred_and_keeps_its_route() {
    let config = fixtures::config();
    let generated = fixtures::generate("fixture", 30, &config.settings).expect("authored scene");
    let world = CollisionWorld::from_map_layout(&generated.layout);
    let carriers = Carriers::from_layout(&generated.layout);
    let navigation =
        SurfaceNavigation::build(&generated.config, &generated.layout, &config, &world, &[], &[]).expect("navigation");
    let physics = config.expect_actor("scuttler").character.physics();
    let start = Position {
        x: -13.5,
        y: 0.0,
        z: -10.0,
    };
    let goal = |x| SurfaceGoal {
        carrier: CarrierId::WORLD,
        position: Position { x, y: 3.0, z: 0.0 },
    };
    let env = TraversalEnvironment {
        world: &world,
        carriers: &carriers,
        settings: &config.settings,
        open: &[],
        delta: 1.0 / 30.0,
    };
    let mut executor = TraversalExecutor::new(start, physics, 3.0, &env);
    let mut agent = SurfaceAgent {
        goal: Some(goal(4.5)),
        ..Default::default()
    };
    let mut planner = RoutePlanner {
        navigation: &navigation,
        carriers: &carriers,
        budget: TICK_SEARCH_VISITS,
    };
    planner.update(&mut agent, &mut executor, CarrierId::WORLD, start, physics, false);
    assert!(agent.failure.is_none() && !agent.pending, "{:?}", agent.failure);
    let route = executor.actions.clone();
    assert!(!route.is_empty());

    agent.goal = Some(goal(6.0));
    planner.budget = 1;
    planner.update(&mut agent, &mut executor, CarrierId::WORLD, start, physics, false);
    assert!(agent.pending && agent.failure.is_none(), "{:?}", agent.failure);
    assert_eq!(executor.actions, route);
    assert_eq!(planner.budget, 0);

    planner.budget = TICK_SEARCH_VISITS;
    planner.update(&mut agent, &mut executor, CarrierId::WORLD, start, physics, false);
    assert!(!agent.pending && agent.failure.is_none(), "{:?}", agent.failure);
    assert_ne!(executor.actions, route);
}
