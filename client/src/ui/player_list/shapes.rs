use bevy::prelude::*;
use common::protocol::{BarrierKindId, ItemType};

use crate::items::item_symbol_image;

#[derive(Resource)]
pub struct HudShapeAssets {
    pub speed: Handle<Image>,
    pub multi_shot: Handle<Image>,
    pub low_gravity: Handle<Image>,
    pub key: Handle<Image>,
    pub missile: Handle<Image>,
}

impl FromWorld for HudShapeAssets {
    fn from_world(world: &mut World) -> Self {
        let mut images = world.resource_mut::<Assets<Image>>();
        Self {
            speed: images.add(item_symbol_image(ItemType::SpeedPowerUp)),
            multi_shot: images.add(item_symbol_image(ItemType::MultiShotPowerUp)),
            low_gravity: images.add(item_symbol_image(ItemType::LowGravityPowerUp)),
            key: images.add(item_symbol_image(ItemType::Key(BarrierKindId(0)))),
            missile: images.add(item_symbol_image(ItemType::MissilePack)),
        }
    }
}
