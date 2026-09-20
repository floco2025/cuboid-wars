# Follow-ups

## Fixes

- **Navigation search exhaustion:** a connected route whose A* search itself exceeds the per-query budget can still fail repeatedly, including some long Hotel detours. Continue these searches across ticks while preserving the shared work cap. Exhausting optional smoothing now retains the route already found.

- **Stuck forward movement:** the player sometimes keeps walking after W is released, during ordinary play with no focus change. `input_movement_system` rebuilds the intent from `ButtonInput<KeyCode>` every frame and nothing else moves a grounded body, so a key release is lost before it reaches `ButtonInput`. Capture a session with `WAYLAND_DEBUG=client` and check whether `wl_keyboard.key` delivered the release (key 17, state 0): if not, the loss is below the game (key remapper, keyboard, or compositor); if so, trace winit and Bevy next.

## Enhancements


- **Missile search cleanup:** every neighbour edge in `client/src/missiles/search.rs` repeats its node's start-overlap query through `sweep_clear`, about a third of the search budget; judge edges by travel alone and charge one query. `AirGraph::endpoint_candidates` probes every carrier grid at any distance; apply the reach gate `neighbors` uses. `MissileFlight.route_status`, `RouteStatus`, and `SearchBudget.used` are written in production and read only by tests, and `AirGraph::path` is a test helper in the production file.

- **Dependency upgrades:** Recheck the `encase` family held at 0.12.1 in `Cargo.lock` once [Bevy's syn compatibility issue](https://github.com/bevyengine/bevy/issues/25844) is resolved; 0.12.2 fails to compile with Bevy 0.19.1. Renet's `crypto-common` dependency also pins `generic-array` to 0.14.7.

- **Obby player speed:** Once Obby is debugged, reduce `movement.player.walk_speed` and `run_speed` in `config/server/maps/obby/settings.json` to 5.0 m/s. The temporary 5.1 m/s setting makes testing easier.

- **Render ramps as stairs:** add an option to show ramps as stairs while retaining smooth ramp collision and movement. Make stair use configurable per actor kind, like ladder use.

- **Surface navigation:** Plan detours larger than one navigation window globally; a search that exceeds its own query budget is the Fix above. Bake in tiles: a field change still rebuilds a whole region over seconds while the previous mesh serves, and Hotel's preloaded region sits near the four-million-column bake limit, which a larger grid or roam extension would exceed. Improve recovery after interrupted boarding/climbing and support removal. Transfers connect a carrier to its parent only at authored motion endpoints, so arbitrary meetings between moving carriers, mid-rung attacks, and actor portal routes remain outstanding. Track compiled collision geometry back to authored objects, and add ordinary actor route/status inspection and experiment reset tools.

- **World-space content authoring:** support placing items, actor zones, objectives, and authored geometry throughout the playable world, including the surrounding terrain. The authored grid boundary should bound an editing region, not gameplay eligibility; use shared collision/support and traversal rules for content inside and outside it. Navigation already loads regions beyond the grid.

- **Sandbox simulation and experiment workflow:** separate scenes, rulesets, and repeatable experiment setups; extract gameplay stepping from rendering/audio/transport so the owning client and server roles can run headlessly. Preserve client authority for local movement and server authority for actors and synchronization, without movement reconciliation. Prove reusable body, controller, team, and loadout composition in two different experiments, then connect runtime inspection and reset to the existing editor. Introduce reusable world conditions/actions and explicit state lifetimes as those experiments need them.

- **Rapier upgrades:** Recheck the capsule floor-motion regression before removing the contact-normal adapter in `common/src/physics/world/character_queries.rs`. It works around imprecise convex cast normals feeding Rapier’s slope decomposition; triangle-mesh casts retain their own normals because a neighboring triangle contact can otherwise stall terrain movement. `running_across_flat_floor_tiles_keeps_its_speed` still fails without it on 0.35. Whatever replaces it must keep its contact query bounded, since an unbounded prediction scans the whole terrain trimesh.

## Testing

- **Client playtest:** check crowded ledges, ramps, and moving supports in Hotel/Obby; missile pursuit through moving rooms and around exterior obstacles with several simultaneous launches; and grass streaming, explosion recovery, and low plank clearance. Headless regression and budget tests cover these systems; visual frame pacing and appearance still need an in-game pass.
