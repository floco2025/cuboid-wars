use super::*;
use crate::actors::{TraversalEnvironment, TraversalExecutor, TraversalStatus, navigation::surface::fixtures};
use common::{
    map::Carriers,
    physics::CollisionWorld,
    protocol::{Position, SwitchState},
};

#[test]
fn authored_shuttle_is_planned_and_ridden_repeatably_between_disconnected_floors() {
    let config = fixtures::config();
    let file = fixtures::shuttle();
    let generated = fixtures::compile(file, 30, &config.settings).expect("authored ferry");
    let physics = config.expect_actor("scuttler").character.physics();
    let mut previous = None;
    for _ in 0..2 {
        let mut world = CollisionWorld::from_map_layout(&generated.layout);
        let navigation = SurfaceNavigation::build(&generated.config, &generated.layout, &config, &world, &[], &[])
            .expect("navigation");
        let from = SurfaceGoal {
            carrier: CarrierId::WORLD,
            position: Position {
                x: -8.0,
                y: 0.0,
                z: -1.5,
            },
        };
        let to = SurfaceGoal {
            carrier: CarrierId::WORLD,
            position: Position {
                x: 8.0,
                y: 0.0,
                z: -1.5,
            },
        };
        let route = navigation
            .route(from, to, physics, false, 4096)
            .expect("route via shuttle");
        assert!(
            route
                .actions
                .iter()
                .any(|action| matches!(action, TraversalAction::Board { .. }))
        );
        assert!(
            route
                .actions
                .iter()
                .any(|action| matches!(action, TraversalAction::Ride { .. }))
        );
        let mut carriers = Carriers::from_layout(&generated.layout);
        let env = TraversalEnvironment {
            world: &world,
            carriers: &carriers,
            settings: &config.settings,
            open: &[],
            delta: 1.0 / 30.0,
        };
        let mut executor = TraversalExecutor::new(from.position, physics, 2.5, &env);
        executor.set_route(route);
        let mut rode = false;
        let mut reached_tick = None;
        for tick in 1..1200 {
            carriers.advance(tick, &SwitchState::default());
            world.set_carrier_poses(&carriers);
            executor.step(&TraversalEnvironment {
                world: &world,
                carriers: &carriers,
                settings: &config.settings,
                open: &[],
                delta: 1.0 / 30.0,
            });
            rode |= !executor.movement.carrier.is_world();
            assert!(!world.character_penetrates_solid(&executor.movement.position, physics, &[]));
            if executor.status == TraversalStatus::Reached {
                reached_tick = Some(tick);
                break;
            }
        }
        assert!(
            rode && executor.status == TraversalStatus::Reached
                && executor.movement.position.distance_sq(&to.position) < 0.1,
            "{executor:?}"
        );
        let outcome = (reached_tick, executor.movement.position);
        if let Some(previous) = previous {
            assert_eq!(outcome, previous);
        }
        previous = Some(outcome);
    }
}
