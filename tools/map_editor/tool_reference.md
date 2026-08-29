# Tool Reference

## Floors

- **Floor** — Drag cells to add floor.
- **Blocked Floor** — Drag cells to add floor slabs that never spawn items, players, or lights.

## Grass

- **Grass** — Drag cells to paint decorative grass tufts (client visual only, no gameplay); only sticks to cells with a floor (regular or blocked). Erasing a floor removes its grass too.
- **Erase Grass** — Drag a rectangle to remove every grass tuft inside it on the current level.

## Spawn Zones

- **Actor Spawn Zone (Paint)** — Drag a rectangle, then enter Kind and Count.
- **Player Spawn Zone (Paint)** — Drag a rectangle. No prompt — players spawn anywhere in any player zone.
- **Spawn Zone (Edit)** — Click any spawn zone (actor or player) to select; drag the body to move, drag a corner/edge handle to resize. Right-click to edit fields (actor zones) or delete.

## Walls + Barriers

- **Wall** — Drag along grid lines to place atomic wall edges.
- **Barrier** — Drag along grid lines to place a translucent pulsating force-field; a dialog asks which kind to use. Kinds and colors are defined in `config/common/gameplay.json::barrier_kinds` + `config/client/assets.json::barrier_kind_colors`.

## Ramps

- **Ramp (Up)** — Drag from this level toward the upper level.
- **Ramp (Down)** — Drag from this level toward the lower level.

## Ladders

- **Ladder** — Click the cell where the ladder's rails should stand, near the edge it climbs; the hover ghost previews it under the cursor. A dialog asks how many storeys it spans (starting at the current level). Ladders are climbable from both sides and block walking through below their top; no wall or floor is required — a ladder can stand at an open balcony front. Click an existing ladder (from either side of its edge) to remove it.
- **Erase Ladders** — Drag a rectangle to remove every ladder whose anchor edge is inside it and whose span touches the current level.

## Materials

- **Floor Material** — Click a single floor cell, or drag a rectangle to cover many; the dialog assigns materials to every face.
- **Wall Material** — Click a single wall to select it, or drag along grid lines to span many; the dialog assigns materials to every face.
- **Ramp Material** — Click any cell of a ramp, or drag a rectangle covering one or more ramps; the dialog assigns materials to every face.

## Lights

- **Light** — Click a cell near a wall to add a wall light on that side; the hover ghost shows the side a click would use, and only where a wall accepts one. Click an existing light marker to remove it. Use **Edit → Auto-Place Lights** to fill the current level on a stride; **Edit → Clear Lights On Level** to start over.
- **Erase Lights** — Drag a rectangle to remove every light inside it on the current level.

## Pressure Plates

- **Barrier Plate** — Left-click a cell to place a plate (square in the barrier kind's color); a dialog asks which barrier kind. While enough plates of a kind are pressed — one fewer than the players alive, capped by the plate count — every barrier of that kind opens globally. Clicking a cell that already holds a plate removes it.
- **Firework Plate** — Left-click a cell to place a firework plate (circle). When every player alive stands on a firework plate — or every plate is held when players outnumber the plates — the firework show starts. Clicking a cell that already holds a plate removes it.
- **Erase Plates** — Drag a rectangle to remove every plate inside it on the current level.

## Items

- **Item** — Left-click a floor cell to place an item; a dialog asks the type (power-ups, health potion, cookie, or key — keys also pick a barrier kind). Clicking a cell that already holds an item removes it. Placed items hide on pickup in-game and reappear after the per-type `placed_items.respawn_secs` delay from `config/server/gameplay.json`. Non-key items render as colored circles; keys as diamonds in their barrier-kind color.
- **Erase Items** — Drag a rectangle to remove every item inside it on the current level.

## Erase

- **Erase** — Click an item, drag cells to erase an area, or right-click for the context menu.
- **Erase (Keep Floors)** — Erase walls, ramps, and spawn zones while preserving floor and inaccessible floor cells.

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
