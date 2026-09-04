use bevy::prelude::*;

use crate::{
    actors::ActorSpawnWarningSecs,
    barriers::{KeyKinds, build_barrier_assets},
    bridges::build_bridge_assets,
    characters::MaxHealth,
    config::AssetSet,
    players::MyPlayerId,
    projectiles::ProjectileAssets,
    ui::{HudBanner, QuestLog},
    vfx::BlastRadii,
};
use common::{physics::CollisionWorld, protocol::*};

pub(crate) fn install_bootstrap(app: &mut App, message: SInit, asset_set: &AssetSet) -> anyhow::Result<()> {
    let gameplay_config = message.world.gameplay.gameplay_config()?;
    let map_settings = &message.world.map.settings;
    let (barrier_kind_table, _) = map_settings.kind_tables()?;
    asset_set.validate_gameplay_bindings(gameplay_config.actors.keys().map(String::as_str))?;

    let (barrier_assets, bridge_assets, projectile_assets) =
        app.world_mut().resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
            (
                build_barrier_assets(
                    &mut meshes,
                    &mut materials,
                    &map_settings.barrier_kinds,
                    map_settings.geometry.barrier_thickness(),
                ),
                build_bridge_assets(
                    &mut meshes,
                    &mut materials,
                    &map_settings.bridge_kinds,
                    map_settings.geometry.bridge_thickness(),
                ),
                ProjectileAssets::new(&mut meshes, &mut materials, gameplay_config.projectiles.radius),
            )
        });

    let max_health = MaxHealth {
        player: message.world.gameplay.player.max_health,
        actors: message
            .world
            .gameplay
            .actors
            .iter()
            .map(|(kind, actor)| (kind.clone(), actor.max_health))
            .collect(),
    };
    let blast_radii = BlastRadii {
        player: message.world.gameplay.player.death_blast_radius,
        missile: message.world.gameplay.missiles.blast_radius,
        actors: message
            .world
            .gameplay
            .actors
            .iter()
            .map(|(kind, actor)| (kind.clone(), actor.death_blast_radius))
            .collect(),
    };
    let collision_world = CollisionWorld::from_map_layout(&message.world.map.layout, &barrier_kind_table);

    debug!("received Init: my_id=player#{}", message.player.id.0);
    app.insert_resource(MyPlayerId(message.player.id))
        .insert_resource(message.player.portal_access)
        .insert_resource(gameplay_config)
        .insert_resource(barrier_kind_table)
        .insert_resource(barrier_assets)
        .insert_resource(bridge_assets)
        .insert_resource(projectile_assets)
        .insert_resource(message.world.map.layout)
        .insert_resource(message.world.map.settings)
        .insert_resource(collision_world)
        .insert_resource(blast_radii)
        .insert_resource(max_health)
        .insert_resource(KeyKinds(message.world.map.key_kinds))
        .insert_resource(ActorSpawnWarningSecs(message.world.gameplay.actor_spawn_warning_secs))
        .insert_resource(QuestLog::default())
        .insert_resource(HudBanner::default());
    Ok(())
}
