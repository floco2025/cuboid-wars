# Follow-ups

## Fixes

- **Editor Connections visibility and focus:** make View → Connections control both the connection list and canvas highlights. While enabled, selecting a pressure plate or controlled object must immediately refresh and reveal its connections without an extra tab click; Properties must not cover the Connections panel. Turning it off hides both the panel and overlays.

- **Client memory:** the hotel client sits at ~2.0 GB RSS with mimalloc, while forcing every freed block back to the OS measured the live set at ~1.2 GB; the rest is allocator slack behind the load-time peak, when every texture decodes before the mipmap pass releases it. Lower that peak (decode and release the textures a few at a time) or tune mimalloc's purge if the gap matters.

- **Portal body pose jumps at the crossing:** between floor portals, the emerging twin is upside down but the main body replaces it upright when the center crosses, swapping the visible legs for the upper body. Preserve the rendered pose across the handoff before reorienting it.

- **Body clipping flickers on moving portals:** straddle detection uses the current tick's portal frame against the interpolated player position, while the visible portal and clipping planes use the interpolated carrier pose. Use the same rendered frames for detection and clipping.

## Enhancements

- **Complete Properties and unify object editing:** give Properties the full controls, helpers, and validation of the existing right-click Edit dialogs, including Apply to all faces, Use top-left materials, portal permissions, spawn-zone first level and span, and complete nested-map motion controls. Support mixed-value batch editing and undo through shared editing controls. Once feature parity is complete, use Properties as the single editor for existing objects: right-click → Edit opens and focuses it, replacing the duplicate Properties/Edit entries and separate object-edit dialogs. Open it only on explicit request; ordinary selection updates it while already open.

- **Compact editor panels:** reduce the left and right panels’ default and minimum widths to return space to the canvas. Use a compact tool palette with icons and short labels, with an optional collapsed icon strip; organize complete property controls within a narrower panel. Make both panels freely resizable, remember their widths, and provide easy collapse plus a canvas shortcut to temporarily hide or restore both.

- **Compact editor dropdowns:** use compact, consistent widths for closed selection boxes, let opened lists expand for long names, and reduce excess internal padding and row spacing while preserving readability. Keep short labels beside their fields; stack longer labels where needed. Narrow the containing panels too so the saved space returns to the canvas instead of becoming blank panel space.

- **Editor selection and erasing:** make click selection target a specific object and drag selection target a group, with Delete/Backspace removing only the selected objects and undo restoring them; distinguish this from deliberate tile-area operations. Reconsider the standalone general Erase tool once object deletion is complete. Keep tool-specific Place/Erase controls and the E toggle directly accessible; routine erasing must not require configuring Elements filters.

- **Shared editor/game map logic:** evaluate a Rust core for map source types, validation, normalization, and geometry rules, preserving invalid authored data and structured editor diagnostics. Consider a thin Python binding for the existing PySide6 UI before a full Rust editor rewrite; assess a full rewrite separately if game-rendered 3D previews become a goal.

- **Editor play from here:** add a playtest action that starts the game at a chosen map location and returns to the same editor view. Define how unsaved edits reach the playtest without changing authored spawn zones.

- **Obby player speed:** Once Obby is debugged, reduce `movement.player.walk_speed` and `run_speed` in `config/server/maps/obby/settings.json` to 5.0 m/s. The temporary 5.1 m/s setting makes testing easier.

- **Pressure plates cover characters' feet:** give plates collision geometry so players and other characters stand on their surface instead of intersecting the model. Keep the support height aligned with the tread in both active and inactive states.

- **Render ramps as stairs:** add an option to show ramps as stairs while retaining smooth ramp collision and movement. Make stair use configurable per actor kind, like ladder use.

- **Rapier upgrades:** Recheck the capsule floor-motion regression before removing the contact-normal adapter in `common/src/physics/world/character_queries.rs`. It works around imprecise cast normals feeding Rapier’s slope decomposition; `running_across_flat_floor_tiles_keeps_its_speed` still fails without it on 0.35. Whatever replaces it must keep its contact query bounded, since an unbounded prediction scans the whole terrain trimesh.

## Testing

- **Sliding-carrier pushing:** Let the moving cabin's wall push you while standing still, walking against it, and stepping sideways out of its path. Confirm open space is safe, being pinned against another wall still crushes, and boarding moving platforms remains safe. Crushed actors should play their normal explosion animation and sound.
