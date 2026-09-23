# Portal Choices

A single-player course about preparing a route, choosing a launch angle, and
changing equipment during flight. It uses a 2 m grid, four checkpoints, three
portal traversals, and no enemies.

```sh
cargo run --release -- --map portal_choices --look 90,-10
python3 tools/editor.py portal_choices
```

The portal gun is always available. Pale surfaces accept portals. Yellow
chevrons grant speed, the white arrow grants low gravity, and the pink X erases
collected equipment. Pickups can float without a supporting floor. They remain
available for retries; an eraser waits if you have nothing it can remove.

## Map design rule

Never require the player to stand at a platform's edge and look straight down
to discover or aim at the next landing or portal pad. Lower targets need enough
horizontal separation to be visible from a safe position on the approach.
Scale that separation with the drop height; a single empty grid cell is not
sufficient for a deep drop. Verify both the sightline and the playable jump
when changing the spacing.

## Route hints

1. **Prepare the exit from the balcony.** The screen in front of the starting
   ledge blocks the shot to the distant portal wall. Walk east along the back
   of the upper balcony, place B on that wall, then return west and follow
   the long arm toward the lower pale pad. The balcony is set back from the
   landing so jumping down cannot replace the fling. Place A on the pale
   floor ahead and below, which is separated from the ledge by a 6 m gap.
   Run off toward its center, then release movement when above the portal.
   The horizontal fling lands at checkpoint 1 and collects
   speed. The exit wall faces the landing; the shooting position gives access
   to that face, rather than changing its angle.
2. **Run into the inclined launch.** From the landing, place B on the distant
   ramp facing you before approaching the nearby wall. Put A on that wall,
   align with its center, and run through. Speed supplies the upward velocity
   needed to reach checkpoint 2. Keep running toward the upper platform after
   exiting. The pink X near the landing clears your boosts. Another speed
   pickup on the entry runway lets you replenish it before launching.
   If you miss the landing or lose speed on the ramp, drop onto the retry
   deck beneath it. Collect speed there, place B on the ramp from beside
   the pickup, then put A on the low wall at the deck's far end and run
   through. You can also reach this deck by dropping off the west end of
   the upper platform, without taking fall damage.
3. **Use low gravity to reach the high ledge.** Collect the white arrow on
   checkpoint 2's platform, then run and jump across to the higher ledge on
   the east side. Normal gravity cannot reach its height. Checkpoint 3 has
   another low-gravity pickup for retries; make sure it is active before the
   final drop.
4. **Choose the final ramp and equipment transition.** Approach the front of
   the high ledge, staying about 1 m back from its edge. You can aim at the
   lower entrance pad and the three inclined exit surfaces from here.
   Those ramps surround the same finish platform.
   Each launches toward that platform, from a different side. The finish is
   beyond a direct low-gravity running jump from the high ledge.
   The steepest ramp, beside the pink X,
   provides the shallowest upward launch. Put B on its slope, then A on the
   pale floor ahead and below your ledge. Its near edge is 22 m beyond the
   upper platform, so it is visible at a normal downward angle.
   With low gravity active, hold Shift and jump toward A. Keep moving until
   you are above the portal, then release movement and fall through it.
   The shorter gap also allows a walking-speed jump with low gravity;
   ordinary sprinting gives more room for steering. A speed pickup is
   unnecessary. Keep low gravity active until you pass through the portal.
   You collect the suspended X shortly after exiting B.
   Returning to normal gravity brings you down onto checkpoint 4. Keeping
   low gravity carries you past it; using normal gravity during the whole
   drop builds too much entry speed. The other ramps send you above the
   same finish, toward speed or low-gravity pickups instead of erasure.
   Their unsteered flights overshoot it.
5. **Finish.** Land on the final platform and touch its gold plate.

The eraser removes collected speed, low gravity, other collected abilities,
and missile ammunition. It preserves the always-active portal gun, keys,
health, and checkpoint progress. It changes active abilities, not existing
velocity: gravity acts on the flight from that point onward.

## Replay and validation

```sh
cargo run --release -- --play-experiment config/server/maps/portal_choices/experiment.json --look 90,-10
cargo test --release -p cuboid-wars choices_tests
```

Space plays/pauses, Enter completes one action, R restarts, and Esc opens the
menu and pauses. The map, settings, and replay are adjacent files, as in the
other [experiments](../../../../EXPERIMENTS.md).

Automated checks cover the complete route, clear portal shots from grounded
approach positions with at most 60 degrees of downward aim, blocked shots
from the starting ledge, direct jump and drop attempts, the speed and low-gravity requirements,
and alternative ramps flying too high across the actual finish. They also
check ordinary gravity during the final drop and overshooting without the
suspended eraser even with full countersteering after transit. The intended
final flight accepts varied portal aim and running-jump approach timing; the
first drop also accepts varied run-off timing. Tests with every speed pickup
removed cover walking-speed jumps, sprinting from five takeoff positions,
and running off without jumping, all the way through the portal to the finish.
Recovery checks
complete a launch after reacquiring speed on the entry runway, after walking
off the ramp, and after returning from the upper eraser. Death and respawn
are tested at every checkpoint, with the pickups still available.

These checks establish a working route and specific failed alternatives, not
an exhaustive proof that no other route exists. Discoverability, visual aiming,
and how forgiving the choices feel still need a manual playtest.
