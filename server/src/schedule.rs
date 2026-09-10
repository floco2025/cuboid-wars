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

// A configured duration as whole server ticks.
#[must_use]
pub fn ticks_from_secs(secs: f32, server_hz: u32) -> u32 {
    (secs * server_hz as f32).round() as u32
}

#[cfg(test)]
#[path = "tests/schedule.rs"]
mod tests;
