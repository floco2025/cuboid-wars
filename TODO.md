# Follow-ups

## Fixes

- **Portal body pose jumps at the crossing:** between floor portals, the emerging twin is upside down but the main body replaces it upright when the center crosses, swapping the visible legs for the upper body. Preserve the rendered pose across the handoff before reorienting it.

- **Body clipping flickers on moving portals:** straddle detection uses the current tick's portal frame against the interpolated player position, while the visible portal and clipping planes use the interpolated carrier pose. Use the same rendered frames for detection and clipping.

## Enhancements

- **Actor gameplay definitions:** Group each actor kind's health, damage, scoring, and destruction-feed setting under `gameplay.json::actors.kinds`, alongside its body and behaviour, so adding a kind does not require updating several separate tables.

- **Pressure plates cover characters' feet:** give plates collision geometry so players and other characters stand on their surface instead of intersecting the model. Keep the support height aligned with the tread in both active and inactive states.

- **Render ramps as stairs:** add an option to show ramps as stairs while retaining smooth ramp collision and movement. Make stair use configurable per actor kind, like ladder use.

- **Missiles through portals:** a missile chasing a target through a portal detonates on the aperture's backing instead of crossing, while bullets hop through; rank `projectile_hop` against the other events in the missile sweep in `client/src/missiles/movement.rs`.

- **Player-scaled actor counts:** let a spawn zone's actor count depend on the number of logged-in players instead of one fixed `count`. The scaling rule is still to be decided; define it so that later joins fill the added slots and departures let the surplus die off without a cull.

- **Rapier upgrades:** Recheck the capsule floor-motion regression before removing the contact-normal adapter in `common/src/physics/world/character_queries.rs`. It works around imprecise cast normals feeding Rapier’s slope decomposition; `running_across_flat_floor_tiles_keeps_its_speed` still fails without it on 0.35.

## Testing

- **Actor movement sound mix:** Tune `assets.json::actors.movement_volume_db` and `sfx_volume_db` against weapons and footsteps while moving, stopping, climbing, and hovering.

- **Startup mouse capture:** Launch windowed, wait before moving the mouse, and confirm there is no initial view jump and the first click fires. Repeat after switching away and back.

- **Shared checkpoints:** Play through Group — any and Group — all with multiple clients, including staggered visits, death, joining, and leaving. Check each player's next respawn and checkpoint notification.

- **Sliding-carrier pushing:** Let the moving cabin's wall push you while standing still, walking against it, and stepping sideways out of its path. Confirm open space is safe, being pinned against another wall still crushes, and boarding moving platforms remains safe. Crushed actors should play their normal explosion animation and sound.
