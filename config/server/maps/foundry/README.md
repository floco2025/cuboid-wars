# Foundry

A hub, courses that each chain a few portal decisions, and an exit they open:
the second building authored by an AI with `tools/mapauthor.py`, after
Gatehouse, whose rooms teach one idea each. `build.py` beside this file is
its source and writes `layout.json`; `experiment.json` is the route its tests
prove. One course, the Gallery, stands so far.

```sh
cargo run --release -- --map foundry
python3 tools/mapauthor.py build foundry
cargo run --release -- --play-experiment config/server/maps/foundry/experiment.json
```

Storeys are 1.6 m on a 1 m grid. Brick, tile, linoleum, and plaster take a
portal; metal does not. A flight keeps its speed: nothing brakes a body in
the air. A course's plate latches and opens its bar in the exit corridor.

## Hints

1. **The Gallery.** Through the barred door east of the hub you see the goal:
   a ledge across a chasm, eight storeys of metal around it. Only the band
   high on the hall's west wall takes a portal: shoot it from the chasm's
   floor, a jump up through the door from the shaft's floor, or through the
   window of the gallery, up the ladder from the landing west of the hub.
   From the landing, put the other portal on the shaft's floor well out from
   the landing. A running jump off
   the landing into the floor portal throws you short, into the chasm. A
   running jump off the gallery's west end does not: you fly out of the band
   and over the chasm onto the ledge.
