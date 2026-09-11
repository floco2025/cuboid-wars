use super::*;
use crate::{
    network::{RETRANSMITTED_CHANNEL, SLICE_BYTES, UNRELIABLE_CHANNEL, channel_for, encode_message},
    physics::CharacterSupport,
    protocol::CarrierId,
};

fn position() -> Position {
    Position {
        x: 12.5,
        y: 3.0,
        z: -7.25,
    }
}

fn barrier_kind_cap() -> u16 {
    u16::try_from(BarrierKindId::MAX.expect("barrier kind datagram cap missing")).expect("barrier kind cap exceeds u16")
}

#[test]
fn the_checkpoint_cue_rides_the_reliable_lane() {
    assert_eq!(
        ServerMessage::CheckpointReached(SCheckpointReached).lane(),
        Lane::Reliable
    );
}

#[test]
fn unreliable_lane_messages_fit_one_packet() {
    let messages = [
        ServerMessage::EquipmentErased(SEquipmentErased),
        ServerMessage::PlayerStatus(SPlayerStatus {
            id: PlayerId(1),
            generation: PlayerGeneration(0),
            collected: Some(ItemType::SpeedPowerUp),
            power_ups: [true; PowerUpKind::COUNT],
            stunned: true,
            held_keys: (0..barrier_kind_cap()).map(BarrierKindId).collect(),
            missiles: 0,
        }),
        ServerMessage::PortalOpened(SPortalOpened {
            shooter: PlayerId(1),
            portal: Portal {
                pair: PortalPairId(1),
                end: PortalEnd::A,
                pos: position(),
                nx: 0.0,
                ny: 1.0,
                nz: 0.0,
                yaw: 0.5,
                carrier: CarrierId::WORLD,
            },
        }),
        ServerMessage::PlayerDeath(SPlayerDeath {
            id: PlayerId(1),
            generation: PlayerGeneration(0),
            pos: position(),
            killer: Some(PlayerId(2)),
            victim_score: -1000,
            killer_score: Some(200),
            effect: PlayerDeathEffect::Explosion,
        }),
        ServerMessage::ProjectileShot(SProjectileShot {
            id: PlayerId(1),
            shot: CProjectileShot {
                origin: position(),
                face_yaw: 1.0,
                face_pitch: 0.1,
                pattern: 1,
            },
        }),
        ServerMessage::ActorBeam(SActorBeam {
            id: ActorId(3),
            tick: u32::MAX,
            beam: Some(ActorBeam {
                target: PlayerId(1),
                started_tick: u32::MAX,
                remaining_secs: 2.0,
            }),
        }),
        ServerMessage::ActorBeam(SActorBeam {
            id: ActorId(3),
            tick: 0,
            beam: None,
        }),
    ];
    for message in &messages {
        assert_eq!(message.lane(), Lane::Unreliable, "{message:?}");
        let len = encode_message(message).expect("message failed to encode").len();
        assert!(len <= SLICE_BYTES, "{message:?} encodes to {len} bytes");
        assert_eq!(channel_for(message.lane(), len), UNRELIABLE_CHANNEL);
    }
}

#[test]
fn reliable_lane_carries_bootstrap_events_and_text() {
    assert_eq!(ServerMessage::Feed(SFeed { spans: Vec::new() }).lane(), Lane::Reliable);
    assert_eq!(ServerMessage::Firework(SFirework { seed: 7 }).lane(), Lane::Reliable);
    assert_eq!(
        ServerMessage::QuestUpdates(SQuestUpdates { updates: Vec::new() }).lane(),
        Lane::Reliable
    );
    assert_eq!(
        ServerMessage::PlayerKnockback(SPlayerKnockback {
            id: PlayerId(1),
            generation: PlayerGeneration(0),
            health: Health(10.0),
            impulse: [1.0, 7.0, -1.0],
        })
        .lane(),
        Lane::Reliable
    );
    assert_eq!(
        ClientMessage::Login(CLogin { name: String::new() }).lane(),
        Lane::Reliable
    );
    assert_eq!(
        ClientMessage::Ping(CPing { timestamp_nanos: 0 }).lane(),
        Lane::Unreliable
    );
}

#[test]
fn movement_is_unreliable() {
    let movement = PlayerMovementState::new(position(), PlayerMoveIntent::Idle, 0.0, 0.0);
    assert_eq!(
        ClientMessage::Move(CMove {
            generation: PlayerGeneration(0),
            seq: 1,
            portal_crossing: 0,
            movement
        })
        .lane(),
        Lane::Unreliable
    );
    assert_eq!(
        ClientMessage::Ping(CPing { timestamp_nanos: 0 }).lane(),
        Lane::Unreliable
    );
}

#[test]
fn sequence_comparison_wraps() {
    assert!(sequence_is_newer(2, 1));
    assert!(!sequence_is_newer(1, 2));
    assert!(!sequence_is_newer(5, 5));
    assert!(sequence_is_newer(0, u32::MAX));
    assert!(!sequence_is_newer(u32::MAX, 0));
}

#[test]
fn hotel_sized_snapshot_takes_the_retransmitted_channel() {
    let player = |i: u32| {
        (
            PlayerId(i),
            Player {
                generation: PlayerGeneration(0),
                name: format!("Player {i}"),
                movement: PlayerMovementState::new(position(), PlayerMoveIntent::Idle, 0.0, 0.0),
                health: Health(500.0),
                score: 0,
                power_ups: [false; PowerUpKind::COUNT],
                stunned: false,
                held_keys: Vec::new(),
                missiles: 0,
                portal_access: PortalAccess::None,
            },
        )
    };
    let actor = |i: u32| {
        (
            ActorId(i),
            Actor {
                beam: None,
                kind: "bruiser".to_owned(),
                movement: ActorMovementState {
                    pos: position(),
                    carrier: CarrierId::WORLD,
                    move_intent: ActorMoveIntent::Idle,
                    vertical_velocity: 0.0,
                    face_yaw: 0.0,
                    support: CharacterSupport::Ground,
                },
                health: Health(1000.0),
            },
        )
    };
    let item = |i: u32| {
        (
            ItemId(i),
            Item {
                item_type: ItemType::Gold,
                carrier: CarrierId::WORLD,
                pos: position(),
            },
        )
    };
    let snapshot = ServerMessage::Snapshot(SSnapshot {
        tick: 1,
        players: (0..4).map(player).collect(),
        actors: (0..24).map(actor).collect(),
        actors_peaceful: false,
        spawning_actors: Vec::new(),
        items: (0..74).map(item).collect(),
        missiles: Vec::new(),
        plates: PlateState::default(),
        quests: Vec::new(),
        locked_switches: Vec::new(),
        rain_intensity: 0.0,
        lighting: LightingBlend {
            from: "bright".to_owned(),
            to: "bright".to_owned(),
            blend: 0.0,
        },
        portals: Vec::new(),
    });
    let len = encode_message(&snapshot).expect("snapshot failed to encode").len();
    assert!(len > SLICE_BYTES, "hotel-sized snapshot encodes to {len} bytes");
    assert_eq!(channel_for(snapshot.lane(), len), RETRANSMITTED_CHANNEL);
}
