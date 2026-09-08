# Follow-ups

## Fixes

- **Containment puzzle:** The start barriers safely hold the player and sentry until the player opens them. The route to the finish returns through the sentry's area, but the player can outrun it and finish without trapping it. Make containment necessary while keeping the lure and escape practical.

- **Missiles near walls:** Missiles repeatedly miss actors positioned close to walls. Check whether requiring missile clearance to the target centre prevents a valid approach within proximity-fuse range.

## Enhancements

- **Rapier upgrades:** Recheck the capsule floor-motion regression before removing the contact-normal adapter in `common/src/physics/world/character_queries.rs`. It works around imprecise cast normals feeding Rapier 0.32’s slope decomposition.

- **Shape contact queries:** If movement shapes expand beyond upright capsules, use Parry contact/distance queries for contact attacks instead of the capsule-specific surface-distance formula. Preserve overlap thresholds and attack-cover rules.

- **Character spatial queries:** Profile character movement and projectile scans at larger actor counts before adding character colliders to Rapier's spatial index. Account for movement ordering, self-exclusion, and client/server collider synchronization.

- **Trigger geometry:** Consider Rapier sensors or shared query shapes for erasers, pickups, and ladders if their separate volume handling grows. Automatic enter/leave events require collision-pipeline integration; retain swept detection for fast crossings and moving fields, and keep trigger-specific gameplay rules.

- **Moving-platform physics:** Evaluate kinematic rigid bodies only if they reduce carrier support/pushing code. Preserve tick-driven client/server motion, boarding and takeoff behavior, portal-relative travel, and crushing rules; body integration alone does not replace those policies.

- **Logic map:** The lower route and ladders bypass amber, leaving only blue to open before collecting gold. Redesign the switch and route dependencies to create a meaningful decision and prevent bypassing the intended conditions.

- **Puzzle design:** See [PUZZLES.md](PUZZLES.md) for the element inventory, nine example maps, guard encounters, and [next decisions](PUZZLES.md#next-decisions).

## Testing

- **Robot asset test:** Stabilize `bevy_loads_embedded_materials_and_animates_the_exported_skeleton`; it intermittently loses the thigh entity during asynchronous scene loading (`thigh transform missing`) and passes on rerun.

- **Character capsules and hitboxes:** Retest smooth walking, running, and speed-power-up movement throughout Hotel after the capsule contact fix, including floor joins and reconciliation snaps. Check slope and step limits, natural edge overhang, jumps, actor clearance and contact attacks, ladders, moving-platform boarding/pushing/crushing, portals, and erasers. Cycle B through Off, Movement, Hitbox, and Grounding; check rotating damage boxes, grounding contact indicators and Grounded/Airborne/Ladder labels, including after respawn and portal travel. Grounding reuses the movement capsule; cycling views shows no banner. Movement and hitbox sizes are independent in gameplay.json. The user handles in-game testing.

- **Humanoid player:** Check appearance in-game and through portals, local/remote walking and running, strafing/backpedalling, ladder ascent/hold/descent, jumping/falling/landing, stun, moving platforms, and respawning. Check joint coverage and arm clearance during transitions, and B-cycled movement and hitbox inspection. The user handles in-game testing.

- **Turret and inspection controls:** Check the textured pedestal and animated head, including targets above/below it, nearby cover, moving carriers, and spawn ghosts. Check its tight collider around the pedestal and main head with the barrel excluded, turret/zapper beams at chest height, `/peace` stopping and restoring attacks, and `B` cycling collision inspection views (off on each client launch). The user handles in-game testing.

- **Puzzle examples:** Play the eight solo `puzzle_*` maps and `puzzle_coop` with two players. Check intended solutions, guard exposure and missile retries, `puzzle_stages` actor restoration after a posthumous turret kill, `puzzle_containment` killing a full-health player on hunter contact and restoring the hunter after solo death or logout, configurable group countdowns and actor reset scopes, `puzzle_access` closing its barrier after everyone dies, bridge toggle/death resets, actor and switch logout policies (including logout during a countdown and an empty server), `puzzle_coop` revealing its finish plates only after each player collects a coin (including one player collecting both and the other waiting for respawn), Hotel’s solo toggles and multiplayer momentary controls, shuttle boarding, momentum aim, hunter containment, and shortcuts. See [map list and solutions](PUZZLES.md#small-example-maps). The user handles in-game testing.

- **Projectile pickups:** Check single-shot (one ball) and multishot (three balls) pickups, pickup-only weapon availability, weapon cycling, multishot-first fallback, single-shot pickups preserving multishot selection, and eraser/death reset. Check equal-width HUD power-up slots in single-shot, multishot, portal, speed, low-gravity order, with gaps before missiles and keys. Single-shot is in Hotel’s random pool and Obby’s pickup row. The user handles in-game testing.
- **Window settings:** Check mixed-DPI restoration on macOS and Windows/X11, size/fullscreen restoration with compositor-controlled placement on Wayland, and saving after a drag or an immediate window close. The user handles in-game testing.
- **Energy fields and erasers:** Check zapper beam clipping, blast shielding, projectile absorption, and erasers clearing weapons and power-ups while keeping keys, including in god mode. Missiles consume ammo in god mode; `/give missiles` refills it. Eraser entry sounds should play only when equipment is removed. The user handles in-game testing.

- **Playtest Switchyard:** Check the customs portal route and raised seal balcony, moving bridge switch, and ladder shuttle solo and with teammates. The user handles in-game testing.
- **Actor ladders:** Visually check intermediate landings, moving carriers, and mines approaching from opposite sides. The user handles in-game testing.
- **Sliding-carrier pushing:** Let the moving cabin's wall push you while standing still, walking against it, and stepping sideways out of its path. Confirm open space is safe, being pinned against another wall still crushes, and boarding moving platforms remains safe. Crushed actors should play their normal explosion animation and sound.
- **Playtest and refine Relay:** The prototype is playable. The user handles in-game testing; refine the puzzles based on their feedback.
