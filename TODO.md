# Follow-ups

## Fixes

## Enhancements

- **Dependency upgrades:** Recheck the `encase` family held at 0.12.1 in `Cargo.lock` once [Bevy's syn compatibility issue](https://github.com/bevyengine/bevy/issues/25844) is resolved; 0.12.2 fails to compile with Bevy 0.19.1. Renet's `crypto-common` dependency also pins `generic-array` to 0.14.7.

- **Obby player speed:** Once Obby is debugged, reduce `movement.player.walk_speed` and `run_speed` in `config/server/maps/obby/settings.json` to 5.0 m/s. The temporary 5.1 m/s setting makes testing easier.

- **Render ramps as stairs:** add an option to show ramps as stairs while retaining smooth ramp collision and movement. Make stair use configurable per actor kind, like ladder use.

- **Rapier upgrades:** Recheck the capsule floor-motion regression before removing the contact-normal adapter in `common/src/physics/world/character_queries.rs`. It works around imprecise cast normals feeding Rapier’s slope decomposition; `running_across_flat_floor_tiles_keeps_its_speed` still fails without it on 0.35. Whatever replaces it must keep its contact query bounded, since an unbounded prediction scans the whole terrain trimesh.

## Testing
