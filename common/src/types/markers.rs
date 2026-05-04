use bevy_ecs::prelude::*;

// Marker components disambiguating entity archetypes across server and client.
#[derive(Component, Debug, Default)]
pub struct PlayerMarker;

#[derive(Component, Debug, Default)]
pub struct ActorMarker;

#[derive(Component, Debug, Default)]
pub struct ItemMarker;
