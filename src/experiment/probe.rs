use anyhow::{Context, Result};
use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    map::Carriers,
    physics::{CollisionWorld, PortalPlacementFailure, compute_portal_placement},
    protocol::*,
};
use serde_json::{Value, json};

use super::session::{Session, yaw_of};

impl Session {
    // The game's verdict on portal shots, with no portal gun, cooldown, or
    // time involved and nothing changed: where each shot's ray lands, and the
    // aperture the placement rule opens there or why it opens none.
    pub fn probe(&self, eye: Option<[f32; 3]>, targets: &[[f32; 3]]) -> Result<Value> {
        let eye = match eye {
            Some(eye) => Vec3::from_array(eye),
            None => self.eye()?,
        };
        let world = self.server.world();
        let config = &world.resource::<GameplayConfig>().portals;
        let collision_world = world.resource::<CollisionWorld>();
        let open_fields = &world.resource::<SwitchState>().open_fields;
        let mut shots = Vec::with_capacity(targets.len());
        for target in targets {
            let direction = (Vec3::from_array(*target) - eye)
                .try_normalize()
                .context("probe target must differ from its eye")?;
            let Some(hit) = collision_world.portal_surface_along_ray(eye, direction, config.range, open_fields) else {
                shots.push(json!({"target": target, "status": "no_surface"}));
                continue;
            };
            let mut shot = json!({"target": target,
                "hit": {"position": hit.point.to_array(), "normal": hit.normal.to_array()}});
            let placement = compute_portal_placement(
                eye,
                direction,
                yaw_of(direction),
                config,
                collision_world,
                world.resource::<MapLayout>(),
                world.resource::<Carriers>(),
                open_fields,
                &world.resource::<MapSettings>().textures,
            );
            match placement {
                Ok(placement) => {
                    shot["status"] = json!("placed");
                    shot["portal"] = json!({"position": placement.pos.to_array(),
                        "normal": placement.normal.to_array(), "yaw": placement.yaw, "carrier": placement.carrier.0});
                }
                Err(PortalPlacementFailure::InvalidPlacement) => shot["status"] = json!("no_fit"),
                Err(PortalPlacementFailure::IncompatibleMaterial(_)) => {
                    shot["status"] = json!("incompatible_material");
                }
            }
            shots.push(shot);
        }
        Ok(json!({"status": "probed", "eye": eye.to_array(), "shots": shots}))
    }
}

#[cfg(test)]
#[path = "tests/probe.rs"]
mod tests;
