use std::collections::VecDeque;

use bevy::prelude::*;
use common::{
    constants::PLAYER_MOVEMENT_TRUST_DISTANCE,
    physics::{
        AirborneMomentum, CharacterSupport, CharacterVerticalVelocity, KnockbackVelocity, player_movement_state,
    },
    protocol::{
        CMove, CPortalCross, ClientMessage, FaceYaw, MovementDivergence, PlayerMoveIntent, PlayerMovementState,
        Position, ServerTick, sequence_is_newer,
    },
};

use crate::{
    network::{ClientToServer, ClientToServerChannel},
    players::{LocalPlayerInfo, LocalPlayerMarker},
};

const COMMITTED_POSITION_RING_LEN: usize = 64;

// What the local player has reported and still has to settle: the position
// recorded with each report, for the echo that compares it; the crossings
// awaiting the server's decision; and the cutoff below which echoes are
// stale. One owner, so every site that invalidates them does it alike.
#[derive(Default)]
pub struct LocalMovementReports {
    seq: u32,
    ring: CommittedPositionRing,
    last_comparison_seq: Option<u32>,
    unsent_crossing: Option<UnsentCrossing>,
    pending_crossings: VecDeque<PendingCrossing>,
}

struct UnsentCrossing {
    entrance: PlayerMovementState,
    view_change: Vec2,
}

struct PendingCrossing {
    seq: u32,
    view_change: Vec2,
}

pub enum CrossingResolution {
    Accepted,
    // The yaw and pitch the rejected crossing and everything after it turned the aim by.
    Rejected { undo_view: Vec2 },
    Unknown,
}

impl LocalMovementReports {
    // The newest sequence sent.
    #[must_use]
    pub fn seq(&self) -> u32 {
        self.seq
    }

    fn next_seq(&mut self) -> u32 {
        self.seq = self.seq.wrapping_add(1);
        self.seq
    }

    pub(crate) fn record(&mut self, seq: u32, tick: u32, pos: Position) {
        self.ring.record(seq, tick, pos);
    }

    // A crossing the next report will carry. The records before it are on
    // the other side of the pair, so they can no longer be compared.
    pub fn begin_crossing(&mut self, entrance: PlayerMovementState, view_change: Vec2) {
        self.unsent_crossing = Some(UnsentCrossing { entrance, view_change });
        self.invalidate();
    }

    pub(crate) fn take_crossing(&mut self, seq: u32) -> Option<PlayerMovementState> {
        let crossing = self.unsent_crossing.take()?;
        self.pending_crossings.push_back(PendingCrossing {
            seq,
            view_change: crossing.view_change,
        });
        Some(crossing.entrance)
    }

    #[must_use]
    pub fn crossing_pending(&self) -> bool {
        !self.pending_crossings.is_empty()
    }

    // Decisions arrive in order, so only the oldest pending crossing can be
    // the one decided. A rejection takes every later crossing with it.
    pub fn resolve_crossing(&mut self, seq: u32, accepted: bool) -> CrossingResolution {
        if self
            .pending_crossings
            .front()
            .is_none_or(|crossing| crossing.seq != seq)
        {
            return CrossingResolution::Unknown;
        }
        if accepted {
            self.pending_crossings.pop_front();
            return CrossingResolution::Accepted;
        }
        let undo_view = self.pending_crossings.iter().map(|crossing| crossing.view_change).sum();
        self.clear_crossings();
        CrossingResolution::Rejected { undo_view }
    }

    // The echo of a report, against the position recorded with it: `Some`
    // when the local prediction must snap. Nothing is compared while a
    // crossing is pending, since the echoes then straddle the pair.
    pub fn snap_divergence(&mut self, seq: u32, server_pos: Position) -> Option<MovementDivergence> {
        if self.crossing_pending() {
            return None;
        }
        if self
            .last_comparison_seq
            .is_some_and(|last| !sequence_is_newer(seq, last))
        {
            return None;
        }
        self.last_comparison_seq = Some(seq);
        let recorded = self.ring.get(seq)?.pos;
        let divergence = MovementDivergence::between(recorded, server_pos, PLAYER_MOVEMENT_TRUST_DISTANCE);
        (!divergence.within_limit()).then_some(divergence)
    }

    // The tick a report was simulated at, for clock correction. Withheld
    // while a crossing is pending: reports then queue on the reliable lane
    // and their bunched echoes would read as clock error.
    #[must_use]
    pub fn echo_tick(&self, seq: u32) -> Option<u32> {
        if self.crossing_pending() {
            return None;
        }
        self.ring.get(seq).map(|record| record.tick)
    }

    // Forget every comparison still in flight: after a snap or a teleport
    // the recorded positions describe a body that no longer exists.
    pub fn invalidate(&mut self) {
        self.ring.clear();
        self.last_comparison_seq = Some(self.seq);
    }

    pub fn clear_crossings(&mut self) {
        self.unsent_crossing = None;
        self.pending_crossings.clear();
    }

    #[cfg(test)]
    pub(crate) fn set_seq(&mut self, seq: u32) {
        self.seq = seq;
    }
}

// Sequences pair the local result with the server comparison; ticks also
// feed clock synchronization.
struct CommittedPositionRing([Option<CommittedPosition>; COMMITTED_POSITION_RING_LEN]);

#[derive(Clone, Copy)]
struct CommittedPosition {
    seq: u32,
    tick: u32,
    pos: Position,
}

impl CommittedPositionRing {
    fn record(&mut self, seq: u32, tick: u32, pos: Position) {
        self.0[Self::slot(seq)] = Some(CommittedPosition { seq, tick, pos });
    }

    fn get(&self, seq: u32) -> Option<CommittedPosition> {
        self.0[Self::slot(seq)].filter(|record| record.seq == seq)
    }

    fn clear(&mut self) {
        *self = Self::default();
    }

    fn slot(seq: u32) -> usize {
        seq as usize % COMMITTED_POSITION_RING_LEN
    }
}

impl Default for CommittedPositionRing {
    fn default() -> Self {
        Self([None; COMMITTED_POSITION_RING_LEN])
    }
}

// Reports the tick's movement after the step and any portal transit. The
// lane rule for reports behind a pending crossing is in the protocol header.
pub fn report_player_movement_system(
    to_server: Res<ClientToServerChannel>,
    tick: Res<ServerTick>,
    mut local: ResMut<LocalPlayerInfo>,
    query: Query<
        (
            &Position,
            &PlayerMoveIntent,
            &FaceYaw,
            &CharacterVerticalVelocity,
            &AirborneMomentum,
            &KnockbackVelocity,
            &CharacterSupport,
        ),
        With<LocalPlayerMarker>,
    >,
) {
    if local.is_dead {
        return;
    }
    let Ok((pos, intent, yaw, vertical, momentum, knockback, support)) = query.single() else {
        return;
    };
    let reports = &mut local.reports;
    let seq = reports.next_seq();
    let movement = player_movement_state(*pos, *intent, yaw, vertical, momentum, knockback, *support);
    reports.record(seq, tick.0, *pos);
    let command = match reports.take_crossing(seq) {
        Some(entrance) => ClientToServer::Send(ClientMessage::PortalCross(CPortalCross {
            seq,
            entrance,
            movement,
        })),
        None if reports.crossing_pending() => {
            ClientToServer::SendReliable(ClientMessage::Move(CMove { seq, movement }))
        }
        None => ClientToServer::Send(ClientMessage::Move(CMove { seq, movement })),
    };
    to_server.send(command);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{characters::PreviousTickPosition, players::PlayerMap, portals::portal_transit_system, test_fixtures};
    use common::{
        map::Carriers,
        physics::{CollisionWorld, PortalSet},
        protocol::{BarrierKindTable, CarrierId, MapLayout, PlayerId, PlayerMarker, Portal, PortalEnd, PortalPairId},
    };
    use std::f32::consts::PI;
    use tokio::sync::mpsc::unbounded_channel;

    const FAR: f32 = PLAYER_MOVEMENT_TRUST_DISTANCE * 2.0;

    fn movement() -> PlayerMovementState {
        PlayerMovementState::new(Position::default(), PlayerMoveIntent::Idle, 0.0, 0.0)
    }

    fn at(x: f32) -> Position {
        Position { x, y: 0.0, z: 0.0 }
    }

    #[test]
    fn a_recorded_report_snaps_only_past_the_trust_distance() {
        let mut reports = LocalMovementReports::default();
        reports.record(1, 10, Position::default());
        assert!(
            reports
                .snap_divergence(1, at(PLAYER_MOVEMENT_TRUST_DISTANCE - 0.01))
                .is_none()
        );
        reports.record(2, 11, Position::default());
        let divergence = reports
            .snap_divergence(2, at(PLAYER_MOVEMENT_TRUST_DISTANCE))
            .expect("divergence at the limit missing");
        assert_eq!(divergence.delta, Vec3::X * PLAYER_MOVEMENT_TRUST_DISTANCE);
    }

    #[test]
    fn pending_crossings_suspend_snaps_until_all_are_confirmed() {
        let mut reports = LocalMovementReports::default();
        reports.begin_crossing(movement(), Vec2::ZERO);
        reports.take_crossing(1);
        reports.begin_crossing(movement(), Vec2::ZERO);
        reports.take_crossing(2);
        reports.record(3, 10, Position::default());
        assert!(reports.snap_divergence(3, at(FAR)).is_none());
        assert!(reports.echo_tick(3).is_none());
        assert!(matches!(
            reports.resolve_crossing(1, true),
            CrossingResolution::Accepted
        ));
        assert!(reports.snap_divergence(3, at(FAR)).is_none());
        assert!(matches!(
            reports.resolve_crossing(2, true),
            CrossingResolution::Accepted
        ));
        assert_eq!(reports.echo_tick(3), Some(10));
        assert!(reports.snap_divergence(3, at(FAR)).is_some());
    }

    #[test]
    fn repeated_outdated_and_unrecorded_echoes_do_not_snap() {
        let mut reports = LocalMovementReports::default();
        reports.record(2, 10, Position::default());
        assert!(reports.snap_divergence(2, at(FAR)).is_some());
        assert!(reports.snap_divergence(2, at(FAR)).is_none());
        assert!(reports.snap_divergence(1, at(FAR)).is_none());
        assert!(reports.snap_divergence(3, at(FAR)).is_none());
    }

    #[test]
    fn invalidation_ignores_in_flight_reports_then_accepts_fresh_echoes() {
        let mut reports = LocalMovementReports::default();
        for _ in 0..10 {
            let seq = reports.next_seq();
            reports.record(seq, seq, Position::default());
        }
        assert!(reports.snap_divergence(4, at(FAR)).is_some());
        reports.invalidate();
        for seq in 5..=10 {
            assert!(reports.snap_divergence(seq, at(FAR)).is_none());
            assert!(reports.echo_tick(seq).is_none());
        }
        let seq = reports.next_seq();
        reports.record(seq, seq, at(FAR));
        assert!(reports.snap_divergence(seq, at(FAR)).is_none());
    }

    #[test]
    fn comparison_sequence_wraps() {
        let mut reports = LocalMovementReports {
            seq: u32::MAX,
            ..default()
        };
        reports.invalidate();
        reports.record(0, 10, Position::default());
        assert!(reports.snap_divergence(0, at(FAR)).is_some());
    }

    #[test]
    fn an_echo_older_than_the_ring_neither_snaps_nor_corrects_the_clock() {
        let mut reports = LocalMovementReports::default();
        reports.record(7, 3, Position::default());
        reports.record(7 + COMMITTED_POSITION_RING_LEN as u32, 4, Position::default());
        assert!(reports.echo_tick(7).is_none());
        assert!(reports.snap_divergence(7, at(FAR)).is_none());
        assert_eq!(reports.echo_tick(7 + COMMITTED_POSITION_RING_LEN as u32), Some(4));
    }

    #[test]
    fn a_rejection_undoes_every_pending_view_change_and_an_unknown_seq_is_ignored() {
        let mut reports = LocalMovementReports::default();
        reports.begin_crossing(movement(), Vec2::new(1.0, 0.1));
        reports.take_crossing(10);
        reports.begin_crossing(movement(), Vec2::new(0.4, 0.1));
        reports.take_crossing(12);
        assert!(matches!(
            reports.resolve_crossing(12, false),
            CrossingResolution::Unknown
        ));
        let CrossingResolution::Rejected { undo_view } = reports.resolve_crossing(10, false) else {
            panic!("the oldest pending crossing was not rejected")
        };
        assert!((undo_view - Vec2::new(1.4, 0.2)).length() < 1e-6);
        assert!(!reports.crossing_pending());
        assert!(matches!(
            reports.resolve_crossing(10, false),
            CrossingResolution::Unknown
        ));
    }

    #[test]
    fn clearing_crossings_returns_reports_to_the_unreliable_lane_and_re_enables_snaps() {
        let mut reports = LocalMovementReports::default();
        reports.begin_crossing(movement(), Vec2::ZERO);
        reports.take_crossing(1);
        reports.record(2, 10, Position::default());
        assert!(reports.crossing_pending());
        assert!(reports.snap_divergence(2, at(FAR)).is_none());
        reports.clear_crossings();
        assert!(!reports.crossing_pending());
        assert!(reports.snap_divergence(2, at(FAR)).is_some());
    }

    #[test]
    fn only_local_players_cross_and_followup_moves_stay_reliable_until_confirmation() {
        let mut app = App::new();
        let (sender, mut receiver) = unbounded_channel();
        let collision = CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default());
        let portals: Vec<_> = [PortalEnd::A, PortalEnd::B]
            .into_iter()
            .enumerate()
            .map(|(i, end)| Portal {
                pair: PortalPairId(1),
                end,
                pos: Position {
                    x: i as f32 * 10.0,
                    y: 1.6,
                    z: 0.0,
                },
                nx: 0.0,
                ny: 0.0,
                nz: 1.0,
                yaw: 0.0,
                carrier: CarrierId::WORLD,
            })
            .collect();
        let portal_set = PortalSet::rebuild(&portals, &collision, &Carriers::default());
        app.insert_resource(portal_set)
            .insert_resource(test_fixtures::gameplay_config())
            .insert_resource(test_fixtures::map_settings())
            .init_resource::<LocalPlayerInfo>()
            .init_resource::<PlayerMap>()
            .insert_resource(ServerTick(10))
            .insert_resource(ClientToServerChannel::new(sender))
            .add_systems(Update, (portal_transit_system, report_player_movement_system).chain());
        let entrance = Position {
            x: 0.0,
            y: 0.7,
            z: -0.05,
        };
        let spawn = |world: &mut World, id| {
            world
                .spawn((
                    PlayerId(id),
                    PlayerMarker,
                    entrance,
                    PreviousTickPosition(Position { z: 0.15, ..entrance }),
                    PlayerMoveIntent::Running { direction: PI },
                    FaceYaw(PI),
                    CharacterVerticalVelocity(-2.0),
                    AirborneMomentum(Vec3::X * 4.0),
                    KnockbackVelocity(Vec3::Z * 2.0),
                    CharacterSupport::Airborne,
                ))
                .id()
        };
        let local = spawn(app.world_mut(), 1);
        app.world_mut().entity_mut(local).insert(LocalPlayerMarker);
        let remote = spawn(app.world_mut(), 2);
        app.update();
        let ClientToServer::Send(ClientMessage::PortalCross(crossing)) = receiver.try_recv().expect("crossing missing")
        else {
            panic!("expected crossing event")
        };
        assert_eq!(crossing.seq, 1);
        assert_eq!(crossing.entrance.pos, entrance);
        assert_eq!(
            crossing.entrance.move_intent,
            PlayerMoveIntent::Running { direction: PI }
        );
        assert!((crossing.movement.pos.x - 10.0).abs() < 1e-5);
        assert!((crossing.movement.airborne_momentum[0] + 4.0).abs() < 1e-4);
        assert!((crossing.movement.knockback[2] + 2.0).abs() < 1e-4);
        assert_eq!(
            *app.world().get::<Position>(remote).expect("remote position missing"),
            entrance
        );
        assert!(receiver.try_recv().is_err());
        assert!(app.world().resource::<LocalPlayerInfo>().reports.crossing_pending());
        app.world_mut()
            .get_mut::<Position>(local)
            .expect("local position missing")
            .z += 1.0;
        app.update();
        assert!(matches!(
            receiver.try_recv(),
            Ok(ClientToServer::SendReliable(ClientMessage::Move(CMove { seq: 2, .. })))
        ));
        app.world_mut()
            .resource_mut::<LocalPlayerInfo>()
            .reports
            .resolve_crossing(1, true);
        app.update();
        assert!(matches!(
            receiver.try_recv(),
            Ok(ClientToServer::Send(ClientMessage::Move(CMove { seq: 3, .. })))
        ));
        let position = *app.world().get::<Position>(local).expect("position missing");
        let reports = &app.world().resource::<LocalPlayerInfo>().reports;
        assert_eq!(reports.ring.get(3).expect("record missing").pos, position);
        assert_eq!(reports.echo_tick(3), Some(10));
    }
}
