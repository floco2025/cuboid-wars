use bevy::prelude::Component;

// Root node of the quest panel (top-right HUD corner). Spawned once in
// `setup_ui_system`; its children — the per-quest rows — are rebuilt from
// `QuestLog`.
#[derive(Component)]
pub struct QuestPanelMarker;

// A single quest row under the panel root.
#[derive(Component)]
pub struct QuestEntryMarker;
