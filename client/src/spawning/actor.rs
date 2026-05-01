use bevy::prelude::*;

use super::spawn_player;
use crate::config::AssetSet;
use common::{
    markers::{ActorMarker, PlayerMarker},
    physics::CharacterVerticalMotion,
    protocol::{Actor, ActorId, PlayerId},
};

pub fn spawn_actor(
    commands: &mut Commands,
    asset_server: &Res<AssetServer>,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
    graphs: &mut ResMut<Assets<AnimationGraph>>,
    asset_set: &AssetSet,
    actor_id: ActorId,
    actor: &Actor,
) -> Entity {
    let entity = spawn_player(
        commands,
        asset_server,
        meshes,
        materials,
        images,
        graphs,
        asset_set,
        actor_id.0,
        &format!("Actor {}", actor_id.0),
        &actor.movement.pos,
        actor.movement.move_intent,
        actor.face_dir,
        false,
    );

    commands
        .entity(entity)
        .remove::<PlayerId>()
        .remove::<PlayerMarker>()
        .insert((
            actor_id,
            ActorMarker,
            CharacterVerticalMotion {
                vertical_velocity: actor.movement.vertical_velocity,
            },
        ));

    entity
}
