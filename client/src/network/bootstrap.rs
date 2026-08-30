use bevy::prelude::*;

use crate::{barriers::KeyKinds, characters::MaxHealth, players::MyPlayerId, vfx::BlastRadii};
use common::{physics::CollisionWorld, protocol::*};

pub(super) fn handle_bootstrap_message(message: SInit, commands: &mut Commands, barrier_kind_table: &BarrierKindTable) {
    debug!("received Init: my_id=player#{}", message.id.0);
    commands.insert_resource(MyPlayerId(message.id));
    let collision_world = CollisionWorld::from_map_layout(&message.map_layout, barrier_kind_table);
    commands.insert_resource(message.map_layout);
    commands.insert_resource(message.map_settings);
    commands.insert_resource(collision_world);
    commands.insert_resource(BlastRadii {
        actors: message.actor_blast_radii.into_iter().collect(),
        player: message.player_blast_radius,
        missile: message.missile_blast_radius,
    });
    commands.insert_resource(MaxHealth {
        player: message.player_max_health,
        actors: message.actor_max_health.into_iter().collect(),
    });
    commands.insert_resource(KeyKinds(message.key_kinds));
}
