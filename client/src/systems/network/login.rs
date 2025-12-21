use bevy::prelude::*;

use crate::{
    resources::{MyPlayerId, PlayerInfo, PlayerMap},
    spawning::spawn_player,
};
use common::{constants::POWER_UP_SPEED_MULTIPLIER, protocol::*};

// ============================================================================
// Login/Logout Handlers
// ============================================================================

/// Handle Init message when not yet logged in - stores player ID and map layout.
pub fn handle_init_message(msg: ServerMessage, commands: &mut Commands) {
    if let ServerMessage::Init(init_msg) = msg {
        debug!("received Init: my_id={:?}", init_msg.id);

        // Store player ID as resource
        commands.insert_resource(MyPlayerId(init_msg.id));

        // Store grid configuration
        commands.insert_resource(init_msg.map_layout);

        // Note: We don't spawn anything here. The first SUpdate will contain
        // all players including ourselves and will trigger spawning via the
        // Update message handler.
    }
}

/// Handle another player logging in - spawn their entity.
pub fn handle_player_login_message(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
    graphs: &mut ResMut<Assets<AnimationGraph>>,
    players: &mut ResMut<PlayerMap>,
    asset_server: &Res<AssetServer>,
    msg: SLogin,
) {
    debug!("{:?} logged in", msg.id);
    if players.0.contains_key(&msg.id) {
        return;
    }

    let mut velocity = msg.player.speed.to_velocity();
    if msg.player.speed_power_up {
        velocity.x *= POWER_UP_SPEED_MULTIPLIER;
        velocity.z *= POWER_UP_SPEED_MULTIPLIER;
    }
    let entity = spawn_player(
        commands,
        asset_server,
        meshes,
        materials,
        images,
        graphs,
        msg.id.0,
        &msg.player.name,
        &msg.player.pos,
        velocity,
        msg.player.face_dir,
        false,
    );
    players.0.insert(
        msg.id,
        PlayerInfo {
            entity,
            hits: 0,
            name: msg.player.name,
            speed_power_up: msg.player.speed_power_up,
            multi_shot_power_up: msg.player.multi_shot_power_up,
            phasing_power_up: msg.player.phasing_power_up,
            sentry_hunt_power_up: msg.player.sentry_hunt_power_up,
            stunned: msg.player.stunned,
        },
    );
}

/// Handle player logging off - despawn their entity.
pub fn handle_player_logoff_message(commands: &mut Commands, players: &mut ResMut<PlayerMap>, msg: SLogoff) {
    debug!("{:?} logged off (graceful: {})", msg.id, msg.graceful);
    if let Some(player) = players.0.remove(&msg.id) {
        commands.entity(player.entity).despawn();
    }
}
