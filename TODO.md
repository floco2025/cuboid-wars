# Follow-ups

## Enhancements

- **Grounding inspection after a blocked step:** the per-frame refresh now only fills characters that lack the diagnostics, so a body blocked by another character draws its probe from the motor's proposed position until the next tick.

- **Rapier upgrades:** Recheck the capsule floor-motion regression before removing the contact-normal adapter in `common/src/physics/world/character_queries.rs`. It works around imprecise cast normals feeding Rapier 0.32’s slope decomposition.

- **Shape contact queries:** If movement shapes expand beyond upright capsules, use Parry contact/distance queries for contact attacks instead of the capsule-specific surface-distance formula. Preserve overlap thresholds and attack-cover rules.

- **Character spatial queries:** Profile character movement and projectile scans at larger actor counts before adding character colliders to Rapier's spatial index. Account for movement ordering, self-exclusion, and client/server collider synchronization.

- **Trigger geometry:** Consider Rapier sensors or shared query shapes for erasers, pickups, and ladders if their separate volume handling grows. Automatic enter/leave events require collision-pipeline integration; retain swept detection for fast crossings and moving fields, and keep trigger-specific gameplay rules.

- **Moving-platform physics:** Evaluate kinematic rigid bodies only if they reduce carrier support/pushing code. Preserve tick-driven client/server motion, boarding and takeoff behavior, portal-relative travel, and crushing rules; body integration alone does not replace those policies.

- **Puzzle design:** See [PUZZLES.md](PUZZLES.md) for the element inventory, nine example maps, guard encounters, and [next decisions](PUZZLES.md#next-decisions).

## Testing

- **Sliding-carrier pushing:** Let the moving cabin's wall push you while standing still, walking against it, and stepping sideways out of its path. Confirm open space is safe, being pinned against another wall still crushes, and boarding moving platforms remains safe. Crushed actors should play their normal explosion animation and sound.