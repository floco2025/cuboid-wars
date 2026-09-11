# Follow-ups

## Fixes

- **Grass burns through walls:** a blast beside a wall burns the grass on the far side of it. The scorch mark is cut by that wall's shadow; the burn outline in `map/grass/burn.rs` could take the same cut.

- **Portal body pose jumps at the crossing:** between floor portals, the emerging twin is upside down but the main body replaces it upright when the center crosses, swapping the visible legs for the upper body. Preserve the rendered pose across the handoff before reorienting it.

- **Body clipping flickers on moving portals:** straddle detection uses the current tick's portal frame against the interpolated player position, while the visible portal and clipping planes use the interpolated carrier pose. Use the same rendered frames for detection and clipping.

## Enhancements

- **Pressure plates cover characters' feet:** give plates collision geometry so players and other characters stand on their surface instead of intersecting the model. Keep the support height aligned with the tread in both active and inactive states.

- **Render ramps as stairs:** add an option to show ramps as stairs while retaining smooth ramp collision and movement. Make stair use configurable per actor kind, like ladder use.

- **Host a game from a client:** colocate the server with one client so it can host the game. Use message queues for communication between the host client and its server, bypassing the network stack; remote clients connect over the network.

- **Missiles through portals:** a missile chasing a target through a portal detonates on the aperture's backing instead of crossing, while bullets hop through; rank `projectile_hop` against the other events in the missile sweep in `client/src/missiles/movement.rs`.

- **Rapier upgrades:** Recheck the capsule floor-motion regression before removing the contact-normal adapter in `common/src/physics/world/character_queries.rs`. It works around imprecise cast normals feeding Rapier 0.32’s slope decomposition.

## Testing

- **Actors on light bridges:** On obby, toggle the bridge and check that scuttlers cross it while it is powered, that one about to step onto it stops at the edge when it loses power, and that any standing on it fall without a death effect.

- **Switched targets after the review fixes:** With several clients, check that pressing a plate fills an On-response actor zone after the beam-in warning, that switching a zone off and on again neither restarts nor skips its refill countdown, that an Off-response actor zone is filled at boot, that a client joining while an Off-response carrier is mid-travel lands on it, that fireworks never start without a press, and that a Save As in the editor carries edited barrier and bridge catalogs to the destination map.

- **Shared checkpoints:** Play through Group — any and Group — all with multiple clients, including staggered visits, death, joining, and leaving. Check each player's next respawn and checkpoint notification.

- **Sliding-carrier pushing:** Let the moving cabin's wall push you while standing still, walking against it, and stepping sideways out of its path. Confirm open space is safe, being pinned against another wall still crushes, and boarding moving platforms remains safe. Crushed actors should play their normal explosion animation and sound.
