use super::*;
use crate::config::fixtures::load_test_gameplay;

#[test]
fn gameplay_bootstrap_rejects_duplicate_actor_kinds() {
    let gameplay = load_test_gameplay().expect("test gameplay config rejected");
    let actors: Vec<_> = gameplay
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
    let mut bootstrap = GameplayBootstrap {
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
    };
    bootstrap.gameplay_config().expect("valid bootstrap rejected");
    bootstrap.actors.push(bootstrap.actors[0].clone());
    let error = bootstrap.gameplay_config().expect_err("duplicate actor kind accepted");
    assert!(error.to_string().contains("duplicate actor kind"));
}
