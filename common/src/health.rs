use crate::types::Health;

#[must_use]
pub fn health_ratio(health: Health, max_health: f32) -> f32 {
    if max_health <= 0.0 {
        return 0.0;
    }
    (health.0 / max_health).clamp(0.0, 1.0)
}

pub fn apply_damage(health: &mut Health, amount: f32) {
    if amount <= 0.0 {
        return;
    }
    health.0 = (health.0 - amount).max(0.0);
}

pub fn regenerate_health(health: &mut Health, max_health: f32, amount: f32) {
    if amount <= 0.0 {
        return;
    }
    health.0 = (health.0 + amount).min(max_health);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_ratio_clamps_to_unit_range() {
        assert_eq!(health_ratio(Health(-10.0), 100.0), 0.0);
        assert_eq!(health_ratio(Health(50.0), 100.0), 0.5);
        assert_eq!(health_ratio(Health(150.0), 100.0), 1.0);
    }

    #[test]
    fn damage_does_not_go_below_zero() {
        let mut health = Health(20.0);
        apply_damage(&mut health, 30.0);
        assert_eq!(health, Health(0.0));
    }

    #[test]
    fn regeneration_does_not_exceed_max_health() {
        let mut health = Health(90.0);
        regenerate_health(&mut health, 100.0, 20.0);
        assert_eq!(health, Health(100.0));
    }
}
