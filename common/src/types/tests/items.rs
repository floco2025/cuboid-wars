use super::*;

#[test]
fn item_type_config_ids_round_trip() {
    let non_key = [
        ItemType::SingleShotPowerUp,
        ItemType::MultiShotPowerUp,
        ItemType::MissilePack,
        ItemType::PortalGunPowerUp,
        ItemType::HealthPotion,
        ItemType::SpeedPowerUp,
        ItemType::LowGravityPowerUp,
        ItemType::Gold,
    ];
    for item_type in non_key {
        assert_eq!(ItemType::from_config_id(item_type.config_id()), Some(item_type));
    }
    assert_eq!(ItemType::Key(BarrierKindId(0)).config_id(), ItemType::KEY_CONFIG_ID);
    assert_eq!(ItemType::from_config_id(ItemType::KEY_CONFIG_ID), None);
}
