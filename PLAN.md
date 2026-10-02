# Plan

The high-level plan for the game, for the human and the AI working on it.
Follow-ups are in [TODO.md](TODO.md), code conventions in
[AGENTS.md](AGENTS.md), and the map authoring guide in
[tools/map_author/README.md](tools/map_author/README.md).

## Goal

A fun game of exploration and puzzles, portal puzzles above all, whose levels
an AI authors with some human guidance. It borrows from Portal and Portal 2
without cloning them: other elements mix in, and the game should become its
own. Everything, game and tools alike, may change to get there.

## Priorities

1. Single-player.
2. Two-player co-operative play.

More players and player against player are not priorities now. Combat is
seasoning, not the core.

## What is fun today

By playtest:

- **Obby's portal part.** Closed rooms that take a portal on every wall,
  inside and out, with plates inside that switch things elsewhere.
- **Moving platforms,** to a degree.
- **Hotel as a whole:** exploring the building, finding the keys, solving the
  quests.

Earlier AI-generated maps were boring and primitive. Appearance, enemies, and
decoration alone do not fix that, and neither do more platforms and repeated
jumps. Complexity comes from connected decisions: preparing a route, choosing
an exit surface, building the right entry velocity, and changing equipment at
the right point.

## How we work

- **The AI authors, the human guides.** The aim is an AI that can author such
  maps, not one map refined by hand. When a map needs something the tools
  cannot express or check, that is a tool follow-up in TODO.md, not a hand
  edit to a layout.
- **Playtests are precious.** The human makes time for them, and that time is
  not cheap. The AI first checks everything it can check itself (a route
  exists, wrong moves fail, there is no shortcut), so a playtest is spent on
  what only a player can judge: whether a map is readable and fun.
- **Lessons stick.** What a playtest teaches is written here as a principle,
  so it is not learned twice.
- **Obby is edited by hand.** The AI does not change its layout or retune its
  movement.

## Design principles

- **Rooms and buildings.** A portal chamber is a room: walls and a ceiling
  bound where a body and a shot can go, which is what makes its portals
  necessary and what reads as a portal puzzle. Hotel is the model for
  architecture and textures: rooms and corridors under ceilings, materials
  chosen by role. Open-air platforms leave every gap to a jump.
- **Exploration, not one line.** Several courses chosen independently from a
  start, each yielding a key, with every key needed for the next part or the
  finish. Keys are held for one life; a switch that never resets keeps a
  finished course finished.
- **Portals go almost anywhere.** An ordinary surface takes a portal, as in
  Hotel; a few materials do not, and metal is the one players know. The
  challenge is to find how and where to portal. A map where one pale panel
  takes a portal and nothing else does makes the answer obvious; Obby gets
  away with it as a special case.
- **Wrong surfaces.** A portal can go where it does not help. A wrong choice
  fails for an understandable physical reason, not by visibly leading nowhere.
- **Portals prepared elsewhere.** One portal is set up from a different place
  than the takeoff. A surface's normal fixes its exit direction; the shooting
  position does not rotate it.
- **Portals on ramps.** Inclined surfaces give different trajectories toward
  one destination, which makes the angle a decision.
- **Changes of equipment.** Speed and low gravity as prerequisites, and
  erasers that take them away. Erasure keeps the body's velocity; losing low
  gravity changes gravity at once.
- **Necessary traversal.** An ordinary jump, a drop, another portal placement,
  or air steering must not bypass a puzzle. Low gravity floats down to any
  floor in reach, so a goal it must not reach is a closed room entered through
  an eraser doorway.
- **No way in at a walk.** A body that walks, steps, or hops into one portal
  leaves the other with almost no speed and drops from it, or drifts along
  it. So nothing a portal can open on lies above the goal or within a few
  metres beside it: the goal and the walls and ceiling around it are metal.
- **Recovery after mistakes.** Losing a required boost leaves a way to refill
  and retry, and every checkpoint has usable equipment and a route onward.
- **Ordinary skill.** The main route needs momentum from an ordinary takeoff
  or fall, portal orientation, and ordinary steering. Expert air control may
  reward optional routes only.
- **Visibility.** A landing or a portal pad sits far enough out to be seen and
  aimed at from a safe approach, never from the lip looking straight down.
- **A fine grid.** 1 m floor cells and wall sections, for geometry a 2 m grid
  cannot draw, and a storey a jump clears once and not twice: 1.6 m, where a
  jump rises 2.9 m.
- **Rooms that look like rooms.** A room is two such storeys tall, about 3 m,
  and a doorway is as tall as its room. Lights hang in one row a little above
  head height, never in rows above each other. A wall's ends and the edges
  of a floor look like the wall they sit in.
- **Enemies** as part of a room's problem, in small doses.
- **Solvable alone.** Every room works for one player; two-player twists come
  on top.

## Where we are

- **Hotel** is hand-built: the model for exploration, keys, quests, and looks.
- **Obby** is hand-edited: its portal rooms are the model for portal puzzles.
- **Portal Primer** is the one AI-authored map: one line of chambers, open-air
  except its finale, on a 2 m grid. It shows the tools work more than it is
  fun.
- **Gatehouse** is the first building, under construction: a hub, an exit
  behind three gates, and the first of its rooms. Its first playtest gave the
  principles above on portals, walking in, and looks.
- **The tools** build a map from a script, draw it as text, measure jumps and
  flings, and run a scripted route through the real game, headless or in a
  window. The builder writes buildings: rooms with ceilings and doorways,
  lights, enemy zones, any texture a map names, and portal surfaces sized to
  the grid. The AI can ask the game what a shot from a standing point opens,
  where its moves end for every pair of portals on its way, and whether
  walking into any pair reaches the goal. It can look at a map by launching
  the game at a chosen spot and capturing the window, by hand so far. It
  cannot yet search for a route or for a shortcut by other moves.
- **Tried in earlier courses, absent from the Primer:** shooting an enemy
  through a portal pair, a floor-to-floor launch upward with an air-steered
  catch, three ramp exits around one landing, erasure in mid-flight, a running
  jump onto a plate, and tests over ranges of timing and aim.

## Roadmap

1. **Self-checks.** The AI asks the game what a shot reaches from a standing
   point, where every portal pair on its way lands, and whether walking into
   a pair is enough. Showing that a route has no shortcut by other moves still
   needs a bot that plays at the level of intent and searches; looking at a
   map should become one command.
2. **A first small building to playtest.** A hub and three portal rooms in
   Obby's manner, each yielding a key or a switch, all needed for the exit,
   handed over once it passes the self-checks.
3. **Playtest lessons** written back here as principles and as room patterns
   that worked.
4. **The puzzle before the geometry:** what the player learns and in what
   order, then rooms, then the building. Then pacing across several maps, and
   two-player twists.
