# Follow-ups

## Fixes

- **Checkpoint re-entry replays the shared activation:** a jump in place inside a group zone (the seeded contact in `server/src/players/checkpoints.rs` survives only within 5 cm of the floor, so the first airborne tick drops it) or stepping out of and back into the already-active shared checkpoint re-runs the activation, regressing individual saves, rewriting everyone's facing, and clearing partial Group — all visits. Skip activation when the entered checkpoint is already `PlayerMap.shared_checkpoint`, and cover Ground → Airborne → Ground at the same spot with a test.

- **Simultaneous shared entries lose the second:** when two shared checkpoints are entered on the same tick, `apply_checkpoint_entries` activates the lowest index and breaks, but the other entrant's `checkpoint_contact` is already recorded, so that checkpoint never activates until they leave and re-enter. Defer the remaining entries to the next tick instead of dropping them.

- **Blocked initial placement is silent:** a login whose checkpoint destination is blocked leaves the player `Dead { 0 }` with no body, no `SPlayerDeath`, no log line, and no spawn-zone fallback (`player_spawn_destination` returns `None` through `?`), so the client sits bodiless with an inert camera until it disconnects. Fall back to the spawn zone or tell the client it is waiting.

- **Negative respawn timer resets the world at once:** a blocked respawn keeps ticking `respawn_remaining_secs` below zero, and `PlayerMap::disconnect` passes that value as the actor-reset delay, so the logout resets actors on the next tick instead of after `respawn_secs`. Clamp the countdown or treat a non-positive remainder as the full delay.

- **God-mode void fall ignores the checkpoint:** `players_fall_death_system` teleports an invincible player through `generate_player_spawn_position` and never seeds `checkpoint_contact`, so it lands in the spawn zone and, where a spawn zone overlaps a checkpoint, counts as a fresh entry. Route it through `player_spawn_destination` like login and respawn.

- **Checkpoint cue can be lost:** `SCheckpointReached` rides the unreliable lane and nothing in the snapshot carries the saved checkpoint, so one dropped datagram loses the sound and banner until the next death. Send it on the reliable lane or add the saved checkpoint to `SSnapshot`.

- **Editor cannot pick a checkpoint under a spawn zone:** `hit_at` and `spawn_zone_at` in `tools/map_editor/erasing.py` return the first zone list's hit, so a checkpoint overlapping a spawn zone can never be right-clicked, hovered, or Alt-dragged. The comments there and in `spawn_zones.py` still describe the old actor → player order, and `spawn_zones.py` compares against the literal `"checkpoints"` instead of `CHECKPOINT_LIST`.

- **Editor confuses same-rectangle checkpoints:** `zone_key` in `tools/map_editor/normalization.py` keys checkpoints by rectangle only while canonicalization keys them by rectangle and type, so editing the type of one of two overlapping checkpoints re-selects the first and later edits apply to the wrong zone.

## Enhancements

- **Checkpoint scan on maps without checkpoints:** `players_checkpoints_system` runs a capsule cast per grounded player every tick before reading an empty list. Gate it with a `run_if` on `map.checkpoints`, like `pending_actor_spawns_active`.

- **Checkpoint spawn sampler duplicates the zone sampler:** `checkpoint_spawn_position` repeats the attempt loop, radius inset, pose transform, and occupied test from `server/src/characters/spawning.rs` with a bare 100 beside `SPAWN_MAX_ATTEMPTS`, and blocks on every solid where zones test only walls. Share the loop and keep the clearance predicate and center-first attempt per caller.

- **Checkpoint list stored twice:** `MapConfig.checkpoints` copies `MapLayout.checkpoints` verbatim and `CheckpointId` indexes both by convention. Drop the copy and pass the layout's list to the checkpoint, respawn, and spawn-destination code; `level_tag` is also evaluated twice per zone in `server/src/map/definition/geometry.rs`.

- **Checkpoint code cleanups:** flatten `ZoneDef` into `CheckpointDef`, replace `wait_for_spawn` with `begin_respawn(0.0)`, move `CHECKPOINT_COLOR` into `client/src/constants.rs`, and name the repeated `"Checkpoint reached"` literal in `ui/hud_banner.rs`.

- **Tool Reference lacks Checkpoints:** the editor's Help → Tool Reference has no section for the Checkpoint tool, its Type selector, right-click type editing, or Erase Checkpoints.

- **Grounding inspection after a blocked step:** the per-frame refresh now only fills characters that lack the diagnostics, so a body blocked by another character draws its probe from the motor's proposed position until the next tick.

- **Rapier upgrades:** Recheck the capsule floor-motion regression before removing the contact-normal adapter in `common/src/physics/world/character_queries.rs`. It works around imprecise cast normals feeding Rapier 0.32’s slope decomposition.

- **Shape contact queries:** If movement shapes expand beyond upright capsules, use Parry contact/distance queries for contact attacks instead of the capsule-specific surface-distance formula. Preserve overlap thresholds and attack-cover rules.

- **Character spatial queries:** Profile character movement and projectile scans at larger actor counts before adding character colliders to Rapier's spatial index. Account for movement ordering, self-exclusion, and client/server collider synchronization.

- **Trigger geometry:** Consider Rapier sensors or shared query shapes for erasers, pickups, and ladders if their separate volume handling grows. Automatic enter/leave events require collision-pipeline integration; retain swept detection for fast crossings and moving fields, and keep trigger-specific gameplay rules.

- **Moving-platform physics:** Evaluate kinematic rigid bodies only if they reduce carrier support/pushing code. Preserve tick-driven client/server motion, boarding and takeoff behavior, portal-relative travel, and crushing rules; body integration alone does not replace those policies.

- **Puzzle design:** See [PUZZLES.md](PUZZLES.md) for the element inventory, nine example maps, guard encounters, and [next decisions](PUZZLES.md#next-decisions).

## Testing

- **Client movement trust:** Play obstacle courses with multiple clients under latency, jitter, and packet loss. Check narrow landings, moving platforms, ladders, portal launches, knockback, and remote-player smoothing. Force a large disagreement and confirm clean local recovery with server rejection/client snap warnings.

- **Shared checkpoints:** Play through Group — any and Group — all with multiple clients, including staggered visits, death, joining, and leaving. Check each player's next respawn and checkpoint notification.

- **Sliding-carrier pushing:** Let the moving cabin's wall push you while standing still, walking against it, and stepping sideways out of its path. Confirm open space is safe, being pinned against another wall still crushes, and boarding moving platforms remains safe. Crushed actors should play their normal explosion animation and sound.
