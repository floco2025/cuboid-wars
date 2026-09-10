use super::super::context::ServerMessageContext;
use crate::{
    characters::PreviousTickPosition,
    constants::RECON_CHARACTER_SNAP_DISTANCE,
    network::{RoundTripTime, ServerReconciliation, TickSync, extrapolated_correction, resources::accept_newer_tick},
    players::{LocalPlayerInfo, PlayerInfo},
};
use bevy::prelude::*;
use common::{
    config::MapMovementConfig,
    physics::{PlayerMotionBundle, player_control_velocity},
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
        );
    }
    for entry in message.moves {
        let Some(player) = context.players.get_mut(&entry.id) else {
            continue;
        };
        if entry.id == my_player_id {
            let local = &mut context.local_player_info;
            if local.is_dead {
                continue;
            }
            if let Some(seq) = entry.move_seq
                && let Some(divergence) = local.reports.snap_divergence(seq, entry.movement.pos)
            {
                warn!(
                    "{}#{}, move {seq} out of sync, {divergence}; snapping to server position",
                    player.name, entry.id.0
                );
                snap_player(commands, player, &entry.movement);
                local.reports.invalidate();
            }
            continue;
        }
        if !sequence_is_newer(tick, player.last_movement_tick) {
            continue;
        }
        player.last_movement_tick = tick;
        let velocity = player_movement_velocity(
            entry.movement,
            &context.map_settings.movement,
            player.power_up(PowerUpKind::Speed),
            player.stunned,
        );
        let current = context.player_data.get(player.entity).ok().copied();
        place_remote_player(
            commands,
            entry.id,
            player,
            &entry.movement,
            velocity,
            current,
            &context.rtt,
        );
    }
}

// A remote update smooths toward its position when the gap is drift and
// cuts to it when it is not: under trust an accepted jump this large is a
// portal, a respawn, or a relocation, and the smoothing would push the
// body through whatever stands between.
fn place_remote_player(
    commands: &mut Commands,
    id: PlayerId,
    player: &PlayerInfo,
    movement: &PlayerMovementState,
    velocity: Vec3,
    current: Option<Position>,
    rtt: &RoundTripTime,
) {
    let correction = current.map(|current| extrapolated_correction(current, movement.pos, velocity, rtt));
    let divergence = correction.map(|delta| MovementDivergence {
        delta,
        limit: RECON_CHARACTER_SNAP_DISTANCE,
    });
    match divergence {
        Some(divergence) if !divergence.within_limit() => {
            debug!("{}#{} cut to server position, {divergence}", player.name, id.0);
            snap_player(commands, player, movement);
        }
        _ => {
            let mut entity = commands.entity(player.entity);
            entity.insert(PlayerMotionBundle::from(movement));
            if let Some(correction) = correction {
                entity.insert(ServerReconciliation::new(correction, movement.pos, velocity, rtt));
            }
        }
    }
}

pub(in crate::network) fn snap_player(commands: &mut Commands, info: &PlayerInfo, movement: &PlayerMovementState) {
    commands
        .entity(info.entity)
        .insert((
            movement.pos,
            PreviousTickPosition(movement.pos),
            PlayerMotionBundle::from(movement),
        ))
        .remove::<ServerReconciliation>();
}

fn sync_player_clock(
    tick: u32,
    move_seq: u32,
    local_player_info: &LocalPlayerInfo,
    tick_sync: &mut TickSync,
    server_tick: &mut ServerTick,
) {
    let Some(recorded_tick) = local_player_info.reports.echo_tick(move_seq) else {
        return;
    };
    let error = tick.wrapping_sub(recorded_tick) as i32;
    trace!("clock error {error} ticks at the echo of seq {move_seq}");
    if let Some(shift) = tick_sync.observe(error, move_seq, local_player_info.reports.seq()) {
        server_tick.0 = server_tick.0.wrapping_add_signed(shift);
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
    use bevy::ecs::system::SystemState;
    use common::physics::CharacterVerticalVelocity;
    use std::time::Duration;

    #[test]
    fn clock_uses_processing_tick_and_ignores_repeated_result() {
        let mut local = LocalPlayerInfo::default();
        local.reports.record(1, 10, Position::default());
        let mut clock = TickSync::default();
        let mut tick = ServerTick(11);
        sync_player_clock(20, 1, &local, &mut clock, &mut tick);
        assert_eq!(tick.0, 21);
        sync_player_clock(20, 1, &local, &mut clock, &mut tick);
        assert_eq!(tick.0, 21);
    }

    #[test]
    fn remote_updates_smooth_drift_and_cut_to_larger_jumps() {
        for (gap, cut) in [(1.0, false), (RECON_CHARACTER_SNAP_DISTANCE, true)] {
            let mut world = World::new();
            let entity = world
                .spawn((Position::default(), PreviousTickPosition(Position::default())))
                .id();
            let player = PlayerInfo::from_snapshot(
                entity,
                &Player::new(
                    "Player".into(),
                    Position::default(),
                    PlayerMoveIntent::Idle,
                    0.0,
                    0,
                    Health(100.0),
                ),
                0,
            );
            let movement =
                PlayerMovementState::new(Position { x: gap, ..default() }, PlayerMoveIntent::Idle, -3.0, 1.0);
            let rtt = RoundTripTime {
                rtt: Duration::from_millis(0),
                ..default()
            };
            let mut state = SystemState::<Commands>::new(&mut world);
            place_remote_player(
                &mut state.get_mut(&mut world).expect("commands unavailable"),
                PlayerId(2),
                &player,
                &movement,
                Vec3::ZERO,
                Some(Position::default()),
                &rtt,
            );
            state.apply(&mut world);
            let position = *world.get::<Position>(entity).expect("position missing");
            assert_eq!(position.x, if cut { gap } else { 0.0 }, "gap {gap}");
            assert_eq!(world.get::<ServerReconciliation>(entity).is_some(), !cut);
            assert_eq!(
                world
                    .get::<CharacterVerticalVelocity>(entity)
                    .expect("vertical velocity missing")
                    .0,
                -3.0
            );
        }
    }
}
