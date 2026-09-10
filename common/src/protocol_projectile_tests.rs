use super::*;
use crate::network::encode_message;

#[test]
fn bullet_volleys_stay_compact_and_unreliable_while_hits_are_reliable() {
    for pattern in [0, 1, u8::MAX] {
        let shot = CProjectileShot {
            origin: Position {
                x: 43.0,
                y: 9.0,
                z: -15.0,
            },
            face_yaw: 2.5,
            face_pitch: -0.1,
            pattern,
        };
        let outbound = ClientMessage::ProjectileShot(shot);
        let relay = ServerMessage::ProjectileShot(SProjectileShot {
            id: PlayerId(u32::MAX),
            shot,
        });
        assert_eq!(outbound.lane(), Lane::Unreliable);
        assert_eq!(relay.lane(), Lane::Unreliable);
        assert!(encode_message(&outbound).expect("shot encoding failed").len() <= 32);
        assert!(encode_message(&relay).expect("relay encoding failed").len() <= 40);
    }
    assert_eq!(
        ClientMessage::ProjectileHit(CProjectileHit {
            target: HitTarget::Player {
                id: PlayerId(2),
                generation: PlayerGeneration(3)
            },
            direction: [0.0, 1.0],
        })
        .lane(),
        Lane::Reliable
    );
}
