# Portal Relay

A single-player traversal course with three portal setups, two running jumps,
a bridge switch, and five checkpoints. The challenges change from finding a
route, to controlling a landing, to carrying fall speed through portals and
steering in the air. There are no enemies.

Run from the repository root:

```sh
cargo run --release -- --map portal_relay --look 270,-10
```

Edit its map with:

```sh
python3 tools/editor.py portal_relay
```

The layout, settings, and `experiment.json` are together in
`config/server/maps/portal_relay/`. The script references the adjacent files, so
editor changes apply to both play and route tests.

The authoring grid is **2 m**, with **2.2 m level spacing** and **0.2 m floor
and wall thickness**. Each wall section is 2 m long and 2 m high; the portal
walls use two stacked sections. Platforms use several cells, so refining the
grid preserves the course's world dimensions and floor elevations. Portal
and player sizes stay unchanged.

Both lower entrance pads are shifted 2 m outward from their upper ledges,
leaving 1.8 m of clear horizontal gap after the slab overhangs. Each pad is
still 4 m square. Pressure plates currently scale with the grid: their active
square is now 1 m wide, so aim for the middle of the cyan plate when landing.

WASD moves, Shift runs, Space jumps. Left click places portal A; right click
places portal B. The portal gun is your only weapon and survives death. Pale
panels accept portals; dark platforms do not. Numbered checkpoint flags mark
the route. The gold plate on the final platform starts the fireworks.

## Hints

1. **Turn onto the balcony.** Put A on the nearby wall and B on the wall behind
   checkpoint 1 across the gap. Enter A. Keep walking through the turn, then
   stop on the balcony.
2. **Make the bridge.** Turn toward the cyan plate on the small isolated island.
   Run and jump across the gap, then land on the plate to activate the bridge.
   Cross to checkpoint 2. The plate toggles the bridge, so avoid stepping on it
   again before crossing. A single-player death resets the bridge for another
   attempt; checkpoints remain saved.
3. **Trade height for distance.** On the next terrace, approach the edge above
   the small pale floor. Put A on that floor, and B on the isolated wall facing
   the large checkpoint 3 platform across the void. Stand near the edge; the
   gap lets the floor shot clear its lip before takeoff. Walk off toward A,
   release movement once centered over it, and let the fall launch you sideways
   onto checkpoint 3.
4. **Launch upward and catch the ledge.** From checkpoint 3, find the pale floor
   at the bottom of the deeper well. Put A there and B on the isolated pale
   floor near the raised checkpoint 4 platform. Drop into A and release movement
   over it. As you rise from B, run sideways toward checkpoint 4 and stop over
   the platform. The height of the drop supplies the upward speed; an ordinary
   jump from the exit floor cannot reach the ledge.
5. **Jump diagonally to the finish.** Move toward the far corner of checkpoint 4,
   then run and jump diagonally to checkpoint 5. Walk onto the gold plate.

## Watch the scripted route

```sh
cargo run --release -- --play-experiment config/server/maps/portal_relay/experiment.json --look 270,-10
```

Space plays/pauses the entire sequence. Enter runs the next action (or finishes
the current one) and pauses at its end. R restarts, paused. Esc opens the settings
menu and pauses playback. Mouse look, zoom, and V let you inspect the frozen scene.
See [EXPERIMENTS.md](../../../../EXPERIMENTS.md) for the script format and scope.

## Automated route

```sh
cargo run --release -- --experiment config/server/maps/portal_relay/experiment.json > /tmp/portal-relay-report.json
cargo test --release -p cuboid-wars course_tests
```

The script places both portals through ordinary placement rays and traverses the
whole course using the client movement motor. Its checks require grounded
landings. Tests verify all five checkpoint claims in order, bridge activation,
three portal crossings, and the finish cue without death or fall damage. They
also exercise several air-steering delays spanning 0.4 seconds, missing the
high ledge and respawning at checkpoint 3, and bypassing the plate while the
bridge remains off. Both lower entrance portals are also tested from grounded
positions inside the upper ledges, with their shots clearing the slab lips.

These checks establish that the route works in simulation. Camera visibility,
how readily players find the route, and whether the pacing feels fun still need
a human playtest. This is one authored course for exercising the tools, not a
procedural map generator.
