use bevy_ecs::prelude::*;

// Marker components to disambiguate entity archetypes across server and client.
#[derive(Component, Debug, Default)]
pub struct PlayerMarker;

#[derive(Component, Debug, Default)]
pub struct GhostMarker;

#[derive(Component, Debug, Default)]
pub struct ProjectileMarker;

#[derive(Component, Debug, Default)]
pub struct ItemMarker;
