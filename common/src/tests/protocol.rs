use super::*;
use crate::{
    network::{SLICE_BYTES, UNRELIABLE_CHANNEL, channel_for, encode_message},
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
    u16::try_from(FieldId::MAX.expect("barrier kind datagram cap missing")).expect("barrier kind cap exceeds u16")
}

#[test]
fn unreliable_lane_messages_fit_one_packet() {
    let messages = [
        ServerMessage::PlayerSoftLanding(SPlayerSoftLanding {
            id: PlayerId(1),
            generation: PlayerGeneration(0),
        }),
        ServerMessage::EquipmentErased(SEquipmentErased),
        ServerMessage::PlayerStatus(SPlayerStatus {
            id: PlayerId(1),
            generation: PlayerGeneration(0),
            collected: Some(ItemType::SpeedPowerUp),
            power_ups: [true; PowerUpKind::COUNT],
            stunned: true,
            held_keys: (0..barrier_kind_cap()).map(FieldId).collect(),
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
            effect: PlayerDeathEffect::Explosion {
                center: Position::default(),
            },
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
