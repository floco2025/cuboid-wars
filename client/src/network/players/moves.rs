use bevy::prelude::*;

use super::super::context::ServerMessageContext;
use crate::{
    characters::PreviousTickPosition,
    network::{
        RoundTripTime, ServerReconciliation, TickSync, extrapolated_correction, recorded_correction,
        resources::accept_newer_tick,
    },
    players::{CommittedPositionRing, CrossingVerdict, LocalPlayerInfo, PlayerInfo, PlayerMap},
};
use common::{
    config::MapMovementConfig,
    physics::{AirborneMomentum, CharacterVerticalVelocity, player_control_velocity},
    protocol::*,
};

// What one entry of the move stream does to our simulation of that player.
// Remote players are steered (intent, facing, vertical velocity) by the
// state; the local player's are its own, so only the reconciliation, or the
// teleport with its ring reset, applies to it.
enum PlayerSteering {
    Skip,
    Teleport {
        steer_remote: bool,
        clear_ring: bool,
    },
    Reconcile {
        steer_remote: bool,
        reconciliation: Option<ServerReconciliation>,
    },
}

// This tick's movement state of every player, with server reconciliation.
pub(in crate::network) fn handle_player_moves_message(
    message: SPlayerMoves,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut ServerMessageContext,
) {
    trace!("moves: {:?}", message);
    if !accept_newer_tick(&mut context.clocks.last_player_moves_tick.0, message.tick) {
        debug!(
            "ignoring outdated player moves (tick {}, last {:?})",
            message.tick, context.clocks.last_player_moves_tick.0
        );
        return;
    }
    let tick = message.tick;
    if context.clocks.tick_sync.takes_rough_seed() {
        context.clocks.server_tick.0 = tick.wrapping_add(1);
    }
    // Measure the clock before rejecting disputed portal crossings, or a startup misprediction can block its own recovery.
    if let Some(own_move) = message.moves.iter().find(|movement| movement.id == my_player_id) {
        sync_player_clock(
            tick,
            own_move.move_seq,
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
        let ours = player.hops;
        let client_pos = context.player_data.get(player.entity).ok().copied();
        let steering = steer_player(
            player,
            &context.local_player_info.committed_positions,
            entry.id == my_player_id,
            tick,
            clock_seeded,
            &entry,
            client_pos,
            &context.map_settings.movement,
            &context.rtt,
        );
        let movement = entry.movement;
        let mut entity = commands.entity(player.entity);
        match steering {
            PlayerSteering::Skip => {}
            PlayerSteering::Teleport {
                steer_remote,
                clear_ring,
            } => {
                // The server's side stands: put the player there outright. The
                // gap between two portals can sit under the snap threshold, and a
                // vertical gap is never eased, so ordinary reconciliation could
                // leave the player on the wrong side with the right count. The
                // local player's view stays mapped through the rejected crossing
                // on purpose: intent is rebuilt every frame from keys and camera,
                // knockback decays within a second, and unmapping the view would
                // need the last crossing's pair, for a case that takes the two
                // simulations drifting apart right at an aperture's edge. The
                // teleport lands when commands flush, so a second move message
                // in this same receive pass still measures against the old
                // position; that costs one tick of a fraction of the gap before
                // the next echo measures against the teleported one.
                warn!(
                    "{} crossing dispute settled for the server: {} hops there, {} here; teleporting",
                    player.name, entry.hops, ours
                );
                entity
                    .insert((
                        movement.pos,
                        PreviousTickPosition(movement.pos),
                        CharacterVerticalVelocity(movement.vertical_velocity),
                        AirborneMomentum::default(),
                    ))
                    .remove::<ServerReconciliation>();
                if steer_remote {
                    entity.insert((movement.move_intent, FaceYaw(movement.face_yaw)));
                }
                if clear_ring {
                    context.local_player_info.committed_positions.clear();
                }
            }
            PlayerSteering::Reconcile {
                steer_remote,
                reconciliation,
            } => {
                if steer_remote {
                    entity.insert((
                        movement.move_intent,
                        FaceYaw(movement.face_yaw),
                        CharacterVerticalVelocity(movement.vertical_velocity),
                    ));
                }
                if let Some(reconciliation) = reconciliation {
                    entity.insert(reconciliation);
                }
            }
        }
    }
}

// A state pairs only with a simulation on the same side of the same portal
// crossings; one from across a crossing we have predicted, or not yet
// predicted, would steer and reconcile the body back through. Which side is
// right is judged by tick (`judge_crossing`). `client_pos` is `None` for a
// body not yet materialized, which then gets no reconciliation.
fn steer_player(
    info: &mut PlayerInfo,
    ring: &CommittedPositionRing,
    is_local: bool,
    tick: u32,
    clock_seeded: bool,
    entry: &PlayerMove,
    client_pos: Option<Position>,
    map_movement: &MapMovementConfig,
    rtt: &RoundTripTime,
) -> PlayerSteering {
    let steer_remote = !is_local;
    match info.judge_crossing(tick, entry.hops, clock_seeded) {
        CrossingVerdict::Paired => {}
        CrossingVerdict::Skipped => return PlayerSteering::Skip,
        CrossingVerdict::Settled => {
            return PlayerSteering::Teleport {
                steer_remote,
                clear_ring: is_local,
            };
        }
    }
    let reconciliation = client_pos.map(|client_pos| {
        let server_velocity = player_movement_velocity(
            entry.movement,
            map_movement,
            info.power_up(PowerUpKind::Speed),
            info.stunned,
        );
        let correction_delta = if is_local {
            // Own state names the `CMove` it reflects: measure against where
            // our simulation stood after that `CMove`. One the ring does not
            // hold (before the first commit, after a snap, or after a one-way
            // stall longer than the ring) is measured against where we stand
            // now, the plain gap to the server.
            let recorded = ring.get(entry.move_seq, entry.hops);
            let recorded_pos = recorded.map_or(client_pos, |recorded| recorded.pos);
            recorded_correction(recorded_pos, entry.movement.pos)
        } else {
            extrapolated_correction(client_pos, entry.movement.pos, server_velocity, rtt)
        };
        ServerReconciliation::new(correction_delta, entry.movement.pos, server_velocity, rtt)
    });
    PlayerSteering::Reconcile {
        steer_remote,
        reconciliation,
    }
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
    velocity.y = movement.vertical_velocity;
    velocity
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::RECON_PLAYER_HOP_DISPUTE_SLACK_TICKS;
    use common::config::{KnockbackConfig, PlayerMovementConfig};
    use std::collections::HashMap;

    fn movement_config() -> MapMovementConfig {
        MapMovementConfig {
            player: PlayerMovementConfig {
                walk_speed: 6.0,
                run_speed: 9.0,
                speed_power_up: 1.6,
                jump_speed: 12.0,
            },
            actors: HashMap::new(),
            missile_speed: 16.0,
            projectile_speed: 90.0,
            gravity: 25.0,
            low_gravity: 5.0,
            ladder_climb_ratio: 0.4,
            knockback: KnockbackConfig {
                max_speed: 15.0,
                up_speed: 7.0,
                deceleration: 35.0,
            },
        }
    }

    fn player_info(entity: Entity, name: &str) -> PlayerInfo {
        PlayerInfo {
            entity,
            score: 0,
            name: name.to_owned(),
            power_ups: [false; PowerUpKind::COUNT],
            stunned: false,
            snap_speed: 0.0,
            held_keys: Vec::new(),
            missiles: 0,
            hops: 0,
            hop_tick: 0,
            disputed_since: None,
        }
    }

    #[test]
    fn first_echo_measures_clock_during_a_crossing_dispute() {
        let id = PlayerId(7);
        let mut player = player_info(Entity::PLACEHOLDER, "Alice");
        player.hops = 1;
        player.hop_tick = 10;
        let mut players = PlayerMap::default();
        players.insert(id, player);
        let mut local = LocalPlayerInfo {
            move_seq: 1,
            ..default()
        };
        local.committed_positions.record(1, 1, 10, Position::default());
        let mut clock = TickSync::default();
        assert!(clock.takes_rough_seed());
        let mut tick = ServerTick(11);

        sync_player_clock(20, 1, &local, &mut clock, &mut tick, &mut players);

        assert!(clock.is_seeded());
        assert_eq!(tick.0, 21);
        assert!(local.committed_positions.get(1, 0).is_none());
        let player = players.get_mut(&id).expect("local player missing from the map");
        assert_eq!(player.hop_tick, 20);
        assert_eq!(
            player.judge_crossing(19, 0, clock.is_seeded()),
            CrossingVerdict::Skipped
        );
        assert_eq!(player.disputed_since, None);
        assert_eq!(
            player.judge_crossing(20, 0, clock.is_seeded()),
            CrossingVerdict::Skipped
        );
        assert_eq!(
            player.judge_crossing(20 + RECON_PLAYER_HOP_DISPUTE_SLACK_TICKS, 0, clock.is_seeded()),
            CrossingVerdict::Settled
        );
        assert_eq!(player.hops, 0);
    }

    fn player_move(id: PlayerId, pos: Position, move_seq: u32, hops: u32) -> PlayerMove {
        PlayerMove {
            id,
            movement: PlayerMovementState::new(pos, PlayerMoveIntent::Idle, 0.0, 0.0),
            move_seq,
            hops,
        }
    }

    // Disputes the player's crossing count until the slack settles it for the server.
    fn settle(info: &mut PlayerInfo, entry: &PlayerMove, is_local: bool) -> PlayerSteering {
        let ring = CommittedPositionRing::default();
        let map_movement = movement_config();
        let rtt = RoundTripTime::default();
        let client_pos = Some(Position::default());
        let opened = steer_player(info, &ring, is_local, 200, true, entry, client_pos, &map_movement, &rtt);
        assert!(matches!(opened, PlayerSteering::Skip));
        let settled_tick = 200 + RECON_PLAYER_HOP_DISPUTE_SLACK_TICKS;
        steer_player(
            info,
            &ring,
            is_local,
            settled_tick,
            true,
            entry,
            client_pos,
            &map_movement,
            &rtt,
        )
    }

    #[test]
    fn remote_settled_state_teleports_with_intent_and_keeps_the_ring() {
        let mut info = player_info(Entity::PLACEHOLDER, "Alice");
        info.hop_tick = 50;
        let entry = player_move(PlayerId(2), Position::default(), 1, 1);
        assert!(matches!(
            settle(&mut info, &entry, false),
            PlayerSteering::Teleport {
                steer_remote: true,
                clear_ring: false,
            }
        ));
        assert_eq!(info.hops, 1);
    }

    #[test]
    fn local_settled_state_teleports_without_intent_and_clears_the_ring() {
        let mut info = player_info(Entity::PLACEHOLDER, "Alice");
        info.hop_tick = 50;
        let entry = player_move(PlayerId(7), Position::default(), 1, 1);
        assert!(matches!(
            settle(&mut info, &entry, true),
            PlayerSteering::Teleport {
                steer_remote: false,
                clear_ring: true,
            }
        ));
    }

    #[test]
    fn paired_local_state_measures_against_the_recorded_position_only_with_matching_hops() {
        let mut info = player_info(Entity::PLACEHOLDER, "Alice");
        let mut ring = CommittedPositionRing::default();
        ring.record(5, 0, 100, Position { x: 1.0, y: 0.0, z: 0.0 });
        let server_pos = Position { x: 3.0, y: 0.0, z: 0.0 };
        let client_pos = Some(Position {
            x: 10.0,
            y: 0.0,
            z: 0.0,
        });
        let map_movement = movement_config();
        let rtt = RoundTripTime::default();

        let recorded = player_move(PlayerId(7), server_pos, 5, 0);
        let PlayerSteering::Reconcile {
            steer_remote: false,
            reconciliation: Some(reconciliation),
        } = steer_player(
            &mut info,
            &ring,
            true,
            101,
            true,
            &recorded,
            client_pos,
            &map_movement,
            &rtt,
        )
        else {
            panic!("paired local state did not reconcile");
        };
        assert_eq!(reconciliation.correction_delta, Vec3::new(2.0, 0.0, 0.0));

        let unrecorded = player_move(PlayerId(7), server_pos, 6, 0);
        let PlayerSteering::Reconcile {
            reconciliation: Some(reconciliation),
            ..
        } = steer_player(
            &mut info,
            &ring,
            true,
            102,
            true,
            &unrecorded,
            client_pos,
            &map_movement,
            &rtt,
        )
        else {
            panic!("paired local state did not reconcile");
        };
        assert_eq!(reconciliation.correction_delta, Vec3::new(-7.0, 0.0, 0.0));
    }

    #[test]
    fn paired_remote_state_steers_and_needs_a_position_to_reconcile() {
        let mut info = player_info(Entity::PLACEHOLDER, "Alice");
        let ring = CommittedPositionRing::default();
        let entry = player_move(PlayerId(2), Position { x: 3.0, y: 0.0, z: 0.0 }, 5, 0);
        let map_movement = movement_config();
        let rtt = RoundTripTime::default();
        let client_pos = Some(Position {
            x: 10.0,
            y: 0.0,
            z: 0.0,
        });

        let PlayerSteering::Reconcile {
            steer_remote: true,
            reconciliation: Some(reconciliation),
        } = steer_player(
            &mut info,
            &ring,
            false,
            101,
            true,
            &entry,
            client_pos,
            &map_movement,
            &rtt,
        )
        else {
            panic!("paired remote state did not reconcile");
        };
        assert_eq!(reconciliation.correction_delta, Vec3::new(-7.0, 0.0, 0.0));
        assert!(matches!(
            steer_player(&mut info, &ring, false, 102, true, &entry, None, &map_movement, &rtt),
            PlayerSteering::Reconcile {
                steer_remote: true,
                reconciliation: None,
            }
        ));
    }

    #[test]
    fn disabled_player_reconciliation_velocity_is_vertical_only() {
        let map_movement = movement_config();
        let movement = PlayerMovementState::new(
            Position::default(),
            PlayerMoveIntent::Running { direction: 0.0 },
            -3.0,
            0.0,
        );

        assert_eq!(
            player_movement_velocity(movement, &map_movement, true, true),
            Vec3::new(0.0, -3.0, 0.0)
        );
    }
}
