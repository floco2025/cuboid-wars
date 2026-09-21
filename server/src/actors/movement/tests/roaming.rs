use super::*;
use crate::{
    actors::navigation::surface::{CarrierDock, fixtures},
    map::ZoneVolume,
};
use bevy::math::Vec3;
use common::protocol::{Position, SwitchState};

#[test]
fn roaming_routes_check_future_docks_and_intermediate_ladder_positions() {
    let config = fixtures::config();
    let generated = fixtures::compile(fixtures::shuttle(), 30, &config.settings).expect("shuttle");
    let mut carriers = Carriers::from_layout(&generated.layout);
    let carrier = CarrierId(1);
    let home = ActorTerritory {
        carrier: CarrierId::WORLD,
        volume: ZoneVolume {
            min: Vec3::new(-12.0, 0.0, -6.0),
            max: Vec3::new(12.0, 2.0, 6.0),
        },
        distance: 0.0,
        center_height: 0.45,
    };
    let start = SurfaceGoal {
        carrier: CarrierId::WORLD,
        position: Position {
            x: -8.0,
            y: 0.0,
            z: -1.5,
        },
    };
    let mut route = SurfaceRoute {
        expanded: 0,
        actions: [
            TraversalAction::Board {
                carrier,
                target: Position::default(),
                dock: CarrierDock {
                    parent: CarrierId::WORLD,
                    position: generated.layout.carriers[0].from,
                },
            },
            TraversalAction::Ride {
                carrier,
                dock: CarrierDock {
                    parent: CarrierId::WORLD,
                    position: generated.layout.carriers[0].to,
                },
            },
            TraversalAction::Walk {
                carrier: CarrierId::WORLD,
                target: start.position,
            },
        ]
        .into(),
    };
    for tick in [0, 120] {
        carriers.advance(tick, &SwitchState::default());
        assert!(route_stays_home(&route, start, &home, &carriers));
    }
    route.actions[1] = TraversalAction::Ride {
        carrier,
        dock: CarrierDock {
            parent: CarrierId::WORLD,
            position: Position {
                x: 20.0,
                y: 0.0,
                z: -1.5,
            },
        },
    };
    assert!(
        !route_stays_home(&route, start, &home, &carriers),
        "ride leaves home despite returning to an inside destination"
    );
    route.actions = [
        TraversalAction::Climb {
            carrier: CarrierId::WORLD,
            ladder: 0,
            target: Position {
                y: 3.0,
                ..start.position
            },
            normal: [0.0, 1.0],
            ascending: true,
        },
        TraversalAction::ExitLadder {
            carrier: CarrierId::WORLD,
            ladder: 0,
            target: start.position,
        },
    ]
    .into();
    assert!(!route_stays_home(&route, start, &home, &carriers));
    assert!(
        route_stays_home(&route, start, &ActorTerritory { distance: 2.0, ..home }, &carriers),
        "roaming extension applies vertically too"
    );
}
