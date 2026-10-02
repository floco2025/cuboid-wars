# Follow-ups

## Fixes

- **Navigation search exhaustion:** a connected route whose A* search exceeds the per-query budget fails repeatedly, including some long Hotel detours. Continue such searches across ticks under the shared work cap.

## Enhancements

- **Player stair stepping:** the motor steps over ledges up to `CHARACTER_STEP_HEIGHT`, 0.2 m; taller is a wall. No shipped map needs more (ramps, ladders, jumps). Needed once a map authors stairs or generated courses put knee-high blocks in the way; Source steps 0.45 m at our scale. Raising Rapier's autostep alone stalled a 0.4 m step, since the capsule's rounded bottom catches the ledge's edge: it needs a capsule-aware step solver. Keep the actor limit, which the navigation mesh assumes; cover low ceilings, ledges narrower than the body, and steps onto carriers.

- **Missile search cleanup:** in `client/src/missiles/search.rs` every neighbour edge repeats its node's start-overlap query, about a third of the budget; judge edges by travel alone. `AirGraph::endpoint_candidates` probes every carrier grid at any distance; apply the reach gate `neighbors` uses. `MissileFlight.route_status`, `RouteStatus`, `SearchBudget.used`, and `AirGraph::path` are test-only code in production files.

- **Dependency upgrades:** `encase` is held at 0.12.1 until [Bevy's syn issue](https://github.com/bevyengine/bevy/issues/25844) is resolved; 0.12.2 fails with Bevy 0.19.1. Renet pins `generic-array` to 0.14.7.

- **Render ramps as stairs:** an option to draw ramps as stairs over the smooth collision; stair use per actor kind, like ladders.

- **Surface navigation:** plan detours larger than one window globally. Bake in tiles: a field change rebuilds a whole region over seconds, and Hotel sits near the bake limit. Recover from interrupted mounts, climbs, exits, and boardings, which only hold and then replan from off the mesh. Standoffs end only between two movers; a longer cycle or a mover behind an immovable body still waits. Transfers connect carriers only at authored endpoints, so moving carriers meeting, mid-rung attacks, and actor portal routes are out. Track compiled geometry back to authored objects; add route inspection and experiment reset.

- **World-space content authoring:** place items, zones, objectives, and geometry anywhere in the playable world, terrain included. The grid bounds editing, not gameplay; navigation already loads regions beyond it.

- **Pressure-plate sizing:** author plate size and activation area independently of the grid. The Primer's 2 m grid gives 1 m plates.

- **Editor preview scope:** Jump Path flies open air at full speed from a tile edge. Missing: the run-up, obstacles, ramps, and bridges stopping a flight (a flight under an overhanging slab reads as clear), crouch, ramp takeoffs, mid-flight equipment or gravity changes, switches, carriers, and a wall-portal capture outline. Steering samples fixed directions and one entry rule; search the input instead. `mapauthor jump` and `fling` inherit all of it.

- **Experiment runner on a dead player:** an `aim` after a death is a process error that discards the report; record it as a failed action like `check` does, so a proof run shows where the route died.

- **Authoring the next courses:** [PLAN.md](PLAN.md) says what an AI-authored map should have; `tools/mapauthor.py` cannot yet express or verify most of it. The builder lacks rooms (walls, ceiling, doorways), wall lights, actor zones, and a texture palette beyond `solid` and `portal`, and its portal pads and walls assume 2 m cells. `surface`, `jump`, and `fling` judge portals by Python rules that assume the same grid and exclude ramps: ask the game's own placement instead, and add what a shot reaches from a standing point and a sweep over every reachable portal pair, which is what shows a wrong surface fails and a route is necessary. Keys are lost on death, plates scale with the grid, actor behaviour is unseeded, and nothing shows the author how a map looks.

- **Sandbox simulation and experiment workflow:** toward AI-generated movement and portal maps that are fun; [PLAN.md](PLAN.md) has the goal. Extend `--experiment` with carriers, route and landing-tolerance measurements, and seeded randomness. Separate scenes, rulesets, and setups; keep client movement authority and server actor authority, no reconciliation. Prove reusable body, controller, team, and loadout composition, then connect inspection and reset to the editor.

- **Rapier upgrades:** the contact-normal adapter in `common/src/physics/world/character_queries.rs` works around imprecise convex cast normals; `running_across_flat_floor_tiles_keeps_its_speed` fails without it on 0.35. Any replacement must keep its contact query bounded, or it scans the whole terrain trimesh.
