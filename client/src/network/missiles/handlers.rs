use bevy::prelude::*;

use super::{super::context::ServerMessageContext, sync::apply_missile_movement_state};
use crate::{
    audio::{play_explosion_sound, play_sound, play_spatial_sound},
    missiles::{MissileFlight, MissileInfo, OwnedMissile, RemoteMissileMotion, spawn_missile},
    network::SampleTiming,
    vfx::spawn_missile_explosion,
};
use common::protocol::*;
use rand::RngExt;
use std::f32::consts::TAU;

pub(in crate::network) fn handle_missile_launch_message(
    message: SMissileLaunch,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut ServerMessageContext,
) {
    if context.missiles.is_retired(&message.id) {
        return;
    }
    if !context.missiles.contains_key(&message.id) {
        if message.shooter != my_player_id && launch_is_stale(context.clocks.last_snapshot_tick.0, message.tick) {
            return;
        }
        let entity = spawn_missile(commands, &context.assets.missile_assets, message.id, &message.movement);
        if message.shooter == my_player_id {
            commands.entity(entity).insert(OwnedMissile {
                flight: MissileFlight::new(
                    message.shooter,
                    message.target,
                    rand::rng().random_range(0.0..TAU),
                    context.gameplay_config.missiles.lifetime_secs,
                ),
                seq: 0,
            });
        } else {
            commands.entity(entity).insert(RemoteMissileMotion::new(
                0,
                message.movement,
                SampleTiming::new(&context.client_settings.interpolation, &context.network),
            ));
        }
        context.missiles.insert(
            message.id,
            MissileInfo {
                entity,
                shooter: message.shooter,
                born_tick: message.tick,
                impact_pending: false,
            },
        );
    }
    if message.shooter == my_player_id {
        play_sound(
            commands,
            &context.assets.asset_server,
            context.assets.asset_set.player_sound("missile_launch"),
        );
    } else {
        play_spatial_sound(
            commands,
            &context.assets.asset_server,
            context.assets.asset_set.player_sound("missile_launch"),
            &context.client_settings.audio,
            Vec3::from(message.movement.pos),
        );
    }
}

// A launch behind a snapshot that already omits the missile describes a flight that has ended.
fn launch_is_stale(last_snapshot_tick: Option<u32>, launch_tick: u32) -> bool {
    last_snapshot_tick.is_some_and(|tick| !sequence_is_newer(launch_tick, tick))
}

pub(in crate::network) fn handle_missile_moves_message(
    message: SMissileMoves,
    commands: &mut Commands,
    context: &mut ServerMessageContext,
) {
    for update in message.moves {
        if let Some(info) = context.missiles.get(&update.id) {
            apply_missile_movement_state(commands, info.entity, update.seq, update.movement);
        }
    }
}

// An observer flies the missile on to the reported impact before exploding
// it; the shooter already exploded its own flight where it ended.
pub(in crate::network) fn handle_missile_detonated_message(
    message: SMissileDetonated,
    commands: &mut Commands,
    context: &mut ServerMessageContext,
) {
    if !context.missiles.retire(message.id, message.tick) {
        return;
    }
    let Some((entity, shooter)) = context
        .missiles
        .get(&message.id)
        .map(|info| (info.entity, info.shooter))
    else {
        explode(commands, context, message.pos);
        return;
    };
    if shooter == context.my_player_id.0 {
        // A flight the server ended before this shooter did.
        if let Some(info) = context.missiles.take(&message.id) {
            commands.entity(info.entity).despawn();
        }
        explode(commands, context, message.pos);
        return;
    }
    if let Some(info) = context.missiles.get_mut(&message.id) {
        info.impact_pending = true;
    }
    let tick_secs = context.network.tick_secs();
    let pos = message.pos;
    commands.entity(entity).queue(move |mut entity: EntityWorldMut| {
        if let Some(mut motion) = entity.get_mut::<RemoteMissileMotion>() {
            motion.detonate_at(pos, tick_secs);
        }
    });
}

fn explode(commands: &mut Commands, context: &mut ServerMessageContext, pos: Position) {
    spawn_missile_explosion(commands, &mut context.explosion_ctx(), pos);
    play_explosion_sound(
        commands,
        &context.assets.asset_server,
        context.assets.asset_set.player_sound("explodes"),
        &context.client_settings.audio,
        Vec3::from(pos),
        Some(context.assets.blast_radii.missile),
    );
}

#[cfg(test)]
mod tests {
    use super::launch_is_stale;

    #[test]
    fn a_launch_is_stale_only_behind_an_applied_snapshot() {
        assert!(!launch_is_stale(None, 5));
        assert!(!launch_is_stale(Some(4), 5));
        assert!(launch_is_stale(Some(5), 5));
        assert!(launch_is_stale(Some(6), 5));
        assert!(!launch_is_stale(Some(u32::MAX), 0));
        assert!(launch_is_stale(Some(0), u32::MAX));
    }
}
