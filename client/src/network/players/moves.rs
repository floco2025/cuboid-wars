use super::super::context::ServerMessageContext;
use crate::{
    characters::PreviousTickPosition,
    network::{ServerReconciliation, TickSync, extrapolated_correction, resources::accept_newer_tick},
    players::{CrossingVerdict, LocalPlayerInfo, PlayerInfo, PlayerMap},
};
use bevy::prelude::*;
use common::{
    config::MapMovementConfig,
    constants::PLAYER_MOVEMENT_TRUST_DISTANCE,
    math::{player_movement_is_trusted, worst_axis_divergence},
    physics::{AirborneMomentum, CharacterVerticalVelocity, KnockbackVelocity, player_control_velocity},
    protocol::*,
};

pub(in crate::network) fn handle_player_moves_message(
    message: SPlayerMoves,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut ServerMessageContext,
) {
    if !accept_newer_tick(&mut context.clocks.last_player_moves_tick.0, message.tick) {
        return;
    }
    let tick = message.tick;
    if context.clocks.tick_sync.takes_rough_seed() {
        context.clocks.server_tick.0 = tick.wrapping_add(1);
    }
    if let Some(own) = message.moves.iter().find(|entry| entry.id == my_player_id)
        && let Some(seq) = own.move_seq
    {
        sync_player_clock(
            tick,
            seq,
            &context.local_player_info,
            &mut context.clocks.tick_sync,
            &mut context.clocks.server_tick,
            &mut context.players,
        );
    }
    let clock_seeded = context.clocks.tick_sync.is_seeded();
    for entry in message.moves {
        let Some(player) = context.players.get_mut(&entry.id) else {
            continue;
        };
        let is_local = entry.id == my_player_id;
        if is_local && context.local_player_info.is_dead {
            continue;
        }
        if is_local {
            let Some(seq) = entry.move_seq else {
                continue;
            };
            let Some(delta) = local_snap_delta(&mut context.local_player_info, &entry) else {
                continue;
            };
            let (axis, magnitude) = worst_axis_divergence(delta);
            warn!(
                "{}#{}, move {} out of sync: |{}|={:.2} >= {:.2} (Δ x={:.2}, y={:.2}, z={:.2}); snapping to server position",
                player.name,
                entry.id.0,
                seq,
                axis,
                magnitude,
                PLAYER_MOVEMENT_TRUST_DISTANCE,
                delta.x,
                delta.y,
                delta.z
            );
            snap_player(commands, player, entry.movement, entry.hops, tick);
            reset_local_comparisons(&mut context.local_player_info);
            continue;
        }
        match player.judge_crossing(tick, entry.hops, clock_seeded) {
            CrossingVerdict::Skipped => continue,
            CrossingVerdict::Settled => {
                warn!("{} crossing dispute settled for the server; teleporting", player.name);
                snap_player(commands, player, entry.movement, entry.hops, tick);
                continue;
            }
            CrossingVerdict::Paired => {}
        }
        let movement = entry.movement;
        let velocity = player_movement_velocity(
            movement,
            &context.map_settings.movement,
            player.power_up(PowerUpKind::Speed),
            player.stunned,
        );
        let mut entity = commands.entity(player.entity);
        entity.insert((
            movement.move_intent,
            FaceYaw(movement.face_yaw),
            CharacterVerticalVelocity(movement.vertical_velocity),
            AirborneMomentum(Vec3::from_array(movement.airborne_momentum)),
            KnockbackVelocity(Vec3::from_array(movement.knockback)),
        ));
        if let Ok(pos) = context.player_data.get(player.entity) {
            entity.insert(ServerReconciliation::new(
                extrapolated_correction(*pos, movement.pos, velocity, &context.rtt),
                movement.pos,
                velocity,
                &context.rtt,
            ));
        }
    }
}

fn local_snap_delta(local: &mut LocalPlayerInfo, entry: &PlayerMove) -> Option<Vec3> {
    let seq = entry.move_seq?;
    if local
        .last_comparison_seq
        .is_some_and(|last| !sequence_is_newer(seq, last))
    {
        return None;
    }
    local.last_comparison_seq = Some(seq);
    let recorded = local.committed_positions.get(seq)?.pos;
    let delta = Vec3::from(entry.movement.pos) - Vec3::from(recorded);
    (!player_movement_is_trusted(delta)).then_some(delta)
}

fn reset_local_comparisons(local: &mut LocalPlayerInfo) {
    local.committed_positions.clear();
    local.pending_input = None;
    local.last_comparison_seq = Some(local.move_seq);
}

fn snap_player(commands: &mut Commands, info: &mut PlayerInfo, movement: PlayerMovementState, hops: u32, tick: u32) {
    info.hops = hops;
    info.hop_tick = tick;
    info.disputed_since = None;
    commands
        .entity(info.entity)
        .insert((
            movement.pos,
            PreviousTickPosition(movement.pos),
            movement.move_intent,
            FaceYaw(movement.face_yaw),
            CharacterVerticalVelocity(movement.vertical_velocity),
            AirborneMomentum(Vec3::from_array(movement.airborne_momentum)),
            KnockbackVelocity(Vec3::from_array(movement.knockback)),
        ))
        .remove::<ServerReconciliation>();
}

fn sync_player_clock(
    tick: u32,
    move_seq: u32,
    local_player_info: &LocalPlayerInfo,
    tick_sync: &mut TickSync,
    server_tick: &mut ServerTick,
    players: &mut PlayerMap,
) {
    let Some(recorded_tick) = local_player_info.committed_positions.tick_for_seq(move_seq) else {
        return;
    };
    let error = tick.wrapping_sub(recorded_tick) as i32;
    trace!("clock error {error} ticks at the echo of seq {move_seq}");
    if let Some(shift) = tick_sync.observe(error, move_seq, local_player_info.move_seq) {
        server_tick.0 = server_tick.0.wrapping_add_signed(shift);
        for player in players.values_mut() {
            player.hop_tick = player.hop_tick.wrapping_add_signed(shift);
        }
        info!("clock shifted by {shift} ticks to {}", server_tick.0);
    }
}

fn player_movement_velocity(
    movement: PlayerMovementState,
    map_movement: &MapMovementConfig,
    has_speed_power_up: bool,
    movement_disabled: bool,
) -> Vec3 {
    let mut velocity = player_control_velocity(
        movement.move_intent,
        map_movement,
        has_speed_power_up,
        movement_disabled,
    );
    velocity += Vec3::from_array(movement.airborne_momentum) + Vec3::from_array(movement.knockback);
    velocity.y = movement.vertical_velocity;
    velocity
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comparison(seq: u32, x: f32, hops: u32) -> PlayerMove {
        PlayerMove {
            id: PlayerId(1),
            move_seq: Some(seq),
            hops,
            movement: PlayerMovementState::new(Position { x, y: 0.0, z: 0.0 }, PlayerMoveIntent::Idle, 0.0, 0.0),
        }
    }

    #[test]
    fn an_update_without_a_processed_report_cannot_snap_the_local_player() {
        let mut local = LocalPlayerInfo::default();
        local.committed_positions.record(1, 10, Position::default());
        let mut entry = comparison(1, 100.0, 0);
        entry.move_seq = None;
        assert!(local_snap_delta(&mut local, &entry).is_none());
        entry.move_seq = Some(1);
        assert!(local_snap_delta(&mut local, &entry).is_some());
    }

    #[test]
    fn local_comparison_uses_the_recorded_position_despite_hop_disagreement() {
        let mut local = LocalPlayerInfo::default();
        local.committed_positions.record(1, 10, Position::default());
        assert!(local_snap_delta(&mut local, &comparison(1, 4.99, 0)).is_none());
        local.committed_positions.record(2, 11, Position::default());
        assert_eq!(
            local_snap_delta(&mut local, &comparison(2, 5.0, 0)),
            Some(Vec3::X * 5.0)
        );
    }

    #[test]
    fn repeated_outdated_and_missing_comparisons_do_not_snap() {
        let mut local = LocalPlayerInfo::default();
        local.committed_positions.record(2, 10, Position::default());
        let result = comparison(2, 8.0, 0);
        assert!(local_snap_delta(&mut local, &result).is_some());
        assert!(local_snap_delta(&mut local, &result).is_none());
        assert!(local_snap_delta(&mut local, &comparison(1, 100.0, 0)).is_none());
        assert!(local_snap_delta(&mut local, &comparison(3, 100.0, 0)).is_none());
    }

    #[test]
    fn snap_ignores_in_flight_reports_then_accepts_fresh_comparisons() {
        let mut local = LocalPlayerInfo {
            move_seq: 10,
            ..default()
        };
        for seq in 1..=10 {
            local.committed_positions.record(seq, seq, Position::default());
        }
        assert!(local_snap_delta(&mut local, &comparison(4, 8.0, 0)).is_some());
        reset_local_comparisons(&mut local);
        for seq in 5..=10 {
            assert!(local_snap_delta(&mut local, &comparison(seq, 8.0, 0)).is_none());
        }
        local
            .committed_positions
            .record(11, 11, Position { x: 8.0, y: 0.0, z: 0.0 });
        assert!(local_snap_delta(&mut local, &comparison(11, 8.0, 1)).is_none());
    }

    #[test]
    fn comparison_sequence_wraps() {
        let mut local = LocalPlayerInfo {
            last_comparison_seq: Some(u32::MAX),
            ..default()
        };
        local.committed_positions.record(0, 10, Position::default());
        assert!(local_snap_delta(&mut local, &comparison(0, 8.0, 0)).is_some());
    }

    #[test]
    fn clock_uses_processing_tick_and_ignores_repeated_result() {
        let mut local = LocalPlayerInfo {
            move_seq: 1,
            ..default()
        };
        local.committed_positions.record(1, 10, Position::default());
        let mut clock = TickSync::default();
        let mut tick = ServerTick(11);
        let mut players = PlayerMap::default();
        sync_player_clock(20, 1, &local, &mut clock, &mut tick, &mut players);
        assert_eq!(tick.0, 21);
        sync_player_clock(20, 1, &local, &mut clock, &mut tick, &mut players);
        assert_eq!(tick.0, 21);
    }
}
