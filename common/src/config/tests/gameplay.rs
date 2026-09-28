use super::*;

fn gameplay_bootstrap() -> GameplayBootstrap {
    let gameplay = load_test_gameplay().expect("server gameplay projection should load");
    let mut actors: Vec<_> = gameplay
        .actors
        .iter()
        .map(|(kind, actor)| {
            (
                kind.clone(),
                ActorGameplayBootstrap {
                    gameplay: actor.clone(),
                    max_health: 100.0,
                    death_blast_radius: 5.0,
                },
            )
        })
        .collect();
    actors.sort_by(|a, b| a.0.cmp(&b.0));
    GameplayBootstrap {
        player: PlayerGameplayBootstrap {
            gameplay: gameplay.player,
            max_health: 100.0,
            death_blast_radius: 5.0,
        },
        actors,
        projectiles: gameplay.projectiles,
        missiles: MissilesGameplayBootstrap {
            gameplay: gameplay.missiles,
            blast_radius: 5.0,
        },
        portals: gameplay.portals,
    }
}

#[test]
fn gameplay_bootstrap_builds_hash_indexed_runtime_config() {
    let bootstrap = gameplay_bootstrap();
    let gameplay = bootstrap.gameplay_config().expect("bootstrap should validate");
    assert_eq!(gameplay.actors.len(), bootstrap.actors.len());
    assert!(gameplay.actor("zapper").is_some());
}

#[test]
fn actor_ladder_permission_survives_bootstrap_encoding() {
    let mut bootstrap = gameplay_bootstrap();
    for (_, actor) in &mut bootstrap.actors {
        actor.gameplay.can_use_ladders = false;
    }
    bootstrap.actors[0].1.gameplay.can_use_ladders = true;
    let bytes =
        bincode::encode_to_vec(&bootstrap, bincode::config::standard()).expect("gameplay bootstrap encoding failed");
    let (decoded, _): (GameplayBootstrap, _) =
        bincode::decode_from_slice(&bytes, bincode::config::standard()).expect("gameplay bootstrap decoding failed");
    let config = decoded.gameplay_config().expect("decoded gameplay invalid");
    assert!(config.expect_actor(&bootstrap.actors[0].0).can_use_ladders);
    assert!(!config.expect_actor(&bootstrap.actors[1].0).can_use_ladders);
}

#[test]
fn gameplay_bootstrap_rejects_duplicate_actor_kinds() {
    let mut bootstrap = gameplay_bootstrap();
    bootstrap.actors.push(bootstrap.actors[0].clone());
    let error = bootstrap.gameplay_config().expect_err("duplicate actor kind accepted");
    assert!(error.to_string().contains("duplicate actor kind"));
}

#[test]
fn portal_size_and_capture_settings_are_validated_after_bootstrap() {
    let mut bootstrap = gameplay_bootstrap();
    bootstrap.portals.size.width = 2.0;
    bootstrap.portals.size.height = 3.0;
    bootstrap.portals.funnel.capture_margin = 0.9;
    let bytes = bincode::encode_to_vec(&bootstrap, bincode::config::standard()).expect("encode");
    let (decoded, _): (GameplayBootstrap, _) =
        bincode::decode_from_slice(&bytes, bincode::config::standard()).expect("decode");
    let config = decoded.gameplay_config().expect("valid resized portals");
    assert_eq!(config.portals.size.width, 2.0);
    assert_eq!(config.portals.size.height, 3.0);
    assert_eq!(config.portals.funnel.capture_margin, 0.9);
    for value in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        let mut bad = config.portals;
        bad.size.width = value;
        assert!(
            bad.validate("portals")
                .expect_err("invalid portal setting accepted")
                .to_string()
                .contains("size.width")
        );
        bad = config.portals;
        bad.size.height = value;
        assert!(
            bad.validate("portals")
                .expect_err("invalid portal setting accepted")
                .to_string()
                .contains("size.height")
        );
    }
    for value in [-1.0, f32::NAN, f32::INFINITY] {
        let mut bad = config.portals;
        bad.funnel.capture_margin = value;
        assert!(
            bad.validate("portals")
                .expect_err("invalid portal setting accepted")
                .to_string()
                .contains("funnel.capture_margin")
        );
    }
    let mut disabled = config.portals;
    disabled.funnel.capture_margin = 0.0;
    disabled.validate("portals").expect("zero disables assistance");
}
