# Follow-ups

## Fixes

- **Client memory:** the hotel client sits at ~2.0 GB RSS with mimalloc, while forcing every freed block back to the OS measured the live set at ~1.2 GB; the rest is allocator slack behind the load-time peak, when every texture decodes before the mipmap pass releases it. Lower that peak (decode and release the textures a few at a time) or tune mimalloc's purge if the gap matters.

- **Missing UI characters:** the bundled Fira Mono subset renders dashes, ellipses, arrows, and other unsupported symbols as rectangles. Bundle full Fira Mono with a symbol fallback for consistent rendering across platforms, and check coverage of the characters used in UI text.

- **Portal body pose jumps at the crossing:** between floor portals, the emerging twin is upside down but the main body replaces it upright when the center crosses, swapping the visible legs for the upper body. Preserve the rendered pose across the handoff before reorienting it.

- **Body clipping flickers on moving portals:** straddle detection uses the current tick's portal frame against the interpolated player position, while the visible portal and clipping planes use the interpolated carrier pose. Use the same rendered frames for detection and clipping.

- **Actors park at the top of the basement ramps:** scuttlers and bruisers heading down a hotel basement ramp stop at the lip, one at a time, sometimes turned into the side wall. It happens with `/peace` on as well, so it is not the pursuit goal, and it predates the terrain query fix. On the hotel map in isolation the route from the landing to the basement, the motor's descent, and `character_ground_route_clear` from every point near the lip all pass; whatever stops them is in the live loop.

## Enhancements

- **Actor gameplay definitions:** Group each actor kind's health, damage, scoring, and destruction-feed setting under `gameplay.json::actors.kinds`, alongside its body and behaviour, so adding a kind does not require updating several separate tables.

- **Pressure plates cover characters' feet:** give plates collision geometry so players and other characters stand on their surface instead of intersecting the model. Keep the support height aligned with the tread in both active and inactive states.

- **Render ramps as stairs:** add an option to show ramps as stairs while retaining smooth ramp collision and movement. Make stair use configurable per actor kind, like ladder use.

- **Missiles through portals:** a missile chasing a target through a portal detonates on the aperture's backing instead of crossing, while bullets hop through; rank `projectile_hop` against the other events in the missile sweep in `client/src/missiles/movement.rs`.

- **Player-scaled actor counts:** let a spawn zone's actor count depend on the number of logged-in players instead of one fixed `count`. The scaling rule is still to be decided; define it so that later joins fill the added slots and departures let the surplus die off without a cull.

- **Rapier upgrades:** Recheck the capsule floor-motion regression before removing the contact-normal adapter in `common/src/physics/world/character_queries.rs`. It works around imprecise cast normals feeding Rapier’s slope decomposition; `running_across_flat_floor_tiles_keeps_its_speed` still fails without it on 0.35. Whatever replaces it must keep its contact query bounded, since an unbounded prediction scans the whole terrain trimesh.

## Testing

- **Shadows with active portal views:** the removed mirror previously made shadows flicker during camera motion. Recheck with portals placed and in view; Bevy shares directional-shadow layers between 3D views. This is separate from the fixed pale leaf flashes caused by inconsistent wind vertex positions between depth and color passes.

- **Outdoors after the terrain query fix:** run fast far out on the hotel's grounds and check that movement stays smooth and the frame rate holds, that chasing actors no longer stop and go, and that grass chunks appearing beside a sprint no longer stutter the frame.

- **Actors on the grounds:** Watch the hotel's bruisers and scuttlers roam past the seam, chase a player onto the hills and around trees and rocks, and return home. Check none stalls at the rim, that chases across open ground start promptly, that routes cross open floor and lawn on straight diagonals, and that corners are taken as turns at speed and reversals as pivots on the spot, with the turn rate feeling right for both kinds.

- **Actor movement sound mix:** Check the tank engine/background balance and speed-driven pitch in-game while moving, turning, stopping, climbing, and hovering. Tune the Enemy movement slider and `assets.json::actors.movement_volume_db` / `sfx_volume_db` against weapons and footsteps.

- **Startup mouse capture:** Launch windowed, wait before moving the mouse, and confirm there is no initial view jump and the first click fires. Repeat after switching away and back.

- **Shared checkpoints:** Play through Group — any and Group — all with multiple clients, including staggered visits, death, joining, and leaving. Check each player's next respawn and checkpoint notification.

- **Sliding-carrier pushing:** Let the moving cabin's wall push you while standing still, walking against it, and stepping sideways out of its path. Confirm open space is safe, being pinned against another wall still crushes, and boarding moving platforms remains safe. Crushed actors should play their normal explosion animation and sound.
