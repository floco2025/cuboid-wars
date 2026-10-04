# Foundry

A hub, courses that each chain a few portal decisions, and an exit they open:
the second building authored by an AI with `tools/mapauthor.py`, after
Gatehouse, whose rooms teach one idea each. `build.py` beside this file is
its source and writes `layout.json`; `experiment.json` is the route its tests
prove: three courses, the Gallery, the Slopes, and the Float.

```sh
cargo run --release -- --map foundry
python3 tools/mapauthor.py build foundry
cargo run --release -- --play-experiment foundry
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
2. **The Slopes.** The door in the hub's south wall, east of the exit, opens
   on a balcony over a pit. Across it stands a metal ledge with the plate,
   too high to climb, and on the pit floor two ramps face it, a gentle one
   and a steep one, both taking a portal. A portal throws you out square to
   its surface, so the steeper ramp throws you flatter: into the ledge's
   face. A fall from the balcony is too slow for either. Shoot the gentle
   ramp from the balcony and the chute's floor through its mouth on the pit
   floor, climb the ladder in the shaft against the balcony's west wall to the loft, and drop
   through its hatch: eight storeys down the chute, out of the gentle ramp,
   and onto the ledge. Its door leads back to the hub.
3. **The Float.** The door in the hub's west wall leads through an eraser
   into the stack, a metal chimney with a perch and the plate high on its
   west wall. Low gravity hangs in its south-west corner, out of a jump's
   reach. With it a jump floats high but not to the perch, and a fall from
   anywhere else in the stack is too slow without it. Fall heavy and rise
   light: put one portal on the chute's floor, shot through its mouth, and
   the other in the corner under the low gravity, climb the ladder in the
   shaft by the vestibule to the loft, and drop through its hatch. You come
   up out of the corner, catch the low gravity on the way, and float past
   the perch; steer onto it. Float down and leave through the eraser, which
   takes the low gravity back.
