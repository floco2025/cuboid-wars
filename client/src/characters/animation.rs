use bevy::{gltf::Gltf, prelude::*, world_serialization::WorldInstanceReady};

use super::CharacterModel;

// The one looping clip a model plays from the moment its scene is ready.
#[derive(Component, Clone)]
pub struct AnimationToPlay {
    pub animation_index: usize,
    pub speed: f32,
}

pub fn character_animation_system(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    models: Query<(&CharacterModel, &AnimationToPlay)>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut players: Query<&mut AnimationPlayer>,
) {
    let Ok((model, animation)) = models.get(ready.entity) else {
        return;
    };
    let Some(clip) = model
        .clips(&gltfs)
        .and_then(|clips| clips.get(animation.animation_index))
    else {
        error!("{} has no clip {}", model.scene(), animation.animation_index);
        return;
    };
    let (graph, index) = AnimationGraph::from_clip(clip.clone());
    let graph = graphs.add(graph);
    for child in children.iter_descendants(ready.entity) {
        if let Ok(mut player) = players.get_mut(child) {
            player.play(index).repeat().set_speed(animation.speed);
            commands.entity(child).insert(AnimationGraphHandle(graph.clone()));
        }
    }
}
