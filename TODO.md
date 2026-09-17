# Follow-ups

## Fixes

- **Client memory:** the hotel client sits at ~2.0 GB RSS with mimalloc, while forcing every freed block back to the OS measured the live set at ~1.2 GB; the rest is allocator slack behind the load-time peak, when every texture decodes before the mipmap pass releases it. Lower that peak (decode and release the textures a few at a time) or tune mimalloc's purge if the gap matters.

- **Portal body pose jumps at the crossing:** between floor portals, the emerging twin is upside down but the main body replaces it upright when the center crosses, swapping the visible legs for the upper body. Preserve the rendered pose across the handoff before reorienting it.

- **Walking across a floor portal does not cross it:** the aperture backing is excluded while the body's centre is inside the aperture rectangle, and at obby's walk speed and gravity the centre walks out of the short axis before it has sunk to the plane. The body then stands inside the slab and surfaces over most of a second instead of emerging from the exit; a jump in crosses because it reaches the plane sooner. Keep the backing excluded until the body has cleared it, or judge the aperture by the capsule rather than its centre.

- **Body clipping flickers on moving portals:** straddle detection uses the current tick's portal frame against the interpolated player position, while the visible portal and clipping planes use the interpolated carrier pose. Use the same rendered frames for detection and clipping.

## Enhancements

- **Shared editor/game map logic:** evaluate a Rust core for map source types, validation, normalization, and geometry rules, preserving invalid authored data and structured editor diagnostics. Consider a thin Python binding for the existing PySide6 UI before a full Rust editor rewrite; assess a full rewrite separately if game-rendered 3D previews become a goal.

- **Obby player speed:** Once Obby is debugged, reduce `movement.player.walk_speed` and `run_speed` in `config/server/maps/obby/settings.json` to 5.0 m/s. The temporary 5.1 m/s setting makes testing easier.

- **Pressure plates cover characters' feet:** give plates collision geometry so players and other characters stand on their surface instead of intersecting the model. Keep the support height aligned with the tread in both active and inactive states.

- **Render ramps as stairs:** add an option to show ramps as stairs while retaining smooth ramp collision and movement. Make stair use configurable per actor kind, like ladder use.

- **Rapier upgrades:** Recheck the capsule floor-motion regression before removing the contact-normal adapter in `common/src/physics/world/character_queries.rs`. It works around imprecise cast normals feeding Rapier’s slope decomposition; `running_across_flat_floor_tiles_keeps_its_speed` still fails without it on 0.35. Whatever replaces it must keep its contact query bounded, since an unbounded prediction scans the whole terrain trimesh.

## Testing

- **Sliding-carrier pushing:** Let the moving cabin's wall push you while standing still, walking against it, and stepping sideways out of its path. Confirm open space is safe, being pinned against another wall still crushes, and boarding moving platforms remains safe. Crushed actors should play their normal explosion animation and sound.
