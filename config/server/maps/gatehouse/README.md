# Gatehouse

A hub whose exit three gates bar, and a room that lowers each: the first
building authored by an AI with `tools/mapauthor.py`. `build.py` beside this
file is its source and writes `layout.json`; `experiment.json` is the route
its tests prove.

```sh
cargo run --release -- --map gatehouse
python3 tools/mapauthor.py build gatehouse
cargo run --release -- --play-experiment config/server/maps/gatehouse/experiment.json
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
   roof: stand at the gate and look up. Nothing opens inside the tank or on
   the ceiling over it. Put one portal on the east wall above the tank, in its
   top two storeys, and the other on the floor, then drop in: you leave the
   wall a storey above the rim and fall into the tank. Lower on the wall and
   you fly into the tank's side. The tank's plate opens the gate and the hub's.
3. **The Firing Line.** The turret on the plinth sees the whole floor and
   kills in about three seconds; the plate inside the door raises a shield in
   front of it while you stand there. The turret cannot see into the pen
   under its own feet, where the room's plate is. From the shield plate, put
   one portal on the pen's back wall and the other on the floor beside you,
   step off and through. Walk back the same way and you will not make it;
   from inside the pen, move your second portal onto the wall by the door
   first. With two players, one holds the shield while the other walks.
