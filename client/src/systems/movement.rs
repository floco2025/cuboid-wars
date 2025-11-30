use bevy::prelude::*;

use crate::resources::WallConfig;
use common::{
    constants::UPDATE_BROADCAST_INTERVAL,
    protocol::{Position, Velocity},
};

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
pub struct ServerReconciliation {
    pub client_pos: Position,
    pub server_pos: Position,
    pub server_vel: Velocity,
    pub timer: f32,
    pub rtt: f32,
}

// ============================================================================
// Client-side Movement System
// ============================================================================

type MovementQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Position,
        &'static Velocity,
        Option<&'static mut BumpFlashState>,
        Option<&'static mut ServerReconciliation>,
        Has<LocalPlayer>,
    ),
>;

// Client-side movement system with wall collision detection for smooth prediction
pub fn client_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    wall_config: Option<Res<WallConfig>>,
    mut query: MovementQuery,
    mut bump_flash_ui: Query<(&mut BackgroundColor, &mut Visibility), With<super::ui::BumpFlashUI>>,
) {
    let delta = time.delta_secs();

    let walls = wall_config.as_deref();
    let entity_positions: Vec<(Entity, Position)> =
        query.iter().map(|(entity, pos, _, _, _, _)| (entity, *pos)).collect();

    for (entity, mut client_pos, client_vel, mut flash_state, mut server_snapshot, is_local) in &mut query {
        if let Some(state) = flash_state.as_mut() {
            decay_flash_timer(state, delta, is_local, &mut bump_flash_ui);
        }

        let target_pos = if let Some(recon) = server_snapshot.as_mut() {
            // recon.timer += delta;
            // if recon.timer >= UPDATE_BROADCAST_INTERVAL {
            //     commands.entity(entity).remove::<ServerReconciliation>();
            // }

            let mut correction_factor = UPDATE_BROADCAST_INTERVAL / 5.0;
            if correction_factor > 1.0 {
                correction_factor = 1.0;
            }

            let server_pos_x = recon.server_pos.x + recon.server_vel.x * recon.rtt / 2.0;
            let server_pos_z = recon.server_pos.z + recon.server_vel.z * recon.rtt / 2.0;

            let total_dx = server_pos_x - recon.client_pos.x;
            let total_dz = server_pos_z - recon.client_pos.z;

            let dx = total_dx * delta / UPDATE_BROADCAST_INTERVAL * correction_factor;
            let dz = total_dz * delta / UPDATE_BROADCAST_INTERVAL * correction_factor;

            Position {
                x: client_vel.x.mul_add(delta, client_pos.x) + dx,
                y: client_pos.y,
                z: client_vel.z.mul_add(delta, client_pos.z) + dz,
            }
        } else {
            Position {
                x: client_vel.x.mul_add(delta, client_pos.x),
                y: client_pos.y,
                z: client_vel.z.mul_add(delta, client_pos.z),
            }
        };

        let hit_wall = hits_wall(walls, &target_pos);
        let hit_player = hits_other_player(entity, &target_pos, &entity_positions);

        if !hit_wall && !hit_player {
            *client_pos = target_pos;
            if let Some(state) = flash_state.as_mut() {
                state.was_colliding = false;
            }
        } else if hit_wall {
            // Slide along the wall instead of stopping
            let slide_pos = common::collision::calculate_wall_slide(
                &walls.expect("walls should exist if hit_wall is true").walls,
                &client_pos,
                &target_pos,
                client_vel.x,
                client_vel.z,
                delta,
            );
            *client_pos = slide_pos;

            if is_local && let Some(state) = flash_state.as_mut() {
                trigger_collision_feedback(&mut commands, &asset_server, &mut bump_flash_ui, state, true);
            }
        } else if hit_player {
            // Stop for player collisions
            if is_local && let Some(state) = flash_state.as_mut() {
                trigger_collision_feedback(&mut commands, &asset_server, &mut bump_flash_ui, state, false);
            }
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

const BUMP_FLASH_DURATION: f32 = 0.08;

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
    if state.flash_timer <= 0.0
        && is_local
        && let Some((mut bg_color, mut visibility)) = bump_flash_ui.iter_mut().next()
    {
        *visibility = Visibility::Hidden;
        bg_color.0 = Color::srgba(1.0, 1.0, 1.0, 0.0);
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
