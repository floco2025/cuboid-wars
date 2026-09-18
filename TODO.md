# Follow-ups

## Fixes

## Enhancements

- **Obby player speed:** Once Obby is debugged, reduce `movement.player.walk_speed` and `run_speed` in `config/server/maps/obby/settings.json` to 5.0 m/s. The temporary 5.1 m/s setting makes testing easier.

- **Render ramps as stairs:** add an option to show ramps as stairs while retaining smooth ramp collision and movement. Make stair use configurable per actor kind, like ladder use.

- **Rapier upgrades:** Recheck the capsule floor-motion regression before removing the contact-normal adapter in `common/src/physics/world/character_queries.rs`. It works around imprecise cast normals feeding Rapier’s slope decomposition; `running_across_flat_floor_tiles_keeps_its_speed` still fails without it on 0.35. Whatever replaces it must keep its contact query bounded, since an unbounded prediction scans the whole terrain trimesh.

## Testing

- **Network convergence:** verify that status, health, scores, portals, and actor presence converge after loss/reordering subsides, delayed messages drain, and fresh snapshots arrive. Temporary intermediate inconsistencies are accepted under the [protocol contract](common/src/protocol.rs). Also check the existing body-generation, movement-ordering (including wraparound), and reliable-event guarantees. Passing login/snapshot/disconnect checks alone does not establish this coverage.

- **Platform and rendering coverage:** repeat focused checks on macOS and Windows, including the first visible startup frames and loading-to-game transition; inspect day/night/rain and portals under both renderers, listen to spatial audio, and run a multiplayer soak. Reproduce the Linux startup cursor-position error alongside focus/recapture testing before changing cursor behavior.

- **Sliding-carrier pushing:** Let the moving cabin's wall push you while standing still, walking against it, and stepping sideways out of its path. Confirm open space is safe, being pinned against another wall still crushes, and boarding moving platforms remains safe. Crushed actors should play their normal explosion animation and sound.
