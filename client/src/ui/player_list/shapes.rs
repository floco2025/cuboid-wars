use bevy::prelude::*;
use common::protocol::{BarrierKindId, ItemType};

use crate::items::{item_symbol_image, item_symbol_image_cropped};

#[derive(Resource)]
pub struct HudShapeAssets {
    pub single_shot: Handle<Image>,
    pub multi_shot: Handle<Image>,
    pub portal_gun: Handle<Image>,
    pub speed: Handle<Image>,
    pub low_gravity: Handle<Image>,
    pub missile: Handle<Image>,
    pub key: Handle<Image>,
}

impl FromWorld for HudShapeAssets {
    fn from_world(world: &mut World) -> Self {
        let mut images = world.resource_mut::<Assets<Image>>();
        Self {
            single_shot: images.add(item_symbol_image(ItemType::SingleShotPowerUp)),
            multi_shot: images.add(item_symbol_image(ItemType::MultiShotPowerUp)),
            portal_gun: images.add(item_symbol_image(ItemType::PortalGunPowerUp)),
            speed: images.add(item_symbol_image(ItemType::SpeedPowerUp)),
            low_gravity: images.add(item_symbol_image(ItemType::LowGravityPowerUp)),
            // Upright silhouettes in height-sized slots.
            missile: images.add(item_symbol_image_cropped(ItemType::MissilePack)),
            key: images.add(item_symbol_image_cropped(ItemType::Key(BarrierKindId(0)))),
        }
    }
}
