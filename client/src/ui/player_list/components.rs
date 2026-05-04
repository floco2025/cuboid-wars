use bevy::prelude::*;

// Marker for the root node of the on-screen player list panel.
#[derive(Component)]
pub struct PlayerListMarker;

// Marker for individual player entry rows inside the player list panel.
#[derive(Component)]
pub struct PlayerEntryMarker;

pub(super) const LOCAL_PLAYER_BG_COLOR: Color = Color::srgba(0.8, 0.8, 0.0, 0.3);
