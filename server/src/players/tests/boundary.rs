use super::*;
use crate::{
    config::{ActorRespawnScope, PlayerRespawnMode},
    players::{
        CheckpointId, PlayerCheckpoint,
        respawn_tests::{add_player, advance, respawn_app},
    },
};
use common::{
    map::{Grounds, GroundsSettings},
    protocol::{CarrierId, Checkpoint, CheckpointKind, Floor, ServerMessage},
};

fn app() -> App {
    let mut app = respawn_app(PlayerRespawnMode::Individual, ActorRespawnScope::Dead);
    let mut layout = MapLayout {
        grounds: Some(Grounds {
            half_size: [20.0, 20.0],
            y: 0.0,
            settings: GroundsSettings {
                level: 0,
                margin: 50.0,
                return_secs: 8.0,
                material: "grass".into(),
            },
        }),
        ..default()
    };
    layout.checkpoints.push(Checkpoint {
        kind: CheckpointKind::Individual,
        carrier: CarrierId::WORLD,
        level: 0,
        min_x: 0.0,
        max_x: 4.0,
        min_z: 0.0,
        max_z: 4.0,
        y: 0.0,
    });
    layout.floors.push(Floor {
        x1: -20.0,
        x2: 20.0,
        z1: -20.0,
        z2: 20.0,
        y: 0.0,
        thickness: 0.4,
        level: 0,
        carrier: CarrierId::WORLD,
    });
    app.insert_resource(CollisionWorld::from_map_layout(&layout))
        .insert_resource(layout)
        .add_systems(Update, players_boundary_system);
    app
}

#[test]
fn boundary_returns_to_checkpoint_preserving_health_inventory_and_score() {
    let mut app = app();
    let id = PlayerId(1);
    let (entity, messages) = add_player(&mut app, id);
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .get_mut(&id)
        .expect("player missing")
        .session
        .checkpoint = Some(PlayerCheckpoint {
        id: CheckpointId(0),
        facing: Vec3::Z,
    });
    app.world_mut().entity_mut(entity).insert(Position {
        x: 75.0,
        y: 0.0,
        z: 0.0,
    });
    advance(&mut app, 4.0);
    assert_eq!(app.world().get::<Position>(entity).expect("position missing").x, 75.0);
    advance(&mut app, 4.0);
    let pos = app.world().get::<Position>(entity).expect("position missing");
    assert!((0.0..4.0).contains(&pos.x) && (0.0..4.0).contains(&pos.z));
    let player = app.world().resource::<PlayerMap>().get(&id).expect("player missing");
    assert_eq!(player.session.generation.0, 1);
    assert_eq!(player.life.missiles, 2);
    assert_eq!(player.session.score, 42);
    assert_eq!(app.world().get::<Health>(entity).expect("health missing").0, 30.0);
    assert!(
        messages
            .try_iter()
            .any(|m| matches!(m, ServerMessage::PlayerRelocated(_)))
    );
}

#[test]
fn reentry_cancels_return_and_no_checkpoint_uses_a_spawn_zone() {
    let mut app = app();
    let (entity, _) = add_player(&mut app, PlayerId(1));
    app.world_mut().entity_mut(entity).insert(Position {
        x: 75.0,
        y: 0.0,
        z: 0.0,
    });
    advance(&mut app, 5.0);
    app.world_mut()
        .entity_mut(entity)
        .insert(Position { x: 0.0, y: 0.0, z: 0.0 });
    advance(&mut app, 4.0);
    app.world_mut().entity_mut(entity).insert(Position {
        x: 75.0,
        y: 0.0,
        z: 0.0,
    });
    advance(&mut app, 4.0);
    assert_eq!(app.world().get::<Position>(entity).expect("position missing").x, 75.0);
    advance(&mut app, 4.0);
    assert!(app.world().get::<Position>(entity).expect("position missing").x.abs() < 20.0);
}

#[test]
fn blocked_checkpoint_returns_to_spawn_instead_of_leaving_the_player_outside() {
    let mut app = app();
    let (entity, _) = add_player(&mut app, PlayerId(1));
    let (blocker, _) = add_player(&mut app, PlayerId(2));
    {
        let mut layout = app.world_mut().resource_mut::<MapLayout>();
        layout.checkpoints[0].max_x = 0.6;
        layout.checkpoints[0].max_z = 0.6;
    }
    app.world_mut()
        .entity_mut(blocker)
        .insert(Position { x: 0.3, y: 0.0, z: 0.3 });
    app.world_mut()
        .resource_mut::<PlayerMap>()
        .get_mut(&PlayerId(1))
        .expect("player missing")
        .session
        .checkpoint = Some(PlayerCheckpoint {
        id: CheckpointId(0),
        facing: Vec3::Z,
    });
    app.world_mut().entity_mut(entity).insert(Position {
        x: 75.0,
        y: 0.0,
        z: 0.0,
    });
    advance(&mut app, 8.0);
    assert!(app.world().get::<Position>(entity).expect("position missing").x < 0.0);
}
