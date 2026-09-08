use bevy::prelude::*;

use super::laser::{LaserBeam, attach_laser_audio, spawn_laser_beam};
use crate::{
    actors::ActorMap,
    config::{AssetSet, ClientSettings},
    players::PlayerMap,
};
use common::protocol::{ActorMarker, Position};

pub(super) fn continuous_beams_sync_system(
    mut commands: Commands,
    actors: Res<ActorMap>,
    players: Res<PlayerMap>,
    asset_server: Res<AssetServer>,
    asset_set: Res<AssetSet>,
    settings: Res<ClientSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    positions: Query<&Position, With<ActorMarker>>,
    mut beams: Query<(Entity, &mut LaserBeam)>,
) {
    if actors.peaceful {
        for (entity, _) in &beams {
            commands.entity(entity).despawn();
        }
        return;
    }
    for (entity, mut beam) in &mut beams {
        if beam.remaining_secs.is_some() {
            continue;
        }
        let target = actors
            .get(&beam.actor)
            .and_then(|actor| actor.beam.target)
            .filter(|id| players.get(id).is_some());
        if let Some(target) = target {
            beam.target = target;
        } else {
            commands.entity(entity).despawn();
        }
    }
    for (id, actor) in actors.iter() {
        let Some(target) = actor.beam.target.filter(|id| players.get(id).is_some()) else {
            continue;
        };
        if beams
            .iter()
            .any(|(_, beam)| beam.actor == *id && beam.remaining_secs.is_none())
        {
            continue;
        }
        let beam = spawn_laser_beam(&mut commands, &mut meshes, &mut materials, *id, target, None);
        attach_laser_audio(
            &mut commands,
            beam,
            &actor.kind,
            &asset_server,
            &asset_set,
            &settings,
            positions.get(actor.entity).ok().copied(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{actors::ActorInfo, players::PlayerInfo};
    use common::protocol::{ActorId, Health, Player, PlayerId, PlayerMoveIntent};

    #[test]
    fn continuous_beam_keeps_one_effect_and_sound_through_retargeting() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .init_asset::<AudioSource>()
            .insert_resource(AssetSet::load_default().expect("asset set rejected"))
            .insert_resource(ClientSettings::load_default().expect("client settings rejected"))
            .init_resource::<ActorMap>()
            .init_resource::<PlayerMap>()
            .add_systems(Update, continuous_beams_sync_system);
        for id in [PlayerId(1), PlayerId(2)] {
            let entity = app.world_mut().spawn_empty().id();
            let player = Player::new(
                "Player".into(),
                Position::default(),
                PlayerMoveIntent::default(),
                0.0,
                0,
                Health(500.0),
            );
            app.world_mut()
                .resource_mut::<PlayerMap>()
                .insert(id, PlayerInfo::from_snapshot(entity, &player, 0));
        }
        let id = ActorId(1);
        let entity = app.world_mut().spawn((ActorMarker, Position::default())).id();
        let mut actor = ActorInfo {
            entity,
            kind: "turret".into(),
            anchor: None,
            beam: Default::default(),
        };
        actor.beam.apply(1, Some(PlayerId(1)));
        app.world_mut().resource_mut::<ActorMap>().insert(id, actor);
        app.update();
        let beam = app
            .world_mut()
            .query_filtered::<Entity, With<LaserBeam>>()
            .single(app.world())
            .expect("beam missing");
        assert!(app.world().get::<AudioPlayer>(beam).is_some());
        for tick in 2..5 {
            app.world_mut()
                .resource_mut::<ActorMap>()
                .get_mut(&id)
                .expect("turret missing")
                .beam
                .apply(tick, Some(PlayerId(2)));
            app.update();
            assert_eq!(
                app.world_mut()
                    .query_filtered::<Entity, With<LaserBeam>>()
                    .single(app.world())
                    .expect("beam missing"),
                beam
            );
            assert_eq!(
                app.world().get::<LaserBeam>(beam).expect("beam missing").target,
                PlayerId(2)
            );
        }
        app.world_mut()
            .resource_mut::<ActorMap>()
            .get_mut(&id)
            .expect("turret missing")
            .beam
            .apply(5, None);
        app.update();
        assert!(app.world().get_entity(beam).is_err());
        app.world_mut()
            .resource_mut::<ActorMap>()
            .get_mut(&id)
            .expect("turret missing")
            .beam
            .apply(6, Some(PlayerId(1)));
        app.update();
        assert_eq!(app.world_mut().query::<&LaserBeam>().iter(app.world()).count(), 1);
        app.world_mut().resource_mut::<ActorMap>().remove(&id);
        app.update();
        assert_eq!(app.world_mut().query::<&LaserBeam>().iter(app.world()).count(), 0);
    }

    #[test]
    fn peace_mode_removes_bursts_and_continuous_beams_including_late_cues() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .init_asset::<AudioSource>()
            .insert_resource(AssetSet::load_default().expect("asset set rejected"))
            .insert_resource(ClientSettings::load_default().expect("client settings rejected"))
            .init_resource::<ActorMap>()
            .init_resource::<PlayerMap>()
            .add_systems(Update, continuous_beams_sync_system);
        app.world_mut().resource_mut::<ActorMap>().peaceful = true;
        for _ in 0..2 {
            for remaining_secs in [None, Some(3.0)] {
                app.world_mut().spawn(LaserBeam {
                    actor: ActorId(1),
                    target: PlayerId(1),
                    remaining_secs,
                    wander_width_fraction: 0.4,
                    wander_height_fraction: 0.2,
                    aim_height_fraction: 0.8,
                });
            }
            app.update();
            assert_eq!(app.world_mut().query::<&LaserBeam>().iter(app.world()).count(), 0);
        }
    }
}
