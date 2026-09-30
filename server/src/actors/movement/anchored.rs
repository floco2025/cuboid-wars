use bevy::prelude::*;
use common::{map::Carriers, physics::CharacterSupport, protocol::ActorMoveIntent};

use super::query::FreeActorQuery;
use crate::actors::{ActorMap, ActorMode};

// An anchored actor sits on its anchor wherever the carrier takes it; gravity,
// knockback, and other bodies never move it. It faces its target while engaged.
pub(crate) fn anchored_actors_placement_system(
    carriers: Res<Carriers>,
    actors: Res<ActorMap>,
    mut query: FreeActorQuery,
) {
    for mut actor in &mut query {
        let Some((info, anchor)) = actors.get(actor.id).and_then(|info| Some((info, info.anchor?))) else {
            continue;
        };
        *actor.position = anchor.world_position(&carriers);
        actor.vertical_velocity.0 = 0.0;
        *actor.intent = ActorMoveIntent::Idle;
        *actor.support = CharacterSupport::Ground;
        actor.crushed.0 = false;
        actor.landing.0 = 0.0;
        if let ActorMode::Engage { target_pos, .. } = info.mode {
            actor.face_yaw.0 = (target_pos.x - actor.position.x).atan2(target_pos.z - actor.position.z);
        }
    }
}

#[cfg(test)]
#[path = "tests/anchored.rs"]
mod tests;
