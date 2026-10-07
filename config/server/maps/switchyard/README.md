# Switchyard

An abandoned station whose doors can travel without you. Two wings can be
explored in either order; their keys unlock the signal room and a final trip
to the roof. The challenge is remembering a useful route while the building
changes around it. No enemies, boosted movement, or precision flings.

```sh
cargo run --release -- --map switchyard
cargo run --release -- --play-experiment switchyard
python3 tools/mapauthor.py build switchyard
cargo test --release -p cuboid-wars switchyard
```

Brick, wallpaper, tile, and plaster accept portals. Metal does not. Orange
and violet plates toggle their machines; completion plates open permanent
shortcuts. Keys respawn immediately for another player or another life.
A checkpoint inside the signal room preserves access to the finale.

## Gentle hints

- **Freight:** a portal belongs to the surface it was fired onto.
- **Archive:** a key lets you through every part of its field. Even the parts
  you were standing on. Explore before collecting it.
- **Signal room:** the ceiling is part of the machine. Watch where it goes.

## Walkthrough

1. **Freight.** Take the north passage from the concourse. The orange plate
   dispatches the car and closes the loading door; pressing it again recalls
   the car. Before dispatching, put a portal on a wall inside the car and
   the other in the station. Send the car away, then step through. Its new
   door opens into the arrival hall. The cyan plate opens the return door;
   collect the cyan key beside it and walk back to the concourse.
2. **Archive.** The west doorway opens onto the green floor. Leave one portal
   on the brick wall beside the entrance. Cross the green floor to the far
   south end, round the end of the metal divider, and look back into the
   vault's south-facing doorway. Put the other portal inside. Return to the
   entrance walkway and collect the green key. The green floor now lets you
   fall through it, but your portals still provide a route to the vault.
   Walk through its green gate, collect the amber key, and press the pale
   plate. It opens a side door and a new bridge straight back to the entry.
3. **Signal room.** Both keys let you through the east ticket gates. Look
   south and up at the plaster underside of the sliding canopy. Put a portal
   on its north half, and another low on a brick wall. The violet plate sends
   the canopy north, behind the balcony's screen. Walk through the wall
   portal to drop onto the balcony, then take its north doorway to the
   terrace and press the white signal plate.

## Recovery and co-op

An early green key leaves a second solution: drop to the maintenance floor,
climb the short ramp in the south-west corner, and aim into the vault from
its inspection perch. The east ladder returns to the entrance walkway.
Neither recovery requires dying or discarding the key. Using the canopy
before dispatch drops onto a safe catch deck; its ramp leads back down.

The freight and archive shortcuts stay open after a death. Keys remain
available at their original locations. Co-op keeps the game's shared portal
pair: one player can prepare an end while the other operates a machine, or
ride in the freight car while their partner dispatches it. Each player can
collect the keys; the layout does not require an extra player.

## Authoring and checks

`build.py` is the geometry source. It uses the builder's embedded geometry
and moving-map placements for the freight car and canopy; `layout.json`
is generated. The map inherits the shared movement, gravity, grid, portal
size, and equipment rules unchanged.

`experiment.json` completes the route with ordinary movement and aimed
shots, without teleports or remote placement. Regression tests cover both
wing orders, early-key recovery, the loading interlock and recall, missing
keys, the undispatched car and canopy, and blocked sightlines. Carrier tests
also cover riding through a reversal, carrier-local body placement, and
pausing graphical playback. These checks establish tested routes and
failure cases, not an exhaustive search of all possible shortcuts.
