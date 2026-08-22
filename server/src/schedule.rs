use bevy::prelude::*;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ServerSet {
    Prepare,
    PrepareFlush,
    Ingress,
    IngressFlush,
    Behavior,
    Movement,
    CombatDamage,
    CombatRemoval,
    CombatExplosions,
    CombatFlush,
    Lifecycle,
    Maintenance,
    SnapshotFlush,
    Snapshot,
}

pub(crate) fn configure_server_schedule(app: &mut App) {
    app.configure_sets(
        Update,
        (
            ServerSet::Prepare,
            ServerSet::PrepareFlush,
            ServerSet::Ingress,
            ServerSet::IngressFlush,
            ServerSet::Behavior,
            ServerSet::Movement,
            ServerSet::CombatDamage,
            ServerSet::CombatRemoval,
            ServerSet::CombatExplosions,
            ServerSet::CombatFlush,
            ServerSet::Lifecycle,
            ServerSet::Maintenance,
            ServerSet::SnapshotFlush,
            ServerSet::Snapshot,
        )
            .chain_ignore_deferred(),
    )
    .add_systems(
        Update,
        (
            ApplyDeferred.in_set(ServerSet::PrepareFlush),
            ApplyDeferred.in_set(ServerSet::IngressFlush),
            ApplyDeferred.in_set(ServerSet::CombatFlush),
            ApplyDeferred.in_set(ServerSet::SnapshotFlush),
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct PhaseTrace(Vec<&'static str>);

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

    fn trace_prepare(mut trace: ResMut<PhaseTrace>) {
        trace.0.push("prepare");
    }

    fn trace_ingress(mut trace: ResMut<PhaseTrace>) {
        trace.0.push("ingress");
    }

    fn trace_behavior(mut trace: ResMut<PhaseTrace>) {
        trace.0.push("behavior");
    }

    fn trace_movement(mut trace: ResMut<PhaseTrace>) {
        trace.0.push("movement");
    }

    fn trace_combat_damage(mut trace: ResMut<PhaseTrace>) {
        trace.0.push("combat_damage");
    }

    fn trace_combat_removal(mut trace: ResMut<PhaseTrace>) {
        trace.0.push("combat_removal");
    }

    fn trace_combat_explosions(mut trace: ResMut<PhaseTrace>) {
        trace.0.push("combat_explosions");
    }

    fn trace_lifecycle(mut trace: ResMut<PhaseTrace>) {
        trace.0.push("lifecycle");
    }

    fn trace_maintenance(mut trace: ResMut<PhaseTrace>) {
        trace.0.push("maintenance");
    }

    fn trace_snapshot(mut trace: ResMut<PhaseTrace>) {
        trace.0.push("snapshot");
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
    fn phases_run_in_declared_order() {
        let mut app = App::new();
        app.init_resource::<PhaseTrace>();
        configure_server_schedule(&mut app);
        app.add_systems(
            Update,
            (
                trace_prepare.in_set(ServerSet::Prepare),
                trace_ingress.in_set(ServerSet::Ingress),
                trace_behavior.in_set(ServerSet::Behavior),
                trace_movement.in_set(ServerSet::Movement),
                trace_combat_damage.in_set(ServerSet::CombatDamage),
                trace_combat_removal.in_set(ServerSet::CombatRemoval),
                trace_combat_explosions.in_set(ServerSet::CombatExplosions),
                trace_lifecycle.in_set(ServerSet::Lifecycle),
                trace_maintenance.in_set(ServerSet::Maintenance),
                trace_snapshot.in_set(ServerSet::Snapshot),
            ),
        );

        app.update();

        assert_eq!(
            app.world().resource::<PhaseTrace>().0,
            [
                "prepare",
                "ingress",
                "behavior",
                "movement",
                "combat_damage",
                "combat_removal",
                "combat_explosions",
                "lifecycle",
                "maintenance",
                "snapshot",
            ]
        );
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
}
