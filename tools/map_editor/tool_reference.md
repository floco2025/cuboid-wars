# Tool Reference

Every element group ends with its own **Erase** tool that removes only that element inside a dragged rectangle on the current level. The **Erase** group at the bottom holds the two tools that clear every element at once.

## No Tool

- **No Tool** — Where the editor starts. Click a spawn zone to select it, then drag its body to move it or a corner/edge handle to resize it; drag a nested map's end square to move that end. A press anywhere else does nothing, so a stray click draws nothing. In every tool, right-click an element to edit its properties or erase it.

## Floors

- **Floor** — Drag cells to add floor.
- **Blocked Floor** — Drag cells to add floor slabs that never spawn items, players, or lights.
- **Erase Floors** — Drag a rectangle to remove every floor and blocked floor inside it; grass and items standing on them go too.

## Grass

- **Grass** — Drag cells to paint decorative grass tufts (client visual only, no gameplay); only sticks to cells with a floor (regular or blocked). Erasing a floor removes its grass too.
- **Erase Grass** — Drag a rectangle to remove every grass tuft inside it on the current level.

## Spawn Zones

- **Actor Spawn Zone** — Drag a rectangle, then enter Kind and Count.
- **Player Spawn Zone** — Drag a rectangle. No prompt — players spawn anywhere in any player zone.
- **Erase Spawn Zones** — Drag a rectangle to remove every actor and player spawn zone it touches on the current level.

## Walls

- **Wall** — Drag along grid lines to place atomic wall edges.
- **Erase Walls** — Drag a rectangle to remove every wall edge inside or on its border; lights on those walls go too.

## Barriers

- **Barrier** — Drag along grid lines to place a translucent pulsating force-field; a dialog asks which kind to use. Kinds and their colors come from that map's `barrier_kinds` in `config/server/gameplay.json`.
- **Erase Barriers** — Drag a rectangle to remove every barrier edge inside or on its border.

## Light Bridges

- **Light Bridge** — Drag cells to place a translucent walkway that is solid only while a bridge plate of its kind is held; a dialog asks which kind to use. Kinds and their colors come from that map's `bridge_kinds` in `config/server/gameplay.json`. The validator flags a bridge that shares a cell with a floor or a ramp.
- **Erase Light Bridges** — Drag a rectangle to remove every light bridge inside it on the current level.

## Ramps

- **Ramp (Up)** — Drag from this level toward the upper level.
- **Ramp (Down)** — Drag from this level toward the lower level.
- **Erase Ramps** — Drag a rectangle to remove every ramp it touches that leaves from or arrives at the current level.

## Nested Maps

- **Nested Map** — Click a cell to place another map file with its cell (0, 0) on it, standing still; drag to a second cell to make it slide there and back. A moving tile is a nested one-cell map (`tile`), and a lift is one whose far end is on another level. A dialog asks which map (any other file in `config/server/maps`), the level of the far end, how long one leg takes, the pause at each end, a phase offset, and a nudge for each end, its (x, y, z) displacement from the anchor, x and z in wall widths (across columns and rows) and y in floor widths (up), zero by default; two floors meeting at a grid line overlap by one wall width, so a nudge of 1.01 back along the travel leaves them just clear. Everything in the nested map rides along: floors, walls, ladders, plates, items, and its own nested maps. On the canvas the ends are numbered squares 1 and 2 with a band between them, and the nested map's footprint is outlined and named where it rests at each end, its nudge applied, solid where it starts and dashed where it arrives (a y nudge cannot be drawn on the plan, so it is written after the name); a red `name?` is a map file that is missing. Nothing checks for overlap with the map around it. Dragging from an end moves that end, clicking an end opens the entry's properties, and right-clicking an end offers the same in any tool, beside Erase.
- **Erase Nested Maps** — Drag a rectangle to remove every nested map whose start or end cell on the current level is inside it.

## Ladders

- **Ladder** — Click the cell where the ladder's rails should stand, near the edge it climbs; the hover ghost previews it under the cursor. A dialog asks how many storeys it spans (starting at the current level). Ladders are climbable from both sides and block walking through below their top; no wall or floor is required — a ladder can stand at an open balcony front. Click an existing ladder (from either side of its edge) to remove it.
- **Erase Ladders** — Drag a rectangle to remove every ladder whose anchor edge is inside it and whose span touches the current level.

## Materials

- **Floor Material** — Click a single floor cell, or drag a rectangle to cover many; the dialog assigns materials to every face.
- **Wall Material** — Click a single wall to select it, or drag along grid lines to span many; the dialog assigns materials to every face.
- **Ramp Material** — Click any cell of a ramp, or drag a rectangle covering one or more ramps; the dialog assigns materials to every face.

## Lights

- **Light** — Click a cell near a wall to add a wall light on that side; the hover ghost shows the side a click would use, and only where a wall accepts one. Right-click a light to erase it. Use **Edit → Auto-Place Lights** to fill the current level on a stride; **Edit → Clear Lights On Level** to start over.
- **Erase Lights** — Drag a rectangle to remove every light inside it on the current level.

## Pressure Plates

- **Barrier Plate** — Left-click a cell to place a plate (square in the barrier kind's color); a dialog asks which barrier kind. While enough plates of a kind are pressed — one fewer than the players alive, capped by the plate count — every barrier of that kind opens globally. Right-click a plate to change its kind or erase it.
- **Bridge Plate** — Left-click a cell to place a plate (diamond in the bridge kind's color); a dialog asks which bridge kind. While enough plates of a kind are pressed — the same count as for barrier plates — every light bridge of that kind turns solid. Right-click a plate to change its kind or erase it.
- **Firework Plate** — Left-click a cell to place a firework plate (circle). When every player alive stands on a firework plate — or every plate is held when players outnumber the plates — the firework show starts. Right-click a plate to change its kind or erase it.
- **Erase Pressure Plates** — Drag a rectangle to remove every plate inside it on the current level.

## Items

- **Item** — Left-click a floor cell to place an item; a dialog asks the type (power-ups, health potion, cookie, or key — keys also pick a barrier kind). Right-click an item to change its type or erase it. Placed items hide on pickup in-game and reappear after the map's per-type `placed_items.respawn_secs` delay from `config/server/gameplay.json`. Non-key items render as colored circles; keys as diamonds in their barrier-kind color.
- **Erase Items** — Drag a rectangle to remove every item inside it on the current level.

## Erase

- **Erase** — Click an element to remove it, or drag a rectangle to clear every element inside it. Right-click for the context menu.
- **Erase (Keep Floors)** — The same, but floor, blocked floor, light bridge, and nested map anchor cells stay, along with the items and plates standing on them.

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `↑` / `↓` | Next / previous level |
| `←` / `→` | Previous / next tool |
| `M` | Toggle Show Material Overlay |
| `Shift+M` | Toggle Show Adjacent Levels |
| `Ctrl/Cmd+Z` | Undo |
| `Ctrl/Cmd+Shift+Z` | Redo |
| `Ctrl/Cmd+N` | New map |
| `Ctrl/Cmd+O` | Open |
| `Ctrl/Cmd+S` | Save |
| `Ctrl/Cmd+Shift+S` | Save As |
| `Ctrl/Cmd+Q` | Quit |
