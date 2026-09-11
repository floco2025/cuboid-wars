use super::{FootstepSet, FootstepSounds};
use std::collections::HashMap;

fn sounds() -> FootstepSounds {
    FootstepSounds {
        volume_db: 0.0,
        ladder_volume_db: -2.0,
        default: "plain".to_owned(),
        sets: HashMap::from([
            (
                "plain".to_owned(),
                FootstepSet {
                    volume_db: 0.0,
                    samples: vec!["plain.ogg".to_owned()],
                    accent: None,
                },
            ),
            (
                "carpet".to_owned(),
                FootstepSet {
                    volume_db: -10.0,
                    samples: vec!["carpet.ogg".to_owned()],
                    accent: None,
                },
            ),
        ]),
    }
}

#[test]
fn unassigned_material_uses_configured_default_with_its_volume() {
    let mut sounds = sounds();
    sounds.validate().expect("valid sound sets rejected");
    assert_eq!(sounds.resolve(None).samples, ["plain.ogg"]);
    sounds.default = "carpet".to_owned();
    assert_eq!(sounds.resolve(None).samples, ["carpet.ogg"]);
    assert_eq!(sounds.resolve(None).volume_db, -10.0);
    assert_eq!(sounds.resolve(Some("plain")).samples, ["plain.ogg"]);
}

#[test]
fn missing_sets_empty_samples_and_invalid_volumes_are_rejected() {
    let mut sounds = sounds();
    assert!(sounds.validate_binding("missing", "material.footstep").is_err());
    sounds.default = "missing".to_owned();
    assert!(sounds.validate().is_err());
    sounds.default = "plain".to_owned();
    sounds.sets.get_mut("plain").expect("plain set missing").samples.clear();
    assert!(sounds.validate().is_err());
    let mut sounds = self::sounds();
    for volume in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX] {
        sounds.sets.get_mut("plain").expect("plain set missing").volume_db = volume;
        assert!(sounds.validate().is_err());
    }
    let mut sounds = self::sounds();
    for volume in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX] {
        sounds.volume_db = volume;
        let error = sounds.validate().expect_err("invalid overall footstep gain accepted");
        assert!(error.to_string().contains("footsteps.volume_db"));
    }
    sounds.volume_db = 0.0;
    for volume in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX] {
        sounds.ladder_volume_db = volume;
        let error = sounds.validate().expect_err("invalid ladder footstep gain accepted");
        assert!(error.to_string().contains("footsteps.ladder_volume_db"));
    }
}
