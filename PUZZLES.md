# Puzzle map design

The kit supports spatial puzzles, portal puzzles, cooperative route planning, and sequences where an earlier achievement matters later. The most useful next decisions are how players retry a failed section and how switches behave across different player counts.

This document separates settled design rules from the current implementation and open decisions. The puzzle examples are design sketches, not playtested maps. Follow-up tracking and in-game testing remain in [TODO.md](TODO.md).

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

### Erasers

Erasers remove power-ups on player contact: speed, multishot, low gravity, and the portal gun. Keys, missile ammo, health, score, and quest progress survive. Losing the portal gun still removes the portal ends controlled by that player.

Erasers block portal shots. Movement, perception, ordinary projectiles, missiles, beams, and explosions otherwise treat them as empty space.

Blocking portal shots prevents the direct solution of shooting an exit portal through an eraser corridor. Portals established by another route or by a teammate already across can still provide a bypass; portal-resistant surfaces let the designer restrict those solutions.

Keys preserve earned access across stages, while missiles carry a useful resource forward. An eraser lets the next stage provide its own power-ups. For example, part 1 can award a key and a missile; after erasure, the key opens part 2 and the missile destroys its guard. The designer controls access to missiles through item placement and random-item availability.

Key and ammo loss on death is a separate decision from erasure.

## Implementation

The settled field and eraser rules are implemented. Attack clearance and beam clipping account for active fields while enemy awareness remains transparent. Ordinary projectiles are absorbed by both field types, and portal-shot blocking is universal rather than configurable. Erasers preserve keys and missile ammo, including when collecting items while standing in a field.

Actor navigation still excludes light bridges even while they are solid. Actors collide with them and can be supported by them, but planned routes do not cross them. Bridge-aware navigation remains an extension for enemy-luring puzzles.

The relevant behavior lives in [collision queries](common/src/physics/world/collision_world.rs), [projectile movement](common/src/physics/projectiles/motion.rs), [beam damage](server/src/combat/beam.rs), [beam rendering](client/src/vfx/laser.rs), [player inventory](server/src/players/resources.rs), and [portal cleanup](server/src/portals/equipment.rs).

## Available elements

| Element | Puzzle uses and constraints |
|---|---|
| Floors, walls, gaps, ramps, ladders, and multiple levels | Mazes, alternate routes, controlled drops, observation positions, and isolated enemy perches. |
| Portals | Connect separated routes, establish temporary access, redirect falling momentum, and carry ordinary projectiles. Portals can ride moving geometry. Players and ordinary projectiles traverse them; actors do not. |
| Portal-resistant materials | Restrict placement by surface and face, making the shooting position and sequence part of the solution. |
| Barriers and keys | Personal passage through a closed field versus globally opening it with plates. Keys are reusable access permissions. |
| Pressure plates | Open barrier groups, power bridge groups, or trigger fireworks. Solo holding plates toggle; multiplayer holding plates depend on occupancy. Different purposes can occupy one tile, so one position can already control several outputs. |
| Light bridges | Switchable crossings and drops; protection above or below their surface. A powered bridge can also obstruct a portal shot. |
| Erasers | Boundaries between sets of power-ups, while keys and ammo connect stages. |
| Moving platforms and nested rooms | Lifts, shuttles, moving cover, moving ladders, moving switches, and carried actors. Motion repeats automatically between two positions with pauses and a phase offset; there is no switch control or rotation yet. |
| Speed and low-gravity power-ups | Reachability puzzles, jumps, timed routes, and combinations with portal momentum. Duration can be timed or last until death or erasure. |
| Ordinary projectiles and multishot | Ricochets off ordinary geometry, attacks through portals, and enemy removal. There are no general shootable puzzle switches. |
| Missiles | A carried resource for removing guards or choosing between dangerous routes. Ordinary projectiles can be disabled independently of missile availability. |
| Mines, sentries, zappers, and reapers | Luring, containment, guarded space, exposure windows, and resource use. The actor named sentry is a contact attacker; zappers provide the ranged guard behavior. |
| Cookies, quests, and fireworks | Collection objectives, individual or group progress, and a finale. Quest conditions currently recognize cookies, actor kills, and fireworks. |
| Health pickups and regeneration | Recovery and control over how much danger a player can endure. They also affect whether a hazard can be bypassed by accepting damage. |
| Materials, lights, grass, weather, and lighting | Landmarks, clues, atmosphere, and visual distinction. They are mostly presentation tools rather than controllable puzzle mechanisms. |
| Spawn zones and item placement | Control where players begin, where enemies appear, and which resources are available. Player spawn zones are not progression checkpoints. |

The map authoring tools are described in the [editor reference](tools/map_editor/tool_reference.md). Tuning, item availability, and quest definitions live in [gameplay.json](config/server/gameplay.json).

## Puzzle patterns

### Progress across stages

Solve a portal or movement puzzle to earn a key and a missile. Cross an eraser, then use the key to enter a guarded section and the missile to remove a zapper. Several stages can award different keys, with successive barriers at the finale requiring all of them.

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

One player holds a bridge plate while another crosses to reach a control or establish portal access. Then they arrange an exit for the helper. Add a zapper to make protected positions matter. Each supported player count needs a complete solution; the current automatic switch to solo toggles changes the puzzle substantially.

### Enemy containment

Lure a contact attacker into a side passage and restore a barrier behind it. The barrier also shields against its blast. Containment can make another route safe, but explosions do not currently operate switches or destroy walls. Luring actors over powered bridges requires navigation support.

### Geometry as logic

Successive gates provide an AND condition; alternative routes provide OR. Keys, placed portals, solo switches, and quest progress provide different forms of memory. Shared plate positions can combine outputs. These spatial constructions already support useful logic, although inversion and explicit switch modes would make authoring more direct.

## Zappers as guards

An isolated ledge is enough to explore the guard role with existing actors. Zappers can acquire and shoot players on another level without a walking route to them; this is covered by the [behavior tests](server/src/actors/behavior/tests.rs). The perch needs both a clear firing angle and no navigable route to the player. The ledge's own floor can obstruct a downward shot.

A perched zapper can still roam within its available area. Its cooldown behavior can also move it. A stationary option would make precise firing positions reliable and would allow stationary and roaming zappers in one map. Current movement validation rejects zero speed, so setting its speed to zero is not an existing authoring option.

The current configuration gives a zapper a 25 m beam range, a two-second burst, an eight-second cooldown, and a three-minute respawn delay. It detects visible players in all directions, rather than scanning a directional cone. A burst commits to one target; baiting it and withdrawing behind cover can give another player an opening.

Its current damage and durability make it a soft obstacle: a complete burst inflicts 80 damage against 500 player health, while one ordinary projectile deals 60 damage against the zapper's 50 health. A map must account for enduring the beam or simply shooting the guard. Disabling ordinary projectiles can reserve destruction for missiles, but does not prevent running through the damage.

The encounter's role should determine the tuning: pressure during traversal, a dangerous boundary, or a guard intended to be destroyed. Placement-level movement, durability, attack timing, and respawn choices would help those roles coexist. Visible aiming, firing, and cooldown feedback would make experimentation easier to understand.

Current barrier plates open their barrier. A release can therefore restore a shield in multiplayer, and another press can restore it in solo play. A direct "press to raise the shield" control needs inversion. The zapper remains alive and dangerous when the shield opens; shielding does not switch off the actor itself.

## Next decisions

### 1. Progress, death, and retries

Decide what constitutes one attempt and which state a retry restores. Keeping keys through erasers supports progression, but death currently removes keys and ammunition. A fall late in the map can therefore require repeating earlier stages.

For puzzle play, retaining earned access and restarting the current section is a useful direction. Ammo and world state need a coordinated rule: restoring a missile while keeping its victim dead gives a different result from restoring the whole encounter. Checkpoints and room resets should specify equipment, portals, actors, switches, and progress together.

A required missile that can be wasted needs a recovery route, another solution, or a retry. Checkpoint scope in co-op also matters: one player's failure should have a defined effect on a teammate still solving the room.

### 2. Player counts and controls

Decide whether every map must work solo and with arbitrary teams, or whether some maps can require a particular player count. Keys are personal today; teammates need their own keys or someone to open a route for them.

The current [plate rules](server/src/map/pressure_plates.rs) use toggles in solo play and occupancy thresholds in multiplayer. Those thresholds depend on living players. With two players connected, one dying can automatically open controlled barriers and power controlled bridges. When barriers provide cover, this can expose the survivor to an attack.

Authored holding plates and toggle switches, explicit thresholds, and inverted outputs are the first useful control extensions. Timed switches and controls for carriers or zappers can follow concrete puzzle needs. Joins, disconnects, and deaths need predictable behavior rather than accidentally satisfying a mechanism.

### 3. Guard behavior

Start with a perched zapper encounter and observe whether confinement is enough. Add a stationary placement option when reliable positioning requires it. Choose whether the encounter rewards avoidance, shielding, destruction, or several solutions; then tune damage, timing, and durability accordingly.

Directional sight and a firing warning would add options for stealth and reaction puzzles, but they are separate capabilities from remaining stationary.

### 4. Resource and enemy respawning

A reusable ammunition supply lets players wait and stockpile up to the ammo cap. A single reward makes saving ammunition meaningful but requires recovery from mistakes. A returning guard can undo the benefit of spending that reward.

Current placed-item respawn times are per item type within a map; actor respawn times are global per kind. Per-placement policies would allow reusable supplies, once-collected rewards, and guards restored only with an encounter reset. The intended policy must also allow teammates to obtain required personal items.

### 5. Readable cause and effect

Plates currently look alike in-game. Optional matching symbols, connection indicators, state lights, and map-authored signs would help players identify what a control affects and whether it worked. A shield control could visibly match the field protecting a crossing.

Hidden connections can be deliberate puzzles, but the basic mechanic should be taught where its effect is observable. Distinguishing holding, toggle, and timed controls visually becomes important if those modes are added.

### 6. Completion conditions

Keys already provide a simple dependency between stages. More explicit completion conditions would support reaching a destination, activating named mechanisms, or collecting particular objects, with completion unlocking another section or recording a checkpoint.

Cookie quests count pickups, not unique token identities. Relay and Switchyard approximate once-collected tokens with a 24-hour respawn delay. Unique collectible identities would make that requirement explicit. Actor-kill quests can filter by kind, but do not identify a particular placed guard.

### 7. Additional puzzle objects

Carryable or pushable weights that hold plates and traverse portals would add placement and transport puzzles, including solo solutions to holding mechanisms. They need recovery when lost.

Beam receivers, reflectors, and beam traversal through portals would add optical puzzles; those interactions do not exist today. Powered-bridge navigation is another extension needed for puzzles that depend on an enemy following that route.

These additions can follow tests of the existing combinations. Progression, retries, and controls affect more of the current kit.

## Existing maps and a first prototype

Relay and Switchyard already explore portal setup, keys, bridges, moving geometry, and quest finales. Both still need user playtesting. Switchyard's customs section puts the portal gun and key across an eraser, with its first seal on a raised balcony back in the starting room. A portal route around the eraser preserves the gun needed to reach that seal. The key survives either route, and the departure eraser ends the power-up section before the moving-bridge puzzle.

A compact prototype can test the settled rules with three connected sections:

1. A portal or movement puzzle awards a key and a missile.
2. An eraser ends the power-up section while preserving both rewards.
3. The key grants access to a guarded crossing; a perched zapper, a protective field, and the saved missile provide the encounter's choices.

Playtesting should establish whether the guard can simply be rushed, whether its firing path and the protective field are readable, whether the helper can escape in co-op, and whether a missed missile or death leaves a complete recovery path. A moving wall can then test the same encounter with changing cover. In-game testing remains with the user.
