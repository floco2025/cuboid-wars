# Follow-ups

## Fixes

- **Player movement result consistency:** `client/src/players/movement/planning.rs` records the proposed motor step's support, carrier, grounding, and outcomes before `application.rs` can reject its horizontal motion against another character; airborne momentum also finishes against the proposed result. Resolve character blocking before committing one complete movement result. Reproduce and cover ledge, ramp, and moving-support contacts; the mismatch is identified by code inspection, not a confirmed playtest symptom. Keep local movement client-owned.

- **Navigation search exhaustion:** a connected route whose A* search itself exceeds the per-query budget can still fail repeatedly, including some long Hotel detours. Continue these searches across ticks while preserving the shared work cap. Exhausting optional smoothing now retains the route already found.

- **Stuck forward movement:** the player sometimes keeps walking after W is released, during ordinary play with no focus change. `input_movement_system` rebuilds the intent from `ButtonInput<KeyCode>` every frame and nothing else moves a grounded body, so a key release is lost before it reaches `ButtonInput`. Capture a session with `WAYLAND_DEBUG=client` and check whether `wl_keyboard.key` delivered the release (key 17, state 0): if not, the loss is below the game (key remapper, keyboard, or compositor); if so, trace winit and Bevy next.

- **Plank ramps:** grass grows through the low end of a plank standing on terrain or the exterior grounds.

## Enhancements

- **Client missile search budgets:** `client/src/missiles/air_graph.rs` runs synchronous, unbudgeted BFS through authored air grids, with collision sweeps during expansion; guidance periodically repeats the search. Measure worst-case search time and collision queries with multiple missiles and unreachable targets, then introduce a shared work budget, resumable searches, and explicit pending/found/unreachable outcomes while retaining safe steering or a usable route. Extend obstacle routing to bounded 3D regions beyond authored grids. Missiles need airspace routing rather than the ground navmesh.

- **Grass rebuild scheduling:** initial grass chunks build asynchronously, but `client/src/map/grass/burn.rs` rebuilds affected meshes synchronously and the initial completion system installs every ready mesh in one frame. Profile explosions and rapid streaming; move burn rebuilds to bounded background work, coalesce changes per chunk, reject stale results using revisions, and budget mesh installation. Reuse physical ramp/floor clearance when building grass exclusions to address the plank-ramp overlap fix above.

- **Dependency upgrades:** Recheck the `encase` family held at 0.12.1 in `Cargo.lock` once [Bevy's syn compatibility issue](https://github.com/bevyengine/bevy/issues/25844) is resolved; 0.12.2 fails to compile with Bevy 0.19.1. Renet's `crypto-common` dependency also pins `generic-array` to 0.14.7.

- **Obby player speed:** Once Obby is debugged, reduce `movement.player.walk_speed` and `run_speed` in `config/server/maps/obby/settings.json` to 5.0 m/s. The temporary 5.1 m/s setting makes testing easier.

- **Render ramps as stairs:** add an option to show ramps as stairs while retaining smooth ramp collision and movement. Make stair use configurable per actor kind, like ladder use.

- **Geometry and actor navigation redesign:** see [the navigation guide](ACTOR_NAVIGATION.md#remaining-work). Hotel, Obby, and Workshop now use surface navigation, normal actor controllers, endpoint ladders, docked transfers, soft separation, shared search budgets, and background rebuilding. Add authored-object provenance, richer interrupted-traversal recovery, and global route planning for detours beyond one navigation window. On-demand regions now support pursuit and return across distant collision geometry without a fixed authored-map leash. Add ordinary actor route/status inspection and experiment reset tools. The old ground graph, controller, backend selector, and temporary lab are removed; regression cases use the normal modules and authored fixtures. Exact historical behavior is not a compatibility requirement. Broader scene/rules composition can continue independently; see [the movement review](GEOMETRY_MOVEMENT_REVIEW.md) and [sandbox review](SANDBOX_ARCHITECTURE_REVIEW.md).

- **World-space content authoring:** support placing items, actor zones, objectives, and authored geometry throughout the playable world, including the surrounding terrain. The authored grid boundary should bound an editing region, not gameplay eligibility; use shared collision/support and traversal rules for content inside and outside it. Navigation already loads regions beyond the grid.

- **Sandbox simulation and experiment workflow:** separate scenes, rulesets, and repeatable experiment setups; extract gameplay stepping from rendering/audio/transport so the owning client and server roles can run headlessly. Preserve client authority for local movement and server authority for actors and synchronization, without movement reconciliation. Prove reusable body, controller, team, and loadout composition in two different experiments, then connect runtime inspection and reset to the existing editor. Introduce reusable world conditions/actions and explicit state lifetimes as those experiments need them. See [the sandbox review](SANDBOX_ARCHITECTURE_REVIEW.md).

- **Rapier upgrades:** Recheck the capsule floor-motion regression before removing the contact-normal adapter in `common/src/physics/world/character_queries.rs`. It works around imprecise convex cast normals feeding Rapier’s slope decomposition; triangle-mesh casts retain their own normals because a neighboring triangle contact can otherwise stall terrain movement. `running_across_flat_floor_tiles_keeps_its_speed` still fails without it on 0.35. Whatever replaces it must keep its contact query bounded, since an unbounded prediction scans the whole terrain trimesh.

## Testing

- **Navigation and movement agreement:** visually playtest Hotel and Obby with the new actors, plus Workshop with its configured scuttler and bruiser bodies. Extend authored-scene coverage with interrupted boarding/climbing, support removal beneath riders, dense crowds, portals, and larger maps. Instrument collision-query counts and combat-heavy full server ticks; the current Hotel/Obby compatibility test measures peaceful headless ticks. Run two clients under lag/loss with moving supports and combat; see [the guide](ACTOR_NAVIGATION.md).

- **Experiment repeatability and composition:** test restart from resolved setup/seed and recorded commands with controlled RNG, tick order, and message delivery; verify that old-session effects cannot alter a restarted experiment. Offline reproduction of networked failures also needs the incoming state each owner observed. Compare two rulesets in the same scene and measure edit-to-play time and unrelated systems touched by a new mechanic. Exercise two clients using moving supports, portals, and projectiles under the existing lag/loss controls; verify immediate local movement, eventual state repair, and existing protocol guarantees without requiring identical intermediate states or live movement replay. See [the sandbox review](SANDBOX_ARCHITECTURE_REVIEW.md).
