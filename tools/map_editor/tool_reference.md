# Tool Reference

Every element group ends with its own **Erase** tool that removes only that element inside a dragged rectangle on the current level. The **Erase** group at the bottom holds the two tools that clear every element at once.

## Navigation and Tool Settings

- **Map** — Open a map registered by name in `gameplay.json`. Its folder contains the editor-owned `layout.json` and hand-edited `settings.json`. The Map picker switches between the outer map and its named nested geometry; all views use the parent's kinds and texture catalog. Save, autosave, and undo cover the whole document. Undo switches to the affected map.

- **Zoom / Pan / Fit Map** — `Cmd+Plus` / `Cmd+Minus` on macOS, or `Ctrl+Plus` / `Ctrl+Minus` elsewhere, zoom in and out. Scroll with a wheel, Magic Mouse, or touchpad to pan; Shift-wheel pans horizontally. Scroll bars appear when the map extends outside the view, and panning stops at the map edges. Space-drag and middle-drag also pan. View → Fit Map (`F`) shows the whole map.
- **Window** — Size, position, and maximized state are remembered across launches, shared by every map. Off-screen positions are brought back onto an available screen. New, Open, and Resize Map fit the canvas without changing the window size.
- **Tool Settings** — Properties appear beside the Tool picker in the top toolbar, only for tools that need them. Placement always uses the previous values; a dialog is needed only when no usable choice has been made yet. Change values in the toolbar, or open nested-map motion through **Settings…**. Right-click property editing still opens a dialog.
- **Single-tile tools** — Ladders, lights, plates, and items preview one tile or edge, never a range. Holding the mouse button lets you adjust the target; releasing places once. Escape or releasing off-grid cancels.
- **Feedback** — Placement warnings and copy confirmations appear briefly over the canvas without taking focus or blocking clicks. Undo and Redo menus name the available actions.
- **Hover details** — Hover any element to see its type and relevant properties, including materials, kind, actor count, or nested-map motion. Overlapping elements follow the same priority as right-click; material tools show their target's materials.
- **Map Issues / Review Repairs** — The toolbar's **Issues** button appears only when problems exist; it and View → Map Issues open the issues list. Click a result to select its map and level and highlight the object. Loading preserves invalid records; Edit → Review Repairs lists automatic changes for approval. Accepted repairs undo in one step. While repairs are pending, edits preserve records for manual correction; saving remains blocked by validation errors.
- **Recovery / Dependencies** — Unsaved maps receive recovery copies every 15 seconds, including untitled maps. Use File → Recover Unsaved Map to restore an untitled session after a crash; active sessions cannot be recovered by a second editor. Named maps offer newer autosaves when opened. Changes to the parent's gameplay catalogs refresh the editor automatically.

## Select Tiles

- **Select Tiles** — Where the editor starts. Click one tile or drag a rectangle to select tiles and their contents; empty tiles are selectable too. The blue outline marks the selection. Alt/Option-click a spawn zone to select it, then drag its handles normally to resize it or Alt/Option-drag its body to move it. Right-clicking a spawn zone also selects it. Alt/Option-drag a nested map's end square to move that end. In every tool, right-click an element to edit its properties or erase it.
- **Copy / Cut / Delete** — Available in Edit and the selection's right-click menu. Each asks how many levels to include, starting at the current level and going upward; the default is always 1. Copy and Cut put the entire block on the clipboard. Cut and Delete remove it. Walls, barriers, and equipment erasers on the rectangle's border are included. Include whole spawn zones, ramp footprints, ladder anchors and spans, and both ends of nested-map motion; a partial object prompts you to enlarge the selection. Removing a boundary wall with a light on its other side also needs that tile selected.
- **Paste** — Select the destination tile (or a rectangle whose top-left tile is the destination), then paste. The dashed outline previews the footprint to replace; its label shows the tile dimensions and level count. Paste replaces all contents, including empty cells in the copied block, starting on the current level. Missing levels are added at the top. A block outside the grid is refused, and incompatible map kinds are reported. The clipboard works across open maps and editor windows. Cut, Delete, and Paste each undo in one step; Delete leaves the clipboard unchanged.

## Jump Reach

- **Jump Reach** — Click any cell, including an empty one, to set a jump origin. Four colored markers, left to right, show reach with Normal, Speed, Anti-gravity, and Both on whichever level you view. A dot means no fall damage, a triangle means damage survivable at full health, and a cross means fatal even at full health. Hover lists each reachable combination and its estimated damage as a percentage of maximum health. The origin has a white dashed outline; another click replaces it. Highlights and the compact legend stay visible while using other tools and editing floors. **Clear** beside the legend or **Clear Jump Reach** in View removes them; Escape leaves them in place.
- **Takeoff margin / Walk or Run** — Controls appear beside the Tool selector while Jump Reach is selected. Default to Run and a **0.100 s** margin for judging the edge and timing the jump. Adjust by **0.01 s** or type three decimal places; the adjacent distances show how much reach is reserved with and without speed. Settings last for this window's session. The guide uses the map's jump and movement settings, the shortest gap between expanded floor footprints, and descending landings in open space. Footprints include exposed-edge extensions and corner fillers, with the same ramp-join exceptions as the game. Empty cells are evaluated individually as potential flat floors using their current neighbors. Obstacles, ramp slopes, body size, and platform motion are ignored. The margin assumes supporting floor behind the takeoff edge. Zero gravity cannot produce a descending landing.
- **Fall damage** — Set `player_fall.safe_distance` and `player_fall.lethal_distance` in the map’s hand-edited `settings.json` (initially 8 m and 15 m). Damage rises from zero at the safe distance to full health at the lethal distance. Drops are measured from the jump’s highest point and adjusted for gravity, matching the game. Triangles can kill an injured player. Estimates assume power-ups last through landing.
- **Changes** — Movement, fall settings, and global maximum player health reload automatically. Adding or removing floors and changing ramp joins refreshes reach while keeping the origin selected, including undo/redo. Switching outer/nested geometry, replacing a document, resizing, or inserting/removing levels clears the origin; structural undo/redo also clears it.

## Floors

- **Floor** — Drag cells to add floor.
- **Blocked Floor** — Drag cells to add floor slabs that never spawn items, players, or lights.
- **Erase Floors** — Drag a rectangle to remove every floor and blocked floor inside it; grass and items standing on them go too.

## Grass

- **Grass** — Drag cells to paint decorative grass tufts (client visual only, no gameplay); only sticks to cells with a floor (regular or blocked). Erasing a floor removes its grass too.
- **Erase Grass** — Drag a rectangle to remove every grass tuft inside it on the current level.

## Spawn Zones

- **Actor Spawn Zone** — Choose Actor, Count, and optionally the Switch that activates the zone in the toolbar, then drag a rectangle. If no actor is selected yet, the first placement asks for all three. A switched zone spawns nothing until its switch is on. Immovable actors spawn at cell centers; Count cannot exceed the zone's usable floor cells.
- **Player Spawn Zone** — Drag a rectangle. No prompt — players spawn anywhere in any player zone.
- **Erase Spawn Zones** — Drag a rectangle to remove every actor and player spawn zone it touches on the current level.

## Checkpoints

A checkpoint is a rectangle of flat accessible floor that players respawn in once they have landed in it. Checkpoints may overlap spawn zones but not each other; Map Issues reports overlaps and cells without flat floor.

- **Checkpoint** — Choose Type in the toolbar, then drag a rectangle. Individual saves the entrant's own respawn point, Group — any saves everyone's when one player enters, and Group — all waits until every player has visited. Alt-drag moves or resizes a checkpoint like a spawn zone; right-click it to change its type or erase it.
- **Erase Checkpoints** — Drag a rectangle to remove every checkpoint it touches on the current level.

## Walls

- **Wall** — Drag along grid lines to place atomic wall edges.
- **Erase Walls** — Drag a rectangle to remove every wall edge inside or on its border; lights on those walls go too.

## Equipment Erasers

- **Equipment Eraser** — Drag along grid lines to place a walk-through field that removes a player's power-ups, missile ammo, and controlled portal ends, preserving keys.
- **Erase Equipment Erasers** — Drag a rectangle to remove every eraser edge inside or on its border.

## Barriers

- **Barrier** — Choose Kind in the toolbar and drag along grid lines to place a translucent pulsating force-field. Kinds and their colors come from that map's `barrier_kinds` in its `settings.json`.
- **Erase Barriers** — Drag a rectangle to remove every barrier edge inside or on its border.

## Light Bridges

- **Light Bridge** — Choose Kind in the toolbar and drag cells to place a translucent walkway that is solid while its kind’s pressure switch is active. Kinds and their colors come from that map's `bridge_kinds` in its `settings.json`. The validator flags a bridge that shares a cell with a floor or a ramp.
- **Erase Light Bridges** — Drag a rectangle to remove every light bridge inside it on the current level.

## Ramps

- **Ramp (Up)** — Drag from this level toward the upper level.
- **Ramp (Down)** — Drag from this level toward the lower level.
- **Erase Ramps** — Drag a rectangle to remove every ramp it touches that leaves from or arrives at the current level.
- **Insert Level** — If insertion would separate a ramp's endpoints, the affected ramps are highlighted. Cancel, or remove them and insert the level as one undoable edit.

## Nested Maps

- **Create / Rename / Delete** — Edit → New Nested Map creates named geometry in the parent file and selects it for editing. Rename Nested Map updates every placement of that name. Delete Nested Map removes an unused definition; erase its placements first if it is in use. All three actions can be undone.

- **Nested Map** — Click a cell to place named nested geometry with its cell (0, 0) on it, standing still; drag to a second cell to make it slide there and back. A moving tile is a nested one-cell map (`tile`), and a lift is one whose far end is on another level. Placement reuses the motion configured under the toolbar's **Settings…** (the first placement opens it): which map (any other file in `config/server/maps`), the level of the far end, how long one leg takes, the pause at each end, a phase offset, a nudge for each end, its (x, y, z) displacement from the anchor, x and z in wall widths (across columns and rows) and y in floor widths (up), zero by default, and optionally the switch that runs the motion (a switched map moves only while its switch is on and freezes where it is otherwise); two floors meeting at a grid line overlap by one wall width, so a nudge of 1.01 back along the travel leaves them just clear. Everything in the nested map rides along: floors, walls, ladders, plates, items, and its own nested maps. On the canvas the ends are numbered squares 1 and 2 with a band between them, and the nested map's footprint is outlined and named where it rests at each end, its nudge applied, solid where it starts and dashed where it arrives (a y nudge cannot be drawn on the plan, so it is written after the name); a red `name?` is a missing geometry definition. Nothing checks for overlap with the map around it. Dragging from an end moves that end, clicking an end opens the entry's properties, and right-clicking an end offers the same in any tool, beside Erase.
- **Erase Nested Maps** — Drag a rectangle to remove every nested map whose start or end cell on the current level is inside it.

## Ladders

- **Ladder** — Set Storeys in the toolbar, then click the cell where the ladder's rails should stand, near the edge it climbs; the hover ghost previews it under the cursor. The span starts at the current level and is capped at the map's top level. Ladders are climbable from both sides and block walking through below their top; no wall or floor is required — a ladder can stand at an open balcony front. Click an existing ladder (from either side of its edge) to remove it.
- **Erase Ladders** — Drag a rectangle to remove every ladder whose anchor edge is inside it and whose span touches the current level.

## Materials

Faces with different materials across the selection start at **Mixed / leave unchanged**. Those faces keep their individual values unless you choose a material; **Apply Top to all faces** uses the Top choice for every face.
**Use top-left materials** fills all six fields from the topmost, then leftmost selected floor, wall, or ramp, independently of file order or drag direction. You can adjust the fields before pressing OK; Cancel leaves the map unchanged.

- **Floor Material** — Click a single floor cell, or drag a rectangle to cover many; the dialog assigns materials to every face.
- **Wall Material** — Click a single wall to select it, or drag along grid lines to span many; the dialog assigns materials to every face.
- **Ramp Material** — Click any cell of a ramp, or drag a rectangle covering one or more ramps; the dialog assigns materials to every face.

## Lights

- **Light** — Choose a **Style** from the toolbar, then click a cell near a wall to add a wall light on that side; the hover ghost shows the side a click would use, and only where a wall accepts one. Right-click a light to erase it. Use **Edit → Auto-Place Lights** to choose a style and fill the current level on a stride; **Edit → Clear Lights On Level** to start over.
- **Erase Lights** — Drag a rectangle to remove every light inside it on the current level.

## Pressure Plates

Each tile can hold one pressure plate on each level. A plate needs a floor or blocked floor under it and cannot sit inside a ramp footprint; the hover ghost shows only where a click would succeed, and loaded plates elsewhere are reported in Map Issues. Every plate operates one of the map's switches, listed in its `settings.json` under `switches`; what a switch does is declared on its targets: a barrier or bridge kind's `switch` in the same file, an actor spawn zone's or nested map's Switch in the editor, and the `fireworks` block. Plates use a symmetric frame with four indicators and a panel coloured like the first linked barrier or bridge kind; hover names the switch. Set a switch's `plate_color` (`#rrggbb`) for other uses or to override inheritance; otherwise the client asset catalog's `pressure_plate.default_color` supplies the color. Right-click a plate to edit or erase it. Configure each switch directly in `settings.json`: `activation` is `momentary`, `toggle`, or `auto`; `reset_on_player_death` is `never`, `solo`, `any`, or `all`; `held` is `any` (one occupied plate) or `everyone` (every living player on one of its plates, or every plate when players outnumber them). Auto toggles with one logged-in player and is momentary with multiple players. The editor reads switch ids; it does not edit these policies. Map Issues reports a plate on an unknown switch and a zone or nested map whose switch no plate in the document operates.

- **Pressure Plate** — Choose Switch in the toolbar and left-click a cell to place a plate. Right-click a plate to change its switch or erase it.
- **Erase Pressure Plates** — Drag a rectangle to remove every plate inside it on the current level.

## Items

- **Item** — Choose the type in the toolbar (single shot, multi shot, missile pack, portal gun, health potion, speed, low gravity, gold, then key — keys also pick a barrier kind), then left-click a floor cell to place it. Right-click an item to change its type or erase it. Placed items hide on pickup in-game and reappear after the map's per-type `placed_items.respawn_secs` delay from its `settings.json`. Item glyphs match their in-game silhouettes: one white ball, three white balls, a rocket, an oval portal ring, a green health cross, speed chevrons, a white feather, a gold coin, and colored keys.
- **Erase Items** — Drag a rectangle to remove every item inside it on the current level.

## Erase

- **Erase** — Click an element to remove it, or drag a rectangle to clear every element inside it. Right-click for the context menu.
- **Erase (Keep Floors)** — The same, but floor, blocked floor, light bridge, and nested map anchor cells stay, along with the items and plates standing on them.

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Page Up` / `Page Down` | Next / previous level, including while editing number boxes |
| `←` / `→` | Previous / next tool |
| `M` | Toggle Show Material Overlay |
| `L` | Toggle Show Adjacent Levels |
| `Ctrl/Cmd+Plus` / `Ctrl/Cmd+Minus` | Zoom |
| Wheel / touch surface / Space-drag / middle-drag | Pan |
| Shift-wheel | Pan horizontally |
| `F` | Fit the whole map |
| `Ctrl/Cmd+Z` | Undo |
| `Ctrl/Cmd+Shift+Z` | Redo |
| `Ctrl/Cmd+C` | Copy selected tiles; ask level count |
| `Ctrl/Cmd+X` | Cut selected tiles; ask level count |
| `Ctrl/Cmd+V` | Replace destination with copied block |
| `Delete` / `Backspace` | Delete selected tiles; ask level count |
| `Ctrl/Cmd+A` | Select all tiles |
| `Esc` | Clear selection / cancel the current drag |
| `Alt/Option` + click/drag | Select or move a spawn zone; move a nested-map end |
| Drag a selected spawn zone's handle | Resize the zone |
| `Ctrl/Cmd+N` | New map |
| `Ctrl/Cmd+O` | Open |
| `Ctrl/Cmd+S` | Save |
| `Ctrl/Cmd+Shift+S` | Save As |
| `Ctrl/Cmd+Q` | Quit |
