# Tool Reference

## Floors

- **Floor** — Drag cells to add floor.
- **Blocked Floor** — Drag cells to add floor slabs that never spawn items, players, or lights.

## Spawn Zones

- **Actor Spawn Zone (Paint)** — Drag a rectangle, then enter Kind and Count.
- **Player Spawn Zone (Paint)** — Drag a rectangle. No prompt — players spawn anywhere in any player zone.
- **Cookie Spawn Zone (Paint)** — Drag a rectangle. Cookies only spawn on walkable floors inside one of these zones.
- **Key Spawn Zone (Paint)** — Drag a rectangle, then pick a kind from the dialog. One key of that kind spawns at the first eligible cell of the zone and respawns after collection.
- **Spawn Zone (Edit)** — Click any spawn zone (actor, player, cookie, or key) to select; drag the body to move, drag a corner/edge handle to resize. Right-click to edit fields (actor and key zones) or delete.

## Walls + Barriers

- **Wall** — Drag along grid lines to place atomic wall edges.
- **Barrier** — Drag along grid lines to place a translucent pulsating force-field; a dialog asks which kind to use. Kinds and colors are defined in `config/common/gameplay.json::barrier_kinds` + `config/client/assets.json::barrier_kind_colors`.

## Ramps

- **Ramp (Up)** — Drag from this level toward the upper level.
- **Ramp (Down)** — Drag from this level toward the lower level.

## Materials

- **Floor Material** — Click a single floor cell, or drag a rectangle to cover many; the dialog assigns materials to every face.
- **Wall Material** — Click a single wall to select it, or drag along grid lines to span many; the dialog assigns materials to every face.
- **Ramp Material** — Click any cell of a ramp, or drag a rectangle covering one or more ramps; the dialog assigns materials to every face.

## Lights

- **Light** — Click a cell near a wall to add a wall light on that side; click an existing light marker to remove it. Use **Edit → Auto-Place Lights** to fill the current level on a stride; **Edit → Clear Lights On Level** to start over.
- **Erase Lights** — Drag a rectangle to remove every light inside it on the current level.

## Pressure Plates

- **Pressure Plate** — Left-click a cell to place a plate; a dialog asks which barrier kind. While enough plates of a kind are pressed, every barrier of that kind opens globally. Right-click a plate to remove it.

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
