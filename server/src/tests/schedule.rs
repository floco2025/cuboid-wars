use super::*;

#[derive(Component)]
struct Prepared;

#[derive(Component)]
struct Received;

#[derive(Component)]
struct Maintained;

#[derive(Component)]
struct Fought;

#[derive(Resource, Default)]
struct FlushObservations {
    ingress_saw_prepared: bool,
    behavior_saw_received: bool,
    lifecycle_saw_fought: bool,
    snapshot_saw_maintained: bool,
}

fn queue_prepared(mut commands: Commands) {
    commands.spawn(Prepared);
}

fn observe_prepared_and_queue_received(
    mut commands: Commands,
    prepared: Query<(), With<Prepared>>,
    mut observations: ResMut<FlushObservations>,
) {
    observations.ingress_saw_prepared = !prepared.is_empty();
    commands.spawn(Received);
}

fn observe_received(received: Query<(), With<Received>>, mut observations: ResMut<FlushObservations>) {
    observations.behavior_saw_received = !received.is_empty();
}

fn queue_fought(mut commands: Commands) {
    commands.spawn(Fought);
}

fn observe_fought(fought: Query<(), With<Fought>>, mut observations: ResMut<FlushObservations>) {
    observations.lifecycle_saw_fought = !fought.is_empty();
}

fn queue_maintained(mut commands: Commands) {
    commands.spawn(Maintained);
}

fn observe_maintained(maintained: Query<(), With<Maintained>>, mut observations: ResMut<FlushObservations>) {
    observations.snapshot_saw_maintained = !maintained.is_empty();
}

#[test]
fn deferred_commands_are_visible_after_phase_flushes() {
    let mut app = App::new();
    app.init_resource::<FlushObservations>();
    configure_server_schedule(&mut app);
    app.add_systems(
        Update,
        (
            queue_prepared.in_set(ServerSet::Prepare),
            observe_prepared_and_queue_received.in_set(ServerSet::Ingress),
            observe_received.in_set(ServerSet::Behavior),
            queue_fought.in_set(ServerSet::CombatExplosions),
            observe_fought.in_set(ServerSet::Lifecycle),
            queue_maintained.in_set(ServerSet::Maintenance),
            observe_maintained.in_set(ServerSet::Snapshot),
        ),
    );

    app.update();

    let observations = app.world().resource::<FlushObservations>();
    assert!(observations.ingress_saw_prepared);
    assert!(observations.behavior_saw_received);
    assert!(observations.lifecycle_saw_fought);
    assert!(observations.snapshot_saw_maintained);
}
