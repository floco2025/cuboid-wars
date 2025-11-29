#[allow(clippy::wildcard_imports)]
use bevy::prelude::*;
use std::time::Duration;

use crate::resources::WallConfig;
use common::protocol::{Position, Velocity};

// ============================================================================
// Components
// ============================================================================

// Marker component for the local player (yourself)
#[derive(Component)]
pub struct LocalPlayer;

// Track bump flash effect state for local player
#[derive(Component, Default)]
pub struct BumpFlashState {
    pub was_colliding: bool,
    pub flash_timer: f32,
}

// Server's authoritative snapshot for this entity
#[derive(Component)]
pub struct ServerSnapshot {
    pub client_pos: Position,
    pub client_vel: Velocity,
    pub server_pos: Position,
    pub server_vel: Velocity,
    pub received_at: Duration,
    pub timer: f32,
}

// ============================================================================
// Client-side Movement System
// ============================================================================

// Client-side movement system with wall collision detection for smooth prediction
pub fn client_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    wall_config: Option<Res<WallConfig>>,
    mut query: Query<(
        Entity,
        &mut Position,
        &Velocity,
        Option<&mut BumpFlashState>,
        Option<&mut ServerSnapshot>,
        Has<LocalPlayer>,
    )>,
    mut bump_flash_ui: Query<(&mut BackgroundColor, &mut Visibility), With<super::ui::BumpFlashUI>>,
) {
    let delta = time.delta_secs();

    let walls = wall_config.as_deref();
    let entity_positions: Vec<(Entity, Position)> =
        query.iter().map(|(entity, pos, _, _, _, _)| (entity, *pos)).collect();

    for (entity, mut pos, velocity, mut flash_state, mut server_snapshot, is_local) in query.iter_mut() {
        if let Some(state) = flash_state.as_mut() {
            decay_flash_timer(state, delta, is_local, &mut bump_flash_ui);
        }

        let target_pos = if let Some(snapshot) = server_snapshot.as_mut() {
            let dx = (snapshot.server_pos.x - snapshot.client_pos.x) * delta / 0.2;
            let dz = (snapshot.server_pos.x - snapshot.client_pos.x) * delta / 0.2;

            snapshot.timer += delta;
            if snapshot.timer >= 0.2 {
                commands.entity(entity).remove::<ServerSnapshot>();
            }

            Position {
                x: pos.x + velocity.x * delta + dx,
                y: pos.y,
                z: pos.z + velocity.z * delta + dz,
            }
        } else {
            if !has_horizontal_velocity(velocity) {
                if let Some(state) = flash_state.as_mut() {
                    state.was_colliding = false;
                }
                continue;
            }

            Position {
                x: pos.x + velocity.x * delta,
                y: pos.y,
                z: pos.z + velocity.z * delta,
            }
        };

        let hit_wall = hits_wall(walls, &target_pos);
        let hit_player = hits_other_player(entity, &target_pos, &entity_positions);
        let blocked = hit_wall || hit_player;

        if !blocked {
            *pos = target_pos;
            if let Some(state) = flash_state.as_mut() {
                state.was_colliding = false;
            }
        } else if is_local {
            if let Some(state) = flash_state.as_mut() {
                trigger_collision_feedback(&mut commands, &asset_server, &mut bump_flash_ui, state, hit_wall);
            }
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

const BUMP_FLASH_DURATION: f32 = 0.08;

fn has_horizontal_velocity(velocity: &Velocity) -> bool {
    velocity.x.abs() > f32::EPSILON || velocity.z.abs() > f32::EPSILON
}

fn hits_wall(walls: Option<&WallConfig>, new_pos: &Position) -> bool {
    let Some(config) = walls else { return false };
    config
        .walls
        .iter()
        .any(|wall| common::collision::check_player_wall_collision(new_pos, wall))
}

fn hits_other_player(entity: Entity, new_pos: &Position, positions: &[(Entity, Position)]) -> bool {
    positions.iter().any(|(other_entity, other_pos)| {
        *other_entity != entity && common::collision::check_player_player_collision(new_pos, other_pos)
    })
}

fn decay_flash_timer(
    state: &mut Mut<BumpFlashState>,
    delta: f32,
    is_local: bool,
    bump_flash_ui: &mut Query<(&mut BackgroundColor, &mut Visibility), With<super::ui::BumpFlashUI>>,
) {
    if state.flash_timer <= 0.0 {
        return;
    }

    state.flash_timer -= delta;
    if state.flash_timer <= 0.0 && is_local {
        if let Some((mut bg_color, mut visibility)) = bump_flash_ui.iter_mut().next() {
            *visibility = Visibility::Hidden;
            bg_color.0 = Color::srgba(1.0, 1.0, 1.0, 0.0);
        }
    }
}

fn trigger_collision_feedback(
    commands: &mut Commands,
    asset_server: &AssetServer,
    bump_flash_ui: &mut Query<(&mut BackgroundColor, &mut Visibility), With<super::ui::BumpFlashUI>>,
    state: &mut Mut<BumpFlashState>,
    collided_with_wall: bool,
) {
    if !state.was_colliding {
        if let Some((mut bg_color, mut visibility)) = bump_flash_ui.iter_mut().next() {
            *visibility = Visibility::Visible;
            bg_color.0 = Color::srgba(1.0, 1.0, 1.0, 0.2);
        }

        let sound_path = if collided_with_wall {
            "sounds/player_bumps_wall.ogg"
        } else {
            "sounds/player_bumps_player.ogg"
        };

        commands.spawn((
            AudioPlayer::new(asset_server.load(sound_path)),
            PlaybackSettings::DESPAWN,
        ));

        state.flash_timer = BUMP_FLASH_DURATION;
    }

    state.was_colliding = true;
}
