# Puzzle map design

The kit supports spatial puzzles, portal puzzles, cooperative route planning, and sequences where an earlier achievement matters later. The most useful next decisions are how players retain progress and retry a failed section.

This document separates settled design rules from the current implementation and open decisions. The [small example maps](#small-example-maps) implement the nine patterns below and await in-game playtesting. Follow-up tracking remains in [TODO.md](TODO.md).

## Settled rules

### Barriers and light bridges

A closed barrier or powered light bridge is transparent solid cover. Players and enemies can see through it, but physical interactions and attacks cannot cross it. Ordinary projectiles are absorbed to communicate that it is an energy field.

| Interaction | Closed barrier or powered bridge |
|---|---|
| Visibility and enemy awareness | Pass through |
| Movement | Blocked; a bridge supports bodies |
| Zapper beams | Blocked, including the visible beam |
| Explosion damage and knockback | Blocked across the surface |
| Ordinary projectiles | Absorbed, with an impact effect |
| Missiles | Collide and detonate; the field shields the other side |
| Portal shots | Blocked; energy fields cannot host portals |

Open barriers and unpowered bridge ghosts have no blocking effect. A matching key remains permission for its holder to cross a closed barrier; it does not let weapons or explosions pass through that barrier.

Visibility and attack clearance are separate. A zapper can notice a player behind a field, but needs a clear firing path to start attacking. A field activated during a burst immediately blocks damage and clips the beam. Contact enemies likewise need an unobstructed attack path before triggering a proximity attack.

Prefer these as universal rules rather than per-map interaction exceptions. Geometry, placement, activation timing, and available equipment supply the map's variation.

### Pressure controls and death resets

Each entry in a map’s `barrier_kinds` or `bridge_kinds` catalog in its `settings.json` requires a `pressure_switch` block. Plates name only their type and kind; all matching plates control that kind together. There are no map defaults or per-plate overrides.

```json
"pressure_switch": {
  "activation": "toggle",
  "reset_on_player_death": "all"
}
```

`activation` is `momentary` (any matching plate is held), `toggle` (each fresh plate press flips the state), or `auto` (toggle with exactly one logged-in player, momentary otherwise). Dead players still count as logged in. Auto seeds its toggle from occupied plates when the player count becomes one and discards that latch when returning to momentary. Explicit toggles keep their state through player-count changes unless a configured logout reset applies. Hotel uses `auto` so a lone player can solve the lobby and teammates must cooperate.

Switches and `respawn.actors.on_player_death` share four death/logout triggers: `never`, `solo` (the sole logged-in player dies or leaves), `any` (any player dies or leaves), and `all` (nobody alive remains). All includes a solo death and a group respawn. Eligibility uses membership immediately before the event and living players remaining afterward. Actor `scope` remains `dead` or `all`, and `respawn.players` remains `individual` or `group`; logout does not start a group player countdown.

Switch resets clear saved toggle state before players respawn; an already-held plate needs a release and fresh press. Momentary controls follow occupancy, and fireworks retain their separate threshold. Actor resets wait through the respawn delay and use the normal beam-in warning; their countdowns survive logout, even on an empty server. Shots remain in flight. The cyan barrier in `puzzle_access` uses `toggle`/`all`, the containment kinds use `auto`/`solo`, and other shipped kinds use `auto`/`never`.

### Erasers

Erasers remove all weapons and power-ups on player contact: single-shot, multishot, missile ammo, the portal gun, speed, and low gravity. Keys, health, score, and quest progress survive. Losing the portal gun still removes the portal ends controlled by that player.

Erasers block portal shots. Movement, perception, ordinary projectiles, missiles, beams, and explosions otherwise treat them as empty space.

Blocking portal shots prevents the direct solution of shooting an exit portal through an eraser corridor. Portals established by another route or by a teammate already across can still provide a bypass; portal-resistant surfaces let the designer restrict those solutions.

Keys can record "step X was solved": award a key for completing that step, then require it at a matching barrier beyond an eraser. The key preserves that progress while the eraser clears equipment. Fresh pickups behind the barrier can replace erased items or provide a different set for the next step. This uses the existing keys, barriers, and pickups.

Death clears keys as well as equipment.

## Implementation

The settled field and eraser rules are implemented. Attack clearance and beam clipping account for active fields while enemy awareness remains transparent. Ordinary projectiles are absorbed by both field types, and portal-shot blocking is universal rather than configurable. Erasers preserve keys and clear power-ups and missile ammo, including when collecting items while standing in a field.

Actor navigation still excludes light bridges even while they are solid. Actors collide with them and can be supported by them, but planned routes do not cross them. Bridge-aware navigation remains an extension for enemy-luring puzzles.

The relevant behavior lives in [collision queries](common/src/physics/world/collision_world.rs), [projectile movement](common/src/physics/projectiles/motion.rs), [beam damage](server/src/combat/beam.rs), [beam rendering](client/src/vfx/laser.rs), [player inventory](server/src/players/resources.rs), and [portal cleanup](server/src/portals/equipment.rs).

## Available elements

| Element | Puzzle uses and constraints |
|---|---|
| Floors, walls, gaps, ramps, ladders, and multiple levels | Mazes, alternate routes, controlled drops, observation positions, and isolated enemy perches. |
| Portals | Availability comes from portal-gun pickups. Connect separated routes, establish temporary access, redirect falling momentum, and carry ordinary projectiles. Portals can ride moving geometry. Players and ordinary projectiles traverse them; actors do not. |
| Portal-resistant materials | Restrict placement by surface and face, making the shooting position and sequence part of the solution. |
| Barriers and keys | Personal passage through a closed field versus globally opening it with plates. Keys are reusable access permissions. |
| Pressure plates | Open barrier groups, power bridge groups, or trigger fireworks. Each barrier or bridge kind selects momentary, toggle, or automatic solo/multiplayer activation and a death-reset rule. Each tile permits one plate per level, with one purpose. |
| Light bridges | Switchable crossings and drops; protection above or below their surface. A powered bridge can also obstruct a portal shot. |
| Erasers | Boundaries between equipment sets, while retained keys unlock routes and restock areas across stages. |
| Moving platforms and nested rooms | Lifts, shuttles, moving cover, moving ladders, moving switches, and carried actors. Motion repeats automatically between two positions with pauses and a phase offset; there is no switch control or rotation yet. |
| Speed and low-gravity power-ups | Reachability puzzles, jumps, timed routes, and combinations with portal momentum. Duration can be timed or last until death or erasure. |
| Single-shot and multishot pickups | Independent weapons: players start without projectile fire, and each pickup grants its own selectable mode. Availability comes entirely from pickups. Both expire or erase like other power-ups. Ricochets, attacks through portals, and enemy removal support combat puzzles; there are no general shootable puzzle switches. |
| Missiles | A carried resource for removing guards or choosing between dangerous routes. Omitting projectile pickups can reserve guard removal for missiles; an eraser requires a fresh missile pickup beyond it. |
| Mines, sentries, zappers, reapers, and turrets | Luring, containment, guarded space, exposure windows, and resource use. The actor named sentry is a contact attacker; zappers provide mobile ranged guards and turrets provide stationary continuous-fire guards. |
| Gold, quests, and fireworks | Collection objectives, individual or group progress, and a finale. Quest conditions currently recognize gold, actor kills, and fireworks. |
| Health pickups and regeneration | Recovery and control over how much danger a player can endure. They also affect whether a hazard can be bypassed by accepting damage. |
| Materials, lights, grass, weather, and lighting | Landmarks, clues, atmosphere, and visual distinction. They are mostly presentation tools rather than controllable puzzle mechanisms. |
| Spawn zones and item placement | Control where players begin, where enemies appear, and which resources are available. Player spawn zones are not progression checkpoints. |

The map authoring tools are described in the [editor reference](tools/map_editor/tool_reference.md). Global tuning lives in [gameplay.json](config/server/gameplay.json); each map’s `settings.json` holds its tuning, item availability, and quests.

## Puzzle patterns

### Progress across stages

Solve a portal or movement puzzle to earn a key. Cross an eraser, then use the key to pass a barrier into a restock area. Collect a missile to remove a zapper, or other power-ups needed for the next section. Several stages can award different keys, with successive barriers at the finale requiring all of them.

### Access versus protection

A barrier separates players from a zapper. Opening it creates both a walking route and a firing path. A key holder can cross while keeping the protective field active, reach another position, and help the others. Barrier shielding makes the difference between personal access and global opening valuable.

### A bridge that also shields

A bridge above a lower passage blocks an elevated zapper's downward shot when powered. Someone crosses beneath it, and the same bridge later provides an upper walking route. The surface needs to lie between the attacker and player; being near a bridge does not provide protection around its edges.

### Moving cover and moving guards

Travel alongside a sliding wall, ride inside a protected cabin, or wait for a moving room to interrupt a firing line. Alternatively, put a zapper on a carried platform so the dangerous area moves. Moving walls and active fields can both shield the crossing.

### Portal setup and sequencing

Open a barrier long enough to place a portal through the opening, then restore the barrier while keeping the portal. A bridge can provide a walking route when powered and expose a portal-shot path when unpowered. The puzzle is choosing the order of setup and traversal. Entering an eraser removes the traveler's controlled portals, so that boundary must be accounted for.

### Momentum and access

Drop into a floor portal and emerge from a wall or sloped portal toward a distant landing. Restrict portalable surfaces so the player must find the useful combination. Speed, low gravity, moving destinations, and ladders can add a second step without needing another object type.

### Cooperative positioning

One player holds a bridge plate while another crosses to reach a control or establish portal access. Then they arrange an exit for the helper. Add a zapper to make protected positions matter. Each supported player count needs a complete solution; the automatic switch to solo toggles changes the puzzle substantially when a kind uses `auto`.

### Enemy containment

Lure a contact attacker into a side passage and restore a barrier behind it. The barrier also shields against its blast. Containment can make another route safe, but explosions do not currently operate switches or destroy walls. Luring actors over powered bridges requires navigation support.

### Geometry as logic

Successive gates provide an AND condition; alternative routes provide OR. Keys, placed portals, solo switches, and quest progress provide different forms of memory. These spatial constructions require layouts that make the conditions necessary; inversion and multiple outputs per switch would make authoring more direct.

## Zappers as guards

An isolated ledge is enough to explore the guard role with existing actors. Zappers can acquire and shoot players on another level without a walking route to them; this is covered by the [behavior tests](server/src/actors/behavior/tests.rs). The perch needs both a clear firing angle and no navigable route to the player. The ledge's own floor can obstruct a downward shot.

A perched zapper can still roam within its available area. Its cooldown behavior can also move it. Use a turret when the encounter needs a fixed guard; zero speed is not a valid movement setting.

The current configuration gives a zapper a 25 m beam range, a two-second burst, an eight-second cooldown, and a three-minute respawn delay. It detects visible players in all directions, rather than scanning a directional cone. A burst commits to one target; baiting it and withdrawing behind cover can give another player an opening.

Its current damage and durability make it a soft obstacle: a complete burst inflicts 80 damage against 500 player health, while one ordinary projectile deals 60 damage against the zapper's 50 health. A map must account for enduring the beam or simply shooting the guard. Withholding single-shot and multishot pickups can reserve destruction for missiles, but does not prevent running through the damage.

The encounter's role should determine the tuning: pressure during traversal, a dangerous boundary, or a guard intended to be destroyed. Placement-level movement, durability, attack timing, and respawn choices would help those roles coexist. Visible aiming, firing, and cooldown feedback would make experimentation easier to understand.

Current barrier plates open their barrier. A release can therefore restore a shield in momentary mode, and another press can restore it in toggle mode. A direct "press to raise the shield" control needs inversion. The zapper remains alive and dangerous when the shield opens; shielding does not switch off the actor itself.

## Turrets as guards

Turrets spawn at usable floor-cell centers and stay there in their carrier's frame. The actor kind sets `immovable: true` and has no per-map speed settings; its `beam` attack uses a long duration and a short cooldown. Each burst follows one exposed player and immediately selects another when that target hides, dies, disconnects, or leaves range. Walls, closed barriers, and powered bridges provide the same protection as against zappers.

At 500 damage per second, a turret kills a full-health player in about one second. This leaves a short window to launch a missile and retreat; guarded crossings must require longer exposure to discourage rushing. A protected health and missile supply supports another attempt after a miss. Missiles are the first puzzle demonstration weapon; single-shot and multishot can add aiming skill in later variants.

## Next decisions

### 1. Progress, death, and retries

Decide what constitutes one attempt and which state a retry restores. Keeping keys through erasers supports progression, but death currently removes keys and ammunition. A fall late in the map can therefore require repeating earlier stages.

For puzzle play, retaining earned access and restarting the current section is a useful direction. Ammo and world state need a coordinated rule: restoring a missile while keeping its victim dead gives a different result from restoring the whole encounter. Checkpoints and room resets should specify equipment, portals, actors, switches, and progress together.

A required missile that can be wasted needs a recovery route, another solution, or a retry. Checkpoint scope in co-op also matters: one player's failure should have a defined effect on a teammate still solving the room.

### 2. Player counts and controls

Decide whether every map must work solo and with arbitrary teams, or whether some maps can require a particular player count. Keys are personal today; teammates need their own keys or someone to open a route for them.

The [plate rules](server/src/map/pressure_plates.rs) support authored momentary, toggle, and automatic controls per barrier or bridge kind. Momentary always requires an occupied plate, so a death cannot satisfy an empty control.

Inverted outputs, timed switches, and controls for carriers or zappers can follow concrete puzzle needs. Explicit multi-plate thresholds may help puzzles that require several distinct positions at once.

### 3. Guard behavior

Playtest the turret examples for avoidance, shielding, and destruction. Check whether the one-second exposure window allows shooting and hiding while preventing a direct rush, then tune the encounters accordingly.

Directional sight and a firing warning would add options for stealth and reaction puzzles, but they are separate capabilities from remaining stationary.

### 4. Resource and enemy respawning

A reusable ammunition supply lets players wait and stockpile up to the ammo cap. A single reward makes saving ammunition meaningful but requires recovery from mistakes. A returning guard can undo the benefit of spending that reward.

Current placed-item respawn times are per item type within a map; actor respawn times are global per kind. Per-placement policies would allow reusable supplies, once-collected rewards, and guards restored only with an encounter reset. The intended policy must also allow teammates to obtain required personal items.

### 5. Readable cause and effect

Plates currently look alike in-game. Optional matching symbols, connection indicators, state lights, and map-authored signs would help players identify what a control affects and whether it worked. A shield control could visibly match the field protecting a crossing.

Hidden connections can be deliberate puzzles, but the basic mechanic should be taught where its effect is observable. Distinguishing momentary and toggle controls visually would help; timed controls would also need their own feedback.

### 6. Completion conditions

Keys already provide a simple dependency between stages. More explicit completion conditions would support reaching a destination, activating named mechanisms, or collecting particular objects, with completion unlocking another section or recording a checkpoint.

Gold quests count pickups, not unique token identities. Relay and Switchyard approximate once-collected tokens with a 24-hour respawn delay. Unique collectible identities would make that requirement explicit. Actor-kill quests can filter by kind, but do not identify a particular placed guard.

### 7. Additional puzzle objects

Carryable or pushable weights that hold plates and traverse portals would add placement and transport puzzles, including solo solutions to holding mechanisms. They need recovery when lost.

Beam receivers, reflectors, and beam traversal through portals would add optical puzzles; those interactions do not exist today. Powered-bridge navigation is another extension needed for puzzles that depend on an enemy following that route.

These additions can follow tests of the existing combinations. Progression, retries, and controls affect more of the current kit.

## Small example maps

Each map has a gold collection objective and a firework finish. Eight are designed for one player and have one token; `puzzle_coop` is designed for two and has two coins, with one pickup required per player. Start a server with `cargo run --release --bin server -- --map puzzle_stages`, substituting any name below. Open the same name with `python3 tools/editor.py puzzle_stages` to inspect or edit it.

| Map | Players | Concept | Goal |
|---|---:|---|---|
| [puzzle_stages](config/server/maps/puzzle_stages/layout.json) | 1 | Progress across stages | Carry the balcony key through customs, restock, and clear the turret hall. |
| [puzzle_access](config/server/maps/puzzle_access/layout.json) | 1 | Access versus protection | Cross a protective barrier without exposing the starting hall. |
| [puzzle_shield](config/server/maps/puzzle_shield/layout.json) | 1 | Bridge as cover and walkway | Reach the far ladder under the powered roof, then return over it. |
| [puzzle_cover](config/server/maps/puzzle_cover/layout.json) | 1 | Moving cover | Board a sheltered shuttle and ride past a turret. |
| [puzzle_sequence](config/server/maps/puzzle_sequence/layout.json) | 1 | Portal setup and sequencing | Set a remote portal through a shutter, restore cover, then travel. |
| [puzzle_momentum](config/server/maps/puzzle_momentum/layout.json) | 1 | Momentum and access | Turn a fall into a launch toward a distant landing. |
| [puzzle_coop](config/server/maps/puzzle_coop/layout.json) | 2 | Cooperative positioning | Hold a crossing for a partner, then arrange the helper's escape. |
| [puzzle_containment](config/server/maps/puzzle_containment/layout.json) | 1 | Enemy containment | Lure a hunter into a pen and leave it behind a closed field. |
| [puzzle_logic](config/server/maps/puzzle_logic/layout.json) | 1 | Geometry as logic | Explore a bridge and lower route to reach the gate switches; the intended two-gate condition is bypassable. |

Steel-panel `skybridge` surfaces accept portals; the other materials in these examples resist them. Supplies replenish after five seconds, equipment lasts until death or erasure, and no random pickups appear. The guard-removal example uses missiles and provides a sheltered health pickup beside the ammunition. Gold takes 24 hours to respawn, except in `puzzle_coop`, where it returns after five seconds so one player collecting both coins cannot block the other. Restart the server for a completely fresh attempt. `puzzle_access` closes its cyan barrier when nobody alive remains after a death or logout. `puzzle_stages` and `puzzle_containment` restore all actors at their starting positions after a solo death or logout. Containment also resets its barrier switches; other example switches and quest progress persist through death.

The player counts describe intended play, not enforced admission limits. With one logged-in player, the examples’ barrier and bridge plates toggle on each fresh press. In the two-player example, one player holds the bridge while the other crosses; its two finish plates ask both players to arrive. Firework thresholds follow the living player count; automatic barrier and bridge modes follow the logged-in count. Enemy containment is an intended solution rather than a recognized quest condition: the game cannot distinguish trapping a hunter from surviving or destroying it.

### Intended solutions

1. **Stages:** Climb the starting room's ladder and collect the amber key and speed pickup. Return through the eraser and amber gate; the key survives, the speed does not. Take a missile from the corner shelter, peek into the long hall, launch at the turret, and retreat. Restock and heal after a miss, then collect the far token and finish.
2. **Access:** Investigate the key alcove around the short partition. The cyan switch opens a direct firing line to its position. Keep the field closed and use the key to cross its southern end, where the permanent wall shields the exit room.
3. **Shield:** Activate the green roof from the starting corner, cross underneath it, and climb the far ladder. Return west along the same bridge to the raised token. The upper wall shields this return route from the turret.
4. **Cover:** Wait for the shuttle at the starting dock, board through the open rear half, and remain behind its front wall during the trip. Step onto the far dock for the token. A fall lands on the recovery floor; only the starting dock has a return ladder.
5. **Sequence:** Collect the portal gun and place a departure portal on the steel floor panel in front of the shutter. Open the shutter from the sheltered switch. Peek around the partition and place the other portal on the steel floor panel beyond the guard's side wall. Retreat, close the shutter, and enter the departure portal to emerge in the finish shelter.
6. **Momentum:** From the recovery floor, place one portal on the steel floor panel beneath the drop deck and the other high on the east-facing steel wall panel, aimed toward the landing. Climb the tall ladder, step off toward the floor portal, and let the fall launch you across. The lower floor, health pickup, and ladder support another attempt.
7. **Co-op:** One player holds the bridge plate. The other crosses, takes the portal gun, and links the far steel floor panel to the steel wall panel on the starting platform. The helper leaves the plate and enters the wall portal. Each player collects a gold coin to reveal the two finish plates, then they occupy both plates together.
8. **Containment:** Open the green start barriers to release both the player and sentry. Open the cyan pen from the corridor switch and lead the sentry inside. Collect the amber exit key, climb the pen's escape ladder, drop outside, and close the pen using the second cyan switch. Return through the sentry's area and use the key at the exit gate. Contact with the hunter is lethal at full health; the closed field shields against its blast. The player can currently outrun the sentry and finish without trapping it.
9. **Logic:** Separate starting plates control amber and the bridge. Reach the blue switch across the bridge or via the lower route and its ladders. The lower route can bypass amber, leaving only blue to open before collecting gold. The layout does not enforce its intended two-gate condition and needs redesign.

Shared tests cover collision, portal traversal, and player movement; every registered map is checked for valid configuration and layout. Readability, peek timing, boarding, luring, and unintended shortcuts need playtesting.

## Existing larger maps

Relay and Switchyard already explore portal setup, keys, bridges, moving geometry, and quest finales. Both still need user playtesting. Switchyard's customs section puts the portal gun and key across an eraser, with its first seal on a raised balcony back in the starting room. A portal route around the eraser preserves the gun needed to reach that seal. The key survives either route, and the departure eraser ends the power-up section before the moving-bridge puzzle.
