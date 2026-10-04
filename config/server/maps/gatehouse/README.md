# Gatehouse

A hub whose exit four gates bar, and a room that lowers each: the first
building authored by an AI with `tools/mapauthor.py`. `build.py` beside this
file is its source and writes `layout.json`; `experiment.json` is the route
its tests prove.

```sh
cargo run --release -- --map gatehouse
python3 tools/mapauthor.py build gatehouse
cargo run --release -- --play-experiment gatehouse
```

Storeys are 1.6 m on a 1 m grid: a jump clears one and not two. Brick, tile,
and plaster take a portal; metal does not. The portal gun is your only tool:
left click places portal A, right click portal B. A room's plate latches:
once pressed it stays pressed. The Firing Line's resets when you die.

## Hints

1. **The Drop.** From the landing, look down the pit at the metal block with
   the plate on top. Put one portal on the pit's floor below you and the other
   on the floor beside the block, then walk off the landing. The fall throws
   you up past the block's top; hold toward it to land there. Jump off the
   block into the pit and climb the ladder back to the landing.
2. **The Cistern.** The plate stands at the bottom of a metal tank with no
   roof: stand at the gate and look up. Nothing opens inside the tank, high
   on the hall's walls, or on its ceiling, and a hop into a portal in the hall
   rises short of the rim. It takes a long fall: put one portal on the hall
   floor a few steps from the tank, shooting it while you face the tank, then
   walk off the Drop's landing into the other on its pit floor. You come up
   out of the hall floor high over the rim and drop inside. The tank's plate
   opens the gate and the hub's.
3. **The Firing Line.** The turret on the plinth sees the whole floor and
   kills in under a second; the plate on the threshold raises a shield in
   front of it while you stand there. The turret cannot see into the pen
   under its own feet, where the room's plate is. From the shield plate, put
   one portal on the pen's back wall. Step back into the foyer, out of the
   turret's sight, put the other on the foyer's north wall, and walk through;
   the same pair brings you back unseen. With two players, one holds the
   shield while the other walks.
4. **The Vat.** Another open tank, behind the second door south of the hub.
   The doorway is an eraser: it closes the portals you bring in, so the
   answer is in the room. The floor and everything high up are metal, and so
   is the ceiling over the vat and just around it. Put one portal low on the
   west wall and the other on the ceiling just west of the vat, shooting it
   from south of that spot, then run into the wall portal heading north-west:
   you drop out of the ceiling drifting east, over the rim and in. A straight
   run drops you beside the vat; the way the ceiling portal was shot and the
   angle of the run decide where the drift goes.
