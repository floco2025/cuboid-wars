use super::*;
use crate::{test_fixtures, vfx::presentation_plugin};
use common::{
    celestial::{CelestialClockAnchor, CelestialCycleSettings},
    config::{
        ActorGameplayBootstrap, GameplayBootstrap, GameplayConfig, MissilesGameplayBootstrap, NetworkConfig,
        PlayerGameplayBootstrap,
    },
};

fn bootstrap() -> SInit {
    let gameplay = test_fixtures::gameplay_config();
    let settings = test_fixtures::map_settings();
    SInit {
        player: PlayerBootstrap {
            id: PlayerId(1),
            portal_access: PortalAccess::Both { pair: PortalPairId(1) },
        },
        celestial_clock: CelestialClockAnchor::initial(&settings.celestial, 0),
        current_tick: 0,
        switch_state: Default::default(),
        locked_switches: Vec::new(),
        world: WorldBootstrap {
            network: NetworkConfig::default(),
            celestial: CelestialCycleSettings {
                day_duration_secs: 300.0,
                lunar_cycle_days: 28.0,
            },
            gameplay: GameplayBootstrap {
                player: PlayerGameplayBootstrap {
                    gameplay: gameplay.player,
                    max_health: 100.0,
                    death_blast_radius: 5.0,
                },
                actors: gameplay
                    .actors
                    .into_iter()
                    .map(|(kind, gameplay)| {
                        (
                            kind,
                            ActorGameplayBootstrap {
                                gameplay,
                                max_health: 100.0,
                                death_blast_radius: 5.0,
                            },
                        )
                    })
                    .collect(),
                projectiles: gameplay.projectiles,
                missiles: MissilesGameplayBootstrap {
                    gameplay: gameplay.missiles,
                    blast_radius: 5.0,
                },
                portals: gameplay.portals,
            },
            map: MapBootstrap {
                layout: MapLayout::default(),
                settings,
                items: MapItems(Vec::new()),
                grids: vec![CarrierGrid {
                    carrier: CarrierId::WORLD,
                    cols: 2,
                    rows: 2,
                    levels: 1,
                }],
            },
        },
    }
}

#[test]
fn bootstrap_creates_portal_assets_after_installing_their_config() {
    let mut app = App::new();
    app.init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .insert_resource(test_fixtures::client_settings())
        .add_plugins(presentation_plugin);
    assert!(!app.world().contains_resource::<GameplayConfig>());
    let mut message = bootstrap();
    message.world.gameplay.portals.size.width = 2.4;
    message.world.gameplay.portals.size.height = 3.6;
    install_bootstrap(&mut app, message, &test_fixtures::asset_set())
        .expect("bootstrap must create configuration-dependent assets without a preinstalled gameplay resource");
    for size in [
        app.world().resource::<PortalAssets>().size,
        app.world().resource::<PortalFizzleAssets>().size,
    ] {
        assert_eq!(size.width, 2.4);
        assert_eq!(size.height, 3.6);
    }
}
