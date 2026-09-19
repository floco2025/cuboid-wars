# Tool Reference

Choose a tool in the left palette. **Place / Erase** switches between placing the selected element and erasing its group inside a dragged rectangle on the current level. The general **Erase** tool is at the top of the palette, with an optional **Keep floors** setting.

## Navigation and Tool Settings

- **Resize Map** — Map → Resize Map… changes rows and columns in the active map. **Shrink to fit** trims empty borders around the contents across all levels, including both ends of moving nested maps. It fills in the fitted size and disables the manual size and anchor controls; turn it off to restore those controls. At least one row and column remain. OK applies one undoable change; Cancel discards it. Resizing preserves the number of levels.
- **Levels** — Map → Edit Levels… opens a table of all levels in the active map. Edit names directly; Add inserts above the selected level, and Remove deletes the selected level. **Shrink to fit** removes empty levels below and above the contents while retaining empty levels between them, spanning objects, and nested-map motion. At least one level must remain. OK applies the whole batch as one undoable change; Cancel discards it. The dialog lists any objects that would be removed and asks before applying those changes. View → Next Level and Previous Level switch the level being shown.

- **Tool Palette** — Icons and short labels stay in fixed positions under Build, Zones & Items, Mechanisms, Appearance, and Measure. Select, Sample, and Erase stay at the top. View → **Tool Icons Only** collapses the palette to a narrow icon strip; tooltips identify each tool. Selecting an element starts in Place mode; click Erase or press `E` with canvas focus to switch its operation. Floors and Blocked Floor share Erase Floors, and both ramp directions share Erase Ramps. The highlighted tool and operation buttons show what is active.
- **Map** — Open a map registered by name in `gameplay.json`. Its folder contains `layout.json` for placement, controls, and kind catalogs, and `settings.json` for tuning. The Map picker switches between the outer map and its named nested geometry; all views use the parent's kinds and texture catalog. Save, autosave, and undo cover the whole document. Undo switches to the affected map.

- **Zoom / Pan / Fit Map** — `Cmd+Plus` / `Cmd+Minus` on macOS, or `Ctrl+Plus` / `Ctrl+Minus` elsewhere, zoom in and out. Scroll with a wheel, Magic Mouse, or touchpad to pan; Shift-wheel pans horizontally. Scroll bars appear when the map extends outside the view, and panning stops at the map edges. Space-drag and middle-drag also pan. View → Fit Map (`F`) shows the whole map.
- **Window** — Size, position, and maximized state are remembered across launches, shared by every map. Off-screen positions are brought back onto an available screen. New, Open, and Resize Map fit the canvas without changing the window size.
- **Tool Settings** — Controls appear in the top toolbar for tools that need placement defaults. Placement reuses the previous values; a dialog is needed only when no usable choice has been made yet. Use **Controls…** for pressure plate assignments or **Settings…** for nested-map motion. The always-visible **Properties** panel edits existing objects as soon as they are selected, keeping placement defaults unchanged.
- **Single-tile tools** — Ladders, lights, plates, and items preview one tile or edge, never a range. Holding the mouse button lets you adjust the target; releasing places once. Escape or releasing off-grid cancels.
- **Feedback** — Placement warnings and copy confirmations appear briefly over the canvas without taking focus or blocking clicks. Undo and Redo menus name the available actions.
- **Hover details** — Hover any element to see its type and relevant properties, including materials, kind, actor count, or nested-map motion. Overlapping elements follow the same priority as right-click; material tools show their target's materials.
- **Check Map / Review Repairs** — Map → **Check Map…** opens a separate issues window when needed. Click a result to select its map and level and highlight the object. Loading preserves invalid records and warns about errors; **Review Repairs** lists automatic changes for approval. Accepted repairs undo in one step. While repairs are pending, edits preserve records for manual correction; saving remains blocked by validation errors.
- **Recovery / Dependencies** — Unsaved maps receive recovery copies every 15 seconds, including untitled maps. Use File → Recover Unsaved Map to restore an untitled session after a crash; active sessions cannot be recovered by a second editor. Named maps offer newer autosaves when opened. Changes to the parent's gameplay catalogs refresh the editor automatically.

## Sample

- **Sample** — Choose the eyedropper and click an element, press `I` while pointing at it with canvas focus, or right-click → **Use This Tool**. This selects its placement tool and reuses its applicable face materials, kind, count, span, and pressure plate controls. Choosing a new material clears the sampled face recipe.

## Properties and connections

- **Properties** — Selecting an object or group immediately shows its properties. Right-click selects an object from any tool; Enter with canvas focus moves keyboard focus into Properties. For mixed element types, choose a type or edit their shared properties. **Mixed / unchanged** preserves individual values until that field changes. Material helpers, actor counts and search, spawn-zone first level and span, and full nested-map motion controls live here. There is no Apply step: a dropdown commits when a value is picked, and a text field commits on Enter or when focus leaves it, including a click on the canvas; **Esc** discards what is still being typed. Every edit made while the same objects stay selected undoes as one step. An invalid value stays in its field with the reason shown and commits together with the next finished field, so values that are only valid as a pair land as one edit; changing the selection discards it with a canvas notice. The mouse wheel scrolls the panel and never changes a dropdown. Autosave and background refreshes preserve text being typed and the cursor, and Save commits it first. If catalogs change while typing, updated choices appear once the field is finished.
- **Connections** — View → **Show Connections** toggles canvas highlights and lines between the selected pressure plate and its controlled objects, or between a selected controlled object and matching plates. It starts off each time the editor opens. Highlights update immediately when toggled or when selection changes and show connections in the current geometry and level.
- **Panel space** — Tools and Properties stay visible. Drag their dividers to resize them; widths are remembered, including separate labeled and icon-only palette widths. Closed dropdowns stay compact; opened lists and tooltips show full names, including material portal permissions.

## Select

- **Objects / Tiles** — Select starts with **Scope: Objects**. The tool activates selection; Scope chooses individual objects or whole tile areas. Click to select the specific object under the pointer. Drag from an unselected object to select that type within the rectangle: start on a wall for walls only, or on an equipment eraser for erasers only. The same rule applies to other object types except nested maps, which drag directly to move, so their underlying floors stay unselected. A rectangle started in empty space includes all types. Shift-click adds or removes an object; Shift-drag adds a group using the same starting-type rule. The canvas shows the type during the drag. The canvas outlines the selection, and Properties shows its object count. **Levels** sets the upward span for group selection. Intersected spawn zones, ramps, ladders, and nested-map motion are selected as complete objects. Switch the toolbar selector to **Tiles** for whole-area operations, including empty cells.
- **Wall / eraser lines** — Drag along a grid edge to select a single line of walls, equipment erasers, or barriers. You can start before the first segment; the first edge encountered chooses the type. The preview follows the line and highlights the selected segments immediately. Floors, adjacent parallel lines, and crossing edges stay unselected.
- **Delete** — `Delete` or `Backspace` removes only the selected objects in Objects mode; undo restores them. Removing support needed by an unselected object is refused. Tool-specific Place / Erase and `E` remain available for quick erasing by type.
- **Copy / Cut / Paste** — Object copies contain only the selected objects. Select a destination and paste to add them while preserving surrounding content; conflicts are refused. Cut removes those objects only after validation succeeds. Copies made in **Tiles** mode replace the destination area, including empty cells, over the chosen level span. Tile operations require whole objects, both ends of nested motion, and any dependent wall lights to be included. Click the destination before pasting. The copied content determines whether paste adds objects or replaces tiles, and the pasted content becomes the selection. Missing upper levels are added. Operations work across open maps and undo in one step; Delete leaves the clipboard unchanged.
- **Context menu** — Right-clicking an object outside the current selection switches to Select and selects that object. Right-clicking within a selected object group or Tiles area keeps the group or area. Every menu action applies to that highlighted selection. Right-clicking empty space outside the selection clears it and offers Paste at that location when the clipboard contains a map block. Properties updates immediately; there is no separate Edit command. **Use This Tool** samples a single selected object. Conflicting loaded plates at one location can be selected individually through **Select plate**.
- **Move / Duplicate** — Drag an already-selected object or inside a selected Tiles area to move the selection. **Duplicate** (`Ctrl/Cmd+D`) keeps the source and starts a preview. Click to place; Escape cancels. Object operations preserve other destination content, while Tiles mode replaces the destination area. Both leave the clipboard unchanged.
- **Handles** — Selecting one spawn or checkpoint zone shows its resize handles immediately. Drag a handle to resize; drag the body to move the zone. Selecting one nested map shows its endpoint handles; drag one to move just that end. Left-click and right-click produce the same selection, Properties, and handles. Alt/Option helps pick a zone or nested endpoint through overlapping objects.
- **Cancel** — Escape cancels an active drag or placement preview; press it again to clear the selection. Changing tool, level, map geometry, or selection scope clears selection. Copy, Delete, and other selection shortcuts act on the map only while the canvas has focus; property fields keep their normal text shortcuts.

- **Rotate / Mirror** — Use the Select toolbar, Edit menu, or selection’s right-click menu. Rotate turns clockwise by 90°; horizontal and vertical mirrors flip left/right and top/bottom. Place the preview to commit one undoable edit. Directional materials, ramps, lights, ladders, and nested motion transform together; a square ramp cannot turn, since its slope always runs north-south, so rotating a block that holds one is refused. Nested geometry receives a transformed copy so other placements keep their original definition; include each nested footprint at both motion ends.

## Jump Reach

- **Jump Reach** — Click any cell, including an empty one, to set a jump origin. Four colored markers, left to right, show reach with Normal, Speed, Anti-gravity, and Both on whichever level you view. A dot means no fall damage, a triangle means damage survivable at full health, and a cross means fatal even at full health. Hover lists each reachable combination and its estimated damage as a percentage of maximum health. The origin has a white dashed outline; another click replaces it. Highlights and the compact legend stay visible while using other tools and editing floors. **Clear** beside the legend or **Clear Jump Reach** in View removes them; Escape leaves them in place.
- **Takeoff margin / Walk or Run** — Controls appear in the top toolbar while Jump Reach is selected. Default to Run and a **0.100 s** margin for judging the edge and timing the jump. Adjust by **0.01 s** or type three decimal places; the adjacent distances show how much reach is reserved with and without speed. Settings last for this window's session. The guide uses the map's jump and movement settings, the shortest gap between expanded floor footprints, and descending landings in open space. Footprints include exposed-edge extensions and corner fillers, with the same ramp-join exceptions as the game. Empty cells are evaluated individually as potential flat floors using their current neighbors. Obstacles, ramp slopes, body size, and platform motion are ignored. The margin assumes supporting floor behind the takeoff edge. Zero gravity cannot produce a descending landing.
- **Fall damage** — Set `player_fall.safe_distance` and `player_fall.lethal_distance` in the map’s hand-edited `settings.json` (initially 8 m and 15 m). Damage rises from zero at the safe distance to full health at the lethal distance. Drops are measured from the jump’s highest point and adjusted for gravity, matching the game. Triangles can kill an injured player. Estimates assume power-ups last through landing.
- **Changes** — Movement, fall settings, and global maximum player health reload automatically. Adding or removing floors and changing ramp joins refreshes reach while keeping the origin selected, including undo/redo. Switching outer/nested geometry, replacing a document, resizing, or inserting/removing levels clears the origin; structural undo/redo also clears it.

## Portal Jump

- **Input selector** — Choose which of five inputs the next click sets: **Jump position**, **Shoot portal 1 from**, **Portal 1 position**, **Shoot portal 2 from**, or **Portal 2 position**. Inputs can be set in any order and edited independently; clicks do not advance the selector or clear other positions. **J** marks the jump position, **S1** and **S2** the shooting positions, and **1** and **2** the portals. Player positions are tiles on the currently viewed level.
- **Shooting positions** — Each shooting position initially follows the jump position. Select its input and click to set an independent override; **Use jump position** restores the default for that shooting position. Floor portal orientation follows the direction from its own shooting tile's center, using the game's quarter-turn snap. A portal directly under its shooting position uses north as the facing convention. Wall portals remain upright.
- **Output selector** — **Portal 1 reach** shows portal positions reachable from the jump position, using portal 1's shooting position for their orientation. **Portal 2 landings** shows where the player can land after traversing the selected pair. Changing output only changes the display; it neither changes the active input nor any selected position. Missing inputs, incompatible surfaces, overlap, or an unreachable entry produce an explanation instead of stale landing results, while preserving the selections for correction.
- **Scenarios** — Both outputs use colored markers for Normal, Speed, Anti-gravity, and Both. Landing dots mean safe, triangles mean damage, and crosses mean fatal at full health; hover reports the lowest achievable damage for each combination. Choose **Step** (the default) or **Jump** off the jump tile, and **Run** or **Walk**. Jump reserves the configured takeoff margin (initially 0.100 s); Step ignores it. Like Jump Reach, takeoff uses a tile footprint, not an exact standing point, and the margin assumes supporting floor behind the edge. Controls last for the window's session.
- **Floors and walls** — For a portal input, click inside a tile for a floor portal, or near a grid edge for a wall portal. The cursor's side of the edge chooses the wall face; the arrow points outward. Floor portals sit at tile centers. Wall portals sit at the midpoint of an edge with the portal's lower rim aligned to the wall's lower edge. The tool assumes enough backing, including any walls stacked above, and does not check wall or floor size. Existing faces must have portal-compatible materials. Ramps, ceilings, and terrain are excluded. The exit does not need to be reachable from the jump position, but cannot overlap the entry.
- **Planned geometry** — Dashed surface outlines indicate hypothetical floors or walls, or potential landing floors in empty cells. Existing incompatible surfaces stay unavailable; a missing surface assumes a compatible material. Selecting positions never creates geometry or edits the map.
- **Flight estimate** — Floor entry passes through the portal center; wall entry accepts the aperture's full height, including walking in below its center. The player's movement-center height and the game's clamped exit offset are accounted for. Wall-entry timing is sampled; post-exit steering uses continuous time intervals. The estimate keeps the possible incoming velocities, applies the game's portal rotation, and retains redirected falling momentum independently of air steering. Steering can stop or change direction throughout flight. A floor-to-wall exit therefore carries the fall horizontally; moving against that momentum does not simply cancel it. Terminal falling speed and the map's fall-damage settings apply. Anti-gravity remains active throughout the flight; zero gravity does not produce a descending landing in these scenarios.
- **Limits and persistence** — This is an open-space guide. It ignores intervening obstacles, body collisions, moving platforms, front clearance, placement nudging, fixtures, portal funnel assistance, and repeated crossings. It evaluates only the active geometry. The overlay and its selected output remain while switching tools, changing levels, and editing; relevant edits and settings reloads recompute it. Document replacement, geometry switching, resizing, or level-structure changes clear all five positions. **Clear**, or **Clear Portal Jump** in View, also removes them; Escape preserves them.

## Run Time

- **Run Time** — Click any cell, including an empty one, to set a run origin. Every other cell on that level shows the seconds from the origin's centre to its own in a straight line: the upper number at run speed, the lower with the speed power-up. Hover shows both to two decimals. The origin has a white dashed outline; another click replaces it. Numbers and the legend stay visible while using other tools. **Clear** beside the legend or **Clear Run Time** in View removes them; Escape leaves them in place.
- **Estimate** — Uses the map's `grid_cell_size`, `run_speed`, and `speed_power_up`, and the legend shows both speeds. Walls, obstacles, ramps, ladders, jumps, and platform motion are ignored, so the number is the shortest possible time. Numbers hide when cells are too small to hold them; hover still reports them. Movement settings reload automatically. Switching outer/nested geometry, replacing a document, resizing, or inserting/removing levels clears the origin.

## Floors

- **Floor** — Drag cells to add floor.
- **Blocked Floor** — Drag cells to add floor slabs that never spawn items, players, or lights.
- **Erase Floors** — Drag a rectangle to remove every regular, blocked, and terrain floor inside it; items standing on them go too.

## Terrain

- **Terrain** — Drag cells to add accessible floor slabs with the procedural outdoor terrain and grass on top. The selected material is used for the slab's bottom and four sides; right-click a terrain cell to edit those five faces independently. Painting another floor kind over terrain replaces it, and terrain replaces any floor already in its cells.
- **Erase Terrain** — Drag a rectangle to remove terrain floors on the current level.

## Spawn Zones

- **Actor Spawn Zone** — Choose Actor and Count in the toolbar, then drag a rectangle. **Controls…** sets First level, Levels, Roam extension in metres (default 0, inside the zone), Respawn, the delay before a killed actor's slot refills (Never fills the zone once), an optional switch and On/Off response, and Active until checkpoint (Always keeps the zone open) with Then, whether the zone only stops spawning or also self-destructs its actors once any player reaches that checkpoint; new zones reuse those values, and right-click edits existing zones. Without a plate assignment the zone spawns normally; a zone its plate holds back keeps counting down and refills once the plate allows. Count applies once across all levels. Immovable actors spawn at cell centers; surface suitability is left to the map designer. View → Show Roam Extensions (`R`, initially disabled) toggles the rounded roaming boundary.
- **Erase Spawn Zones** — Drag a rectangle to remove every actor spawn zone it touches on the current level.

## Checkpoints

A checkpoint is a rectangle of flat accessible floor that players respawn in once they have landed in it. Checkpoint 0 is the start: every player begins there, the game neither marks nor announces it, and every map needs at least one somewhere in its placed geometry. Several checkpoints may share a number, anywhere in the map or its placed nested geometry; the game then picks a spot among all of them. Checkpoints may overlap spawn zones but not each other; Map → Check Map reports overlaps, cells without flat floor, and a map without a start.

- **Checkpoint** — Set the Number and choose Type in the toolbar, then drag a rectangle. The toolbar offers the map's next free number and advances after each placement, except after a start, where it stays at 0 so several can be placed in a row; the Type control is off while the number is 0, since the start has no type. Individual saves the entrant's own respawn point, Group — any saves everyone's when one player enters, and Group — all waits until every player has visited. Select it to move it, resize with its handles, or change its number or type in Properties; Delete removes it. The canvas shows a start as Start and every other checkpoint by its number, which orders the course (a player only ever advances to a higher number), ends the actor zones that name it, and lets the game start or respawn there (`--checkpoint <number>` at launch, `/checkpoint <number>` in the console). Copies, duplicates, rotations, and mirrors keep their numbers. **Map → Edit Checkpoints…** lists each number of the map in course order, with how many checkpoints carry it, the maps they are in, and their types, to renumber or reorder the course below the fixed start; every checkpoint carrying a renumbered number follows it, as do the actor zones ending there.
- **Erase Checkpoints** — Drag a rectangle to remove every checkpoint it touches on the current level.

## Walls

- **Wall** — Drag along grid lines to place atomic wall edges.
- **Erase Walls** — Drag a rectangle to remove every wall edge inside or on its border; lights on those walls go too.

## Equipment Erasers

- **Equipment Eraser** — Drag along grid lines to place a walk-through field that removes a player's power-ups, missile ammo, and controlled portal ends, preserving keys.
- **Erase Equipment Erasers** — Drag a rectangle to remove every eraser edge inside or on its border.

## Barriers

- **Barrier** — Choose Kind and **Controls…** in the toolbar, then drag along grid lines. Controls choose a switch and whether to open when it is On or Off. Every barrier also accepts its matching key automatically. Select one or more barriers to edit their shared Properties.
- **Erase Barriers** — Drag a rectangle to remove every barrier edge inside or on its border.

## Light Bridges

- **Light Bridge** — Choose Kind and **Controls…** in the toolbar, then drag cells. Controls choose a switch and whether the walkway is powered when it is On or Off; without an assignment it is unpowered. Select one or more bridges to edit their shared Properties. Bridges cannot share cells with floors or ramps.
- **Erase Light Bridges** — Drag a rectangle to remove every light bridge inside it on the current level.

## Ramps

- **Ramp (Up)** — Drag from this level toward the upper level.
- **Ramp (Down)** — Drag from this level toward the lower level.
- **Erase Ramps** — Drag a rectangle to remove every ramp it touches that leaves from or arrives at the current level.
- **Edit Levels** — Adding a level between a ramp's endpoints removes that ramp. The Edit Levels dialog includes it in the removal summary before you apply the changes.

## Nested Maps

- **Create / Rename / Delete** — Map → New Nested Map creates named geometry in the parent file and selects it for editing. Rename Nested Map updates every placement of that name. Delete Nested Map removes an unused definition; erase its placements first if it is in use. All three actions can be undone.

- **Nested Map** — Click to place named nested geometry with its cell (0, 0) on that cell; drag to set a different second endpoint. The toolbar's **Settings…** configures new placements; Properties edits selected placements. Both show Map, Motion, Travel time, Cycle pause, Cycle phase, Switch, Respond when, End 2 level, and endpoint nudges in that order.
  - **Cycle** repeats between endpoints, pausing at each end for **Cycle pause**. **Cycle phase** offsets its starting point in that cycle. An optional switch pauses and resumes it where it is.
  - **Follow switch** requires a switch. It moves to end 2 while the switch matches **Respond when**, and to end 1 otherwise. It holds on arrival and reverses immediately from its current position when the switch changes. It starts at the endpoint matching the initially off switch. Cycle pause and phase stay visible but greyed out; their values are preserved when switching modes.
  - **Travel time** is the time for the full distance between endpoints in either mode. **Respond when** selects On or Off and is disabled without a switch.
  - Nudges displace each endpoint from its anchor: X and Z use wall widths; Y uses floor thicknesses. Two floors meeting at a grid line overlap by one wall width, so a nudge of 1.01 back along the travel leaves them just clear. A lift uses a different End 2 level.

Everything in the nested map rides along, including its own nested maps. On the canvas, numbered ends 1 and 2 show its resting footprints, joined by a band; a Y nudge is written after the name, and a red `name?` marks missing geometry. In Select, drag its outline or name to move the whole placement immediately; once selected, its interior also drags the placement. Endpoint handles move individual ends. Both gestures preview the full footprints, nudges, and motion span; Escape cancels the change. Unselected footprint interiors leave underlying objects clickable. The placement tool creates new placements and refuses occupied starting anchors. Nothing checks overlap with the surrounding map.

- **Erase Nested Maps** — Drag a rectangle to remove every nested map whose start or end cell on the current level is inside it.

## Ladders

- **Ladder** — Set Storeys in the toolbar, then click the cell where the ladder's rails should stand, near the edge it climbs; the hover ghost previews it under the cursor. The span starts at the current level and is capped at the map's top level. Ladders are climbable from both sides and block walking through below their top; no wall or floor is required — a ladder can stand at an open balcony front. Right-click a ladder on any level it spans to change its Storeys while keeping its base; overlapping another ladder is refused. Placing on an existing ladder leaves it unchanged. Select it and press Delete, or use Erase Ladders, to remove it.
Selecting a ladder highlights its rails and rungs on the climbing side of the edge. Move and duplicate previews follow that same shape.

- **Erase Ladders** — Drag a rectangle to remove every ladder whose anchor edge is inside it and whose span touches the current level.

## Materials

Right-click a floor, blocked floor, wall, or ramp to edit that element's materials. A ramp can be edited from either level it connects.

Faces with different materials across the selection start at **Mixed / unchanged**. Those faces keep their individual values unless you choose a material; **Apply Top to all faces** uses the Top choice for every face.
**Use top-left materials** fills all six fields from the topmost, then leftmost selected floor, wall, or ramp, independently of file order or drag direction. Each button commits all six faces as one edit.

- **Floor Material** — Click a single floor cell, or drag a rectangle to cover many; Properties edits the selected faces.
- **Wall Material** — Click a single wall to select it, or drag along grid lines to span many; Properties edits the selected faces.
- **Ramp Material** — Click any cell of a ramp, or drag a rectangle covering one or more ramps; Properties edits the selected faces.

## Lights

- **Light** — Choose a **Style** from the toolbar, then click a cell near a wall to add a wall light on that side; the hover ghost shows the side a click would use, and only where a wall accepts one. Right-click a light to change its style or erase it. Use **Edit → Auto-Place Lights** to choose a style and fill the current level on a stride; **Edit → Clear Lights On Level** to start over.
- **Erase Lights** — Drag a rectangle to remove every light inside it on the current level.

## Switches and Pressure Plates

Each tile can hold one pressure plate per level, on a floor or blocked floor outside ramp footprints. Several plates may operate the same switch and its targets. **Map → Switches** creates, edits, renames, and deletes switches. The root layout stores their policies and all target assignments; nested maps share the root catalog. Rename updates all references, and a switch still in use cannot be deleted.

Each switch chooses `momentary`, `toggle`, or `auto` activation, a death reset rule (`never`, `solo`, `any`, or `all`), and `any` or `everyone` holding. Auto toggles with one logged-in player and is momentary with several. Everyone requires each living player on a plate, or every plate occupied when players outnumber them. The optional color override is picked from a swatch; otherwise plates inherit the first linked barrier color, then bridge color, then the default fixture color.

Barriers, bridges, actor zones, and moving nested maps choose one switch and an On/Off response; **Map → Fireworks** chooses the switch alone and a cooldown. Without an assignment, barriers stay closed, bridges unpowered, actor zones active, and Cycle maps running. Follow switch requires an assignment. Fireworks need an assignment. **Map → Barrier Kinds** and **Bridge Kinds** edit the layout's appearance catalogs; each kind's color is picked from a swatch, and keys always match barrier kinds. Catalog changes share Save, Undo, and autosave recovery with the layout.

- **Pressure Plate** — Choose a Switch in the toolbar and click a cell. Select a plate to change its Switch in Properties or erase it. Its rim light and four indicators show the switch's state in-game.

- **Erase Pressure Plates** — Drag a rectangle to remove every plate inside it on the current level.

## Items

- **Item** — Choose the type in the toolbar (single shot, multi shot, missile pack, portal gun, health potion, speed, low gravity, gold, then key — keys also pick a barrier kind), then left-click a floor cell to place it. Right-click an item to change its type or erase it. Placed items hide on pickup in-game and reappear after the map's per-type `placed_items.respawn_secs` delay from its `settings.json`; zero allows immediate recollection, omitted or `null` entries never respawn, and `placed_items: null` makes all placed pickups one-time. Item glyphs match their in-game silhouettes: one white ball, three white balls, a rocket, an oval portal ring, a green health cross, speed chevrons, a white feather, a gold coin, and colored keys.
- **Erase Items** — Drag a rectangle to remove every item inside it on the current level.

## Erase

- **Erase** — Click an element to remove it, or drag a rectangle to clear every element inside it. Right-click for the context menu.
- **Erase (Keep Floors)** — The same, but floor, blocked floor, light bridge, and nested map anchor cells stay, along with the items and plates standing on them.

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Page Up` / `Page Down` | Next / previous level, including while editing number boxes |
| `←` / `→` | Previous / next palette tool, with canvas focus |
| `E` | Toggle Place / Erase for the selected element, with canvas focus |
| `I` | Sample the element under the cursor, with canvas focus |
| `Ctrl/Cmd+D` | Preview a duplicate of the selection |
| `M` | Toggle Show Material Overlay |
| `L` | Toggle Show Adjacent Levels |
| `R` | Toggle Show Roam Extensions |
| `Ctrl/Cmd+Plus` / `Ctrl/Cmd+Minus` | Zoom |
| Wheel / touch surface / Space-drag / middle-drag | Pan |
| Shift-wheel | Pan horizontally |
| `F` | Fit the whole map |
| `Enter` | Focus Properties for the selection, with canvas focus |
| `Ctrl/Cmd+Z` | Undo |
| `Ctrl/Cmd+Shift+Z` | Redo |
| `Ctrl/Cmd+C` | Copy selected objects or tiles |
| `Ctrl/Cmd+X` | Cut selected objects or tiles |
| `Ctrl/Cmd+V` | Paste objects or replace copied tile area |
| `Delete` / `Backspace` | Delete selected objects or tiles |
| `Ctrl/Cmd+A` | Select all objects or tiles |
| `Esc` | Cancel an active drag or preview; otherwise clear selection |
| `Shift` + click/drag | Toggle one object / add a rectangle of objects |
| `Alt/Option` + click/drag | Pick a zone or nested endpoint through overlapping objects |
| Drag a selected zone's handle | Resize the zone |
| Drag a selected nested map's endpoint handle | Move that endpoint |
| `Ctrl/Cmd+N` | New map |
| `Ctrl/Cmd+O` | Open |
| `Ctrl/Cmd+S` | Save |
| `Ctrl/Cmd+Shift+S` | Save As |
| `Ctrl/Cmd+Q` | Quit |
