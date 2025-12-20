use bevy::{prelude::*, scene::SceneInstanceReady};

use crate::constants::*;

// ============================================================================
// Components
// ============================================================================

// Component that stores a reference to an animation we want to play
#[derive(Component, Clone)]
pub struct AnimationToPlay {
    pub graph_handle: Handle<AnimationGraph>,
    pub index: AnimationNodeIndex,
}

// ============================================================================
// Animation System
// ============================================================================

// System that plays animations when the player scene is loaded
pub fn players_animation_system(
    scene_ready: On<SceneInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    animations_to_play: Query<&AnimationToPlay>,
    mut players: Query<&mut AnimationPlayer>,
) {
    // The entity we spawned in `spawn_player` is the trigger's target.
    // Start by finding the AnimationToPlay component we added to that entity.
    if let Ok(animation_to_play) = animations_to_play.get(scene_ready.entity) {
        // The SceneRoot component will have spawned the scene as a hierarchy
        // of entities parented to our entity. Since the asset contained a skinned
        // mesh and animations, it will also have spawned an animation player
        // component. Search our entity's descendants to find the animation player.
        for child in children.iter_descendants(scene_ready.entity) {
            if let Ok(mut player) = players.get_mut(child) {
                // Tell the animation player to start the animation and keep
                // repeating it.
                player.play(animation_to_play.index).repeat().set_speed(PLAYER_MODEL_ANIMATION_SPEED);

                // Add the animation graph. This only needs to be done once to
                // connect the animation player to the mesh.
                commands
                    .entity(child)
                    .insert(AnimationGraphHandle(animation_to_play.graph_handle.clone()));
            }
        }
    }
}

// System that plays animations when the sentry scene is loaded
pub fn sentries_animation_system(
    scene_ready: On<SceneInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    animations_to_play: Query<&AnimationToPlay>,
    mut players: Query<&mut AnimationPlayer>,
) {
    if let Ok(animation_to_play) = animations_to_play.get(scene_ready.entity) {
        for child in children.iter_descendants(scene_ready.entity) {
            if let Ok(mut player) = players.get_mut(child) {
                player.play(animation_to_play.index).repeat().set_speed(SENTRY_MODEL_ANIMATION_SPEED);

                commands
                    .entity(child)
                    .insert(AnimationGraphHandle(animation_to_play.graph_handle.clone()));
            }
        }
    }
}


