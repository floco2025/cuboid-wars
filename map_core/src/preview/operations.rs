use anyhow::{Context, Result, bail};
use common::constants::PLAYER_COYOTE_SECS;
use serde_json::{Value, json};

use super::{
    jump::{JumpRequest, jump_preview},
    physics::{PreviewPhysics, SurfaceSpec, portal_frame},
};

pub fn dispatch(op: &str, a: &Value) -> Result<Value> {
    Ok(match op {
        "preview_physics" => preview_physics(&a[0])?,
        "jump_preview" => {
            let request: JumpRequest = serde_json::from_value(a[1].clone()).context("request")?;
            json!(jump_preview(&physics(&a[0])?, &request)?)
        }
        "portal_frame" => {
            let surface: SurfaceSpec = serde_json::from_value(a[0].clone()).context("surface")?;
            let frame = portal_frame(&physics(&a[1])?, &surface);
            json!({
                "center": frame.center.to_array(),
                "normal": frame.normal.to_array(),
                "up": frame.up.to_array(),
                "right": frame.right.to_array(),
            })
        }
        _ => bail!("Unknown preview operation: {op}"),
    })
}

fn physics(value: &Value) -> Result<PreviewPhysics> {
    let physics: PreviewPhysics = serde_json::from_value(value.clone()).context("physics")?;
    physics.validate()?;
    Ok(physics)
}

// The physics block of merged map settings, validated, with errors naming
// the settings path they come from.
fn preview_physics(settings: &Value) -> Result<Value> {
    let at = |path: &str| -> Result<Value> {
        settings
            .pointer(&format!("/{}", path.replace('.', "/")))
            .filter(|value| !value.is_null())
            .cloned()
            .with_context(|| format!("{path} is missing"))
    };
    let block = json!({
        "server_hz": at("network.server_hz")?,
        "gravity": at("movement.gravity")?,
        "low_gravity": at("movement.low_gravity")?,
        "player": at("movement.player")?,
        "player_fall": at("player_fall")?,
        "max_health": at("combat.health.player.max")?,
        "body": at("player.movement_collider")?,
        "portal_size": at("weapons.portals.size")?,
        "funnel": at("weapons.portals.funnel")?,
        "coyote_secs": PLAYER_COYOTE_SECS,
    });
    physics(&block)?;
    Ok(block)
}

#[cfg(test)]
#[path = "tests/operations.rs"]
mod tests;
