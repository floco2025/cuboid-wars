# Follow-ups

## Fixes

- **Models load once per label:** the asset server loads a GLB again for every label requested while the file is still in flight (`bevy_asset` `load_internal` forces the base load per new labelled handle), and each extra completion re-inserts the sub-assets, emits `Modified`, and respawns every scene instance. The first spawn of each actor kind requests `#Scene0` plus its wheel or aim clip labels in one frame, and the player requests `#Scene0` plus eight clip labels, so the 23 MB player GLB decodes up to nine times at login and its scene respawns after each, resetting animation state and re-running the setup observers. Load the root `Gltf` once and take scene and clip handles from the loaded asset, or preload every configured model at bootstrap before any labelled request.

- **Follow camera distance clamp:** `third_person_transform` clamps the arm to `camera.follow.max_distance` but `FollowCamera::visible_distance` scales with the unclamped distance, so body visibility and the arm disagree whenever the stored distance exceeds the configured maximum.

- **Per-hit death scoring:** `apply_player_projectile_hit` charges `scoring.player_kill` and `scoring.player_death` on every projectile hit, blasts charge them once per death, and beam, fall, void, and crush deaths never do. Move both adjustments into `kill_player` and retarget `nonlethal_hit_returns_survived_and_adjusts_score`.

- **Actor reset clears peace:** `reset_actors` with scope `all` replaces `ActorMap` wholesale, so a death on a map with `respawn.actors.scope: "all"` turns `/peace` off.

- **Blocked movable spawns never retry:** a movable kind with `respawn_secs: null` whose spawn is blocked logs that it will retry and then drops the timer; give it the `WaitingForSpace` state immovable kinds get. The "waiting for space" log also never fires after `reset_actors` because every zone starts in that state.

- **Invincible void-fall recovery keeps momentum:** the god-mode teleport in `players/falling.rs` re-inserts position, yaw, and vertical velocity but not `AirborneMomentum`, unlike `players_respawn_system`.

- **Missing required sound:** `REQUIRED_PLAYER_SOUNDS` lacks `void_fall`, so a removed key passes validation and panics at the first void fall.

- **Tall nested map panics:** a nested map reaching past storey 255 hits an `assert!` in `definition/compile.rs` instead of a map-load error naming the map.

- **Wall light height is a constant:** `WALL_LIGHT_HEIGHT = 2.5` ignores the map's `level_height` (obby uses 2.4); derive it from the geometry config.

- **Client config defaults drifted:** every `client.json` block carries `#[serde(default)]` with a `Default` impl the shipped JSON no longer matches (barrier emissive 2000 vs 2.0, opacity 0.015 vs 0.45, bloom threshold 1.5 vs 2.5, and more), so a missing key silently produces a wrong look. Drop the fallbacks and let a missing key error.

- **Third-person camera ignores shake:** `CameraShake` is applied only on the first-person branch of the follow camera.

- **Barrier pulse at zero frequency still re-uploads:** `barrier_pulsate_system` calls `get_mut` on every kind material every frame even when `frequency_hz` is 0, marking them modified for no change.

- **Editor File → New overwrites a registered map:** choosing an existing map name sets the document path without asking, so the next Save replaces that map's `layout.json` and deletes its autosave; only Save As asks.

- **Editor erase drops orphans level-wide:** Erase Floors, Erase Walls, and Erase remove every item, grass cell, and light on the level that lacks support, not just those in the dragged rectangle, so records kept for manual repair vanish.

- **Wall light GLBs lose their bevels:** `wall_lights.py` adds bevel and weighted-normal modifiers but never applies them before export, so both fixtures ship as plain smooth-shaded boxes.

- **Turret cooldown tests:** `closing_a_barrier_immediately_stops_a_turret` and `turret_holds_long_burst_and_stops_when_player_disconnects` assume a 0.1-second cooldown, while the shipped turret configuration uses 0.5 seconds. Make the fixtures independent of tuning.

- **Ladder descent overlap:** `side_by_side_scuttlers_descend_without_jamming` fails its character-overlap assertion in `server/src/actors/movement/tests/ladder_traffic.rs`, including on the committed baseline. Check separation while two actors descend side by side.

- **Missiles near walls:** Missiles repeatedly miss actors positioned close to walls. Check whether requiring missile clearance to the target centre prevents a valid approach within proximity-fuse range.

## Enhancements

- **Rapier upgrades:** Recheck the capsule floor-motion regression before removing the contact-normal adapter in `common/src/physics/world/character_queries.rs`. It works around imprecise cast normals feeding Rapier 0.32’s slope decomposition.

- **Shape contact queries:** If movement shapes expand beyond upright capsules, use Parry contact/distance queries for contact attacks instead of the capsule-specific surface-distance formula. Preserve overlap thresholds and attack-cover rules.

- **Character spatial queries:** Profile character movement and projectile scans at larger actor counts before adding character colliders to Rapier's spatial index. Account for movement ordering, self-exclusion, and client/server collider synchronization.

- **Trigger geometry:** Consider Rapier sensors or shared query shapes for erasers, pickups, and ladders if their separate volume handling grows. Automatic enter/leave events require collision-pipeline integration; retain swept detection for fast crossings and moving fields, and keep trigger-specific gameplay rules.

- **Moving-platform physics:** Evaluate kinematic rigid bodies only if they reduce carrier support/pushing code. Preserve tick-driven client/server motion, boarding and takeoff behavior, portal-relative travel, and crushing rules; body integration alone does not replace those policies.

- **Puzzle design:** See [PUZZLES.md](PUZZLES.md) for the element inventory, nine example maps, guard encounters, and [next decisions](PUZZLES.md#next-decisions).

## Testing

- **Playtest Switchyard:** Check the customs portal route and raised seal balcony, moving bridge switch, and ladder shuttle solo and with teammates. The user handles in-game testing.
- **Sliding-carrier pushing:** Let the moving cabin's wall push you while standing still, walking against it, and stepping sideways out of its path. Confirm open space is safe, being pinned against another wall still crushes, and boarding moving platforms remains safe. Crushed actors should play their normal explosion animation and sound.
- **Playtest and refine Relay:** The prototype is playable. The user handles in-game testing; refine the puzzles based on their feedback.
