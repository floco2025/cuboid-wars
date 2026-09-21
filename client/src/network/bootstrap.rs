use std::{
    ops::ControlFlow,
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use bevy::prelude::*;
use crossbeam_channel::Sender;

use crate::{
    barriers::KeyFields,
    carriers::{CarrierStoreys, spawn_carrier_entities},
    characters::MaxHealth,
    config::{AssetSet, ClientSettings},
    fields::build_field_assets,
    map::MapDimensions,
    missiles::AirGraph,
    players::MyPlayerId,
    projectiles::ProjectileAssets,
    ui::{HudBanner, QuestLog},
    vfx::BlastRadii,
};
use common::{map::Carriers, physics::CollisionWorld, protocol::*};

use super::ServerLink;

// Past netcode's own 15 s request timeout, so a dead address reports as such.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(20);
const LOGIN_POLL: Duration = Duration::from_millis(2);

// Sends `CLogin` and pumps the link until `SInit`. Anything that lands before
// it is a snapshot or an unreliable message, which the protocol lets us drop;
// the reliable lane guarantees `SInit` comes first on it. Reading stops at
// `SInit`, so what the server sent right behind it reaches the app.
pub fn login(link: &mut ServerLink, to_server: &Sender<ClientMessage>, name: String) -> Result<SInit> {
    to_server
        .send(ClientMessage::Login(CLogin { name }))
        .context("server link closed before login")?;
    let deadline = Instant::now() + LOGIN_TIMEOUT;
    loop {
        let now = Instant::now();
        let mut init = None;
        link.receive(now, |message| {
            if let ServerMessage::Init(message) = message {
                init = Some(message);
                return ControlFlow::Break(());
            }
            ControlFlow::Continue(())
        })
        .context("server disconnected before SInit")?;
        if let Some(init) = init {
            return Ok(init);
        }
        link.flush(now);
        if now > deadline {
            bail!("no SInit within {} s", LOGIN_TIMEOUT.as_secs());
        }
        thread::sleep(LOGIN_POLL);
    }
}

pub(crate) fn install_bootstrap(app: &mut App, message: SInit, asset_set: &AssetSet) -> Result<()> {
    message.world.network.validate()?;
    message.world.celestial.validate("world.celestial")?;
    let gameplay_config = message.world.gameplay.gameplay_config()?;
    let map_settings = &message.world.map.settings;
    map_settings.movement.validate("map.settings.movement")?;
    map_settings.celestial.validate("map.settings.celestial")?;
    let field_table = map_settings.field_table()?;
    asset_set.validate_map_bindings(map_settings, &message.world.map.layout)?;
    asset_set.validate_gameplay_bindings(gameplay_config.actors.keys().map(String::as_str))?;
    let vfx = app.world().resource::<ClientSettings>().vfx;

    let (field_assets, projectile_assets) = app.world_mut().resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        (
            build_field_assets(
                &mut meshes,
                &mut materials,
                &map_settings.fields,
                vfx.fields.rail_emissive_brightness,
                vfx.pickups.emissive_brightness,
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
    let air_graph = AirGraph::new(&message.world.map.grids, map_settings.geometry);
    let mut collision_world = CollisionWorld::from_map_layout(&message.world.map.layout);
    collision_world.set_locked_pressure_plates(&message.locked_switches);
    let carriers = Carriers::from_layout(&message.world.map.layout);
    let carrier_entities = spawn_carrier_entities(app.world_mut(), &message.world.map.layout, &carriers);
    let carrier_storeys = CarrierStoreys::from_layout(&message.world.map.layout);
    let map_dimensions = MapDimensions::from_grids(
        &message.world.map.layout,
        &message.world.map.grids,
        map_settings.geometry,
    );

    debug!("received Init: my_id=player#{}", message.player.id.0);
    app.insert_resource(message.world.network)
        .insert_resource(message.world.celestial)
        .insert_resource(message.celestial_clock)
        .insert_resource(ServerTick(message.current_tick))
        .insert_resource(MyPlayerId(message.player.id))
        .insert_resource(message.player.portal_access)
        .insert_resource(gameplay_config)
        .insert_resource(field_table)
        .insert_resource(field_assets)
        .insert_resource(projectile_assets)
        .insert_resource(message.world.map.layout)
        .insert_resource(message.world.map.settings)
        .insert_resource(collision_world)
        .insert_resource(air_graph)
        .insert_resource(carriers)
        .insert_resource(carrier_entities)
        .insert_resource(carrier_storeys)
        .insert_resource(map_dimensions)
        .insert_resource(blast_radii)
        .insert_resource(max_health)
        .insert_resource(KeyFields(message.world.map.items.key_fields()))
        .insert_resource(message.world.map.items)
        .insert_resource(QuestLog::default())
        .insert_resource(HudBanner::default());
    Ok(())
}
