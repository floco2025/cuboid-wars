# Follow-ups

## Fixes

## Enhancements

- **Third-person body clipping during portal traversal:** jumping into a floor portal makes the player's legs disappear before the body emerges from the exit, leaving the model visibly cut in half. Keep the body presentation continuous across portal entry and exit.

- **Pressure plates cover characters' feet:** give plates collision geometry so players and other characters stand on their surface instead of intersecting the model. Keep the support height aligned with the tread in both active and inactive states.

- **Render ramps as stairs:** add an option to show ramps as stairs while retaining smooth ramp collision and movement. Make stair use configurable per actor kind, like ladder use.

- **Host a game from a client:** colocate the server with one client so it can host the game. Use message queues for communication between the host client and its server, bypassing the network stack; remote clients connect over the network.

- **Missiles through portals:** a missile chasing a target through a portal detonates on the aperture's backing instead of crossing, while bullets hop through; rank `projectile_hop` against the other events in the missile sweep in `client/src/missiles/movement.rs`.

- **Late launch cue:** an observer whose snapshot arrives before the launch cue spawns the missile from the snapshot and plays the launch sound only when the cue lands, after the missile is already in flight (`client/src/network/missiles/handlers.rs`).

- **Checkpoint scan on maps without checkpoints:** `players_checkpoints_system` runs a capsule cast per grounded player every tick before reading an empty list. Gate it with a `run_if` on `map.checkpoints`, like `pending_actor_spawns_active`.

- **Checkpoint spawn sampler duplicates the zone sampler:** `checkpoint_spawn_position` repeats the attempt loop, radius inset, pose transform, and occupied test from `server/src/characters/spawning.rs` with a bare 100 beside `SPAWN_MAX_ATTEMPTS`, and blocks on every solid where zones test only walls. Share the loop and keep the clearance predicate and center-first attempt per caller.

- **Checkpoint list stored twice:** `MapConfig.checkpoints` copies `MapLayout.checkpoints` verbatim and `CheckpointId` indexes both by convention. Drop the copy and pass the layout's list to the checkpoint, respawn, and spawn-destination code; `level_tag` is also evaluated twice per zone in `server/src/map/definition/geometry.rs`.

- **Checkpoint code cleanups:** flatten `ZoneDef` into `CheckpointDef`, move `CHECKPOINT_COLOR` into `client/src/constants.rs`, and name the repeated `"Checkpoint reached"` literal in `ui/hud_banner.rs`.

- **Tool Reference lacks Checkpoints:** the editor's Help → Tool Reference has no section for the Checkpoint tool, its Type selector, right-click type editing, or Erase Checkpoints.

- **Grounding inspection after a blocked step:** the per-frame refresh now only fills characters that lack the diagnostics, so a body blocked by another character draws its probe from the motor's proposed position until the next tick.

- **Rapier upgrades:** Recheck the capsule floor-motion regression before removing the contact-normal adapter in `common/src/physics/world/character_queries.rs`. It works around imprecise cast normals feeding Rapier 0.32’s slope decomposition.

- **Shape contact queries:** If movement shapes expand beyond upright capsules, use Parry contact/distance queries for contact attacks instead of the capsule-specific surface-distance formula. Preserve overlap thresholds and attack-cover rules.

- **Character spatial queries:** Profile character movement and projectile scans at larger actor counts before adding character colliders to Rapier's spatial index. Account for movement ordering, self-exclusion, and client/server collider synchronization.

- **Trigger geometry:** Consider Rapier sensors or shared query shapes for erasers, pickups, and ladders if their separate volume handling grows. Automatic enter/leave events require collision-pipeline integration; retain swept detection for fast crossings and moving fields, and keep trigger-specific gameplay rules.

- **Moving-platform physics:** Evaluate kinematic rigid bodies only if they reduce carrier support/pushing code. Preserve tick-driven client/server motion, boarding and takeoff behavior, portal-relative travel, and crushing rules; body integration alone does not replace those policies.

## Testing

- **Switched carriers, zones, and fireworks:** With several clients, run a nested map on a switch (the one-off correction after a flip learned late, riders staying aboard through a freeze), an actor zone on a switch through death resets and logout, and a fireworks switch with `held: everyone` repeating after its cooldown.

- **Shared checkpoints:** Play through Group — any and Group — all with multiple clients, including staggered visits, death, joining, and leaving. Check each player's next respawn and checkpoint notification.

- **Sliding-carrier pushing:** Let the moving cabin's wall push you while standing still, walking against it, and stepping sideways out of its path. Confirm open space is safe, being pinned against another wall still crushes, and boarding moving platforms remains safe. Crushed actors should play their normal explosion animation and sound.
