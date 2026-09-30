use super::*;
use crate::test_fixtures;
use common::{
    physics::CharacterMovementResult,
    protocol::{CarrierId, ClientMessage, Floor, MapLayout, PlayerGeneration},
};
use crossbeam_channel::{Receiver, unbounded};

fn app() -> (App, Entity, Receiver<ClientMessage>) {
    let layout = MapLayout {
        floors: vec![Floor {
            x1: -5.0,
            x2: 5.0,
            z1: -5.0,
            z2: 5.0,
            y: 0.0,
            thickness: 0.2,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..default()
    };
    let (sender, receiver) = unbounded();
    let mut app = App::new();
    app.init_resource::<Time>()
        .insert_resource(CollisionWorld::from_map_layout(&layout))
        .insert_resource(Carriers::from_layout(&layout))
        .insert_resource(test_fixtures::map_settings())
        .insert_resource(test_fixtures::gameplay_config())
        .init_resource::<PortalSet>()
        .init_resource::<SwitchState>()
        .insert_resource(NetworkConfig {
            update_hz: 30,
            ..default()
        })
        .init_resource::<PlayerMap>()
        .init_resource::<ActorMap>()
        .insert_resource(ClientToServerChannel::new(sender))
        .init_resource::<LocalPlayerInfo>()
        .add_systems(Update, local_player_movement_system);
    let start = Position { x: 0.0, y: 1.0, z: 0.0 };
    let entity = app
        .world_mut()
        .spawn((
            PlayerId(1),
            PlayerMarker,
            LocalPlayerMarker,
            start,
            PreviousTickPosition(Position { y: 1.7, ..start }),
            PlayerMoveIntent::NONE,
            FaceYaw(0.0),
            CharacterVerticalVelocity(-3.0),
            HorizontalVelocity(Vec3::X),
            KnockbackVelocity::default(),
            PlayerStance::default(),
            CharacterSupport::Airborne,
            JumpRequest::default(),
            LocalMovementStep::default(),
            CrouchBlend::default(),
        ))
        .id();
    (app, entity, receiver)
}

#[test]
fn a_dead_local_player_neither_moves_nor_reports_and_its_render_lerp_collapses() {
    let (mut app, entity, receiver) = app();
    app.world_mut().resource_mut::<LocalPlayerInfo>().is_dead = true;
    app.update();
    let world = app.world();
    let position = *world.get::<Position>(entity).expect("position missing");
    assert_eq!(position, Position { y: 1.0, ..default() });
    assert_eq!(
        world.get::<PreviousTickPosition>(entity).expect("previous missing").0,
        position
    );
    assert!(receiver.try_recv().is_err());
}

#[test]
fn a_living_local_player_steps_records_its_step_and_reports_in_its_generation() {
    let (mut app, entity, receiver) = app();
    app.world_mut()
        .resource_mut::<LocalPlayerInfo>()
        .reports
        .begin_body(PlayerGeneration(3));
    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(std::time::Duration::from_secs_f32(0.1));
    app.update();
    let world = app.world();
    let position = *world.get::<Position>(entity).expect("position missing");
    assert!(position.y < 1.0 && position.x > 0.0, "{position:?}");
    let step = world.get::<LocalMovementStep>(entity).expect("step missing");
    assert_eq!(step.result.position, position);
    assert_ne!(step.result, CharacterMovementResult::default());
    let Ok(ClientMessage::Move(report)) = receiver.try_recv() else {
        panic!("the first tick reports at once");
    };
    assert_eq!(report.generation, PlayerGeneration(3));
    assert_eq!(report.movement.pos, position);
}
