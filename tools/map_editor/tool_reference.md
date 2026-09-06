# Tool Reference

Every element group ends with its own **Erase** tool that removes only that element inside a dragged rectangle on the current level. The **Erase** group at the bottom holds the two tools that clear every element at once.

## Navigation and Tool Settings

- **Zoom / Pan / Fit Map** — Wheel to zoom around the pointer; hold Space and drag, or middle-drag, to pan. View → Fit Map (`F`) shows the whole map. Zooming in reveals small details; only visible geometry is painted.
- **Window** — Size, position, and maximized state are remembered across launches, shared by every map. Off-screen positions are brought back onto an available screen. New, Open, and Resize Map fit the canvas without changing the window size.
- **Tool Settings** — Properties appear beside the Tool picker in the top toolbar, only for tools that need them. Placement always uses the previous values; a dialog is needed only when no usable choice has been made yet. Change values in the toolbar, or open nested-map motion through **Settings…**. Right-click property editing still opens a dialog.
- **Single-tile tools** — Ladders, lights, plates, and items preview one tile or edge, never a range. Holding the mouse button lets you adjust the target; releasing places once. Escape or releasing off-grid cancels.
- **Feedback** — Placement warnings and copy confirmations appear briefly over the canvas without taking focus or blocking clicks. Undo and Redo menus name the available actions.
- **Map Issues / Review Repairs** — The toolbar's **Issues** button appears only when problems exist; it and View → Map Issues open the issues list. Click a result to focus its level and highlight the object. Loading preserves invalid records; Edit → Review Repairs lists automatic changes for approval. Accepted repairs undo in one step. While repairs are pending, edits preserve records for manual correction; saving remains blocked by validation errors.
- **Recovery / Dependencies** — Unsaved maps receive recovery copies every 15 seconds, including untitled maps. Use File → Recover Unsaved Map to restore an untitled session after a crash; active sessions cannot be recovered by a second editor. Named maps offer newer autosaves when opened. Changes to nested-map files and gameplay/material catalogs refresh the editor automatically.

## Select Tiles

- **Select Tiles** — Where the editor starts. Click one tile or drag a rectangle to select tiles and their contents; empty tiles are selectable too. The blue outline marks the selection. Alt/Option-click a spawn zone to select it, then Alt/Option-drag its body or handles to move or resize it; Alt/Option-drag a nested map's end square to move that end. In every tool, right-click an element to edit its properties or erase it.
- **Copy / Cut / Delete** — Available in Edit and the selection's right-click menu. Each asks how many levels to include, starting at the current level and going upward; the default is always 1. Copy and Cut put the entire block on the clipboard. Cut and Delete remove it. Walls and barriers on the rectangle's border are included. Include whole spawn zones, ramp footprints, ladder anchors and spans, and both ends of nested-map motion; a partial object prompts you to enlarge the selection. Removing a boundary wall with a light on its other side also needs that tile selected.
- **Paste** — Select the destination tile (or a rectangle whose top-left tile is the destination), then paste. The dashed outline previews the footprint to replace; its label shows the tile dimensions and level count. Paste replaces all contents, including empty cells in the copied block, starting on the current level. Missing levels are added at the top. A block outside the grid is refused, and incompatible map kinds are reported. The clipboard works across open maps and editor windows. Cut, Delete, and Paste each undo in one step; Delete leaves the clipboard unchanged.

## Floors

- **Floor** — Drag cells to add floor.
- **Blocked Floor** — Drag cells to add floor slabs that never spawn items, players, or lights.
- **Erase Floors** — Drag a rectangle to remove every floor and blocked floor inside it; grass and items standing on them go too.

## Grass

- **Grass** — Drag cells to paint decorative grass tufts (client visual only, no gameplay); only sticks to cells with a floor (regular or blocked). Erasing a floor removes its grass too.
- **Erase Grass** — Drag a rectangle to remove every grass tuft inside it on the current level.

## Spawn Zones

- **Actor Spawn Zone** — Choose Actor and Count in the toolbar, then drag a rectangle. If no actor is selected yet, the first placement asks for one.
- **Player Spawn Zone** — Drag a rectangle. No prompt — players spawn anywhere in any player zone.
- **Erase Spawn Zones** — Drag a rectangle to remove every actor and player spawn zone it touches on the current level.

## Walls

- **Wall** — Drag along grid lines to place atomic wall edges.
- **Erase Walls** — Drag a rectangle to remove every wall edge inside or on its border; lights on those walls go too.

## Barriers

- **Barrier** — Choose Kind in the toolbar and drag along grid lines to place a translucent pulsating force-field. Kinds and their colors come from that map's `barrier_kinds` in `config/server/gameplay.json`.
- **Erase Barriers** — Drag a rectangle to remove every barrier edge inside or on its border.

## Light Bridges

- **Light Bridge** — Choose Kind in the toolbar and drag cells to place a translucent walkway that is solid only while a bridge plate of its kind is held. Kinds and their colors come from that map's `bridge_kinds` in `config/server/gameplay.json`. The validator flags a bridge that shares a cell with a floor or a ramp.
- **Erase Light Bridges** — Drag a rectangle to remove every light bridge inside it on the current level.

## Ramps

- **Ramp (Up)** — Drag from this level toward the upper level.
- **Ramp (Down)** — Drag from this level toward the lower level.
- **Erase Ramps** — Drag a rectangle to remove every ramp it touches that leaves from or arrives at the current level.
- **Insert Level** — If insertion would separate a ramp's endpoints, the affected ramps are highlighted. Cancel, or remove them and insert the level as one undoable edit.

## Nested Maps

- **Nested Map** — Click a cell to place another map file with its cell (0, 0) on it, standing still; drag to a second cell to make it slide there and back. A moving tile is a nested one-cell map (`tile`), and a lift is one whose far end is on another level. Placement reuses the motion configured under the toolbar's **Settings…** (the first placement opens it): which map (any other file in `config/server/maps`), the level of the far end, how long one leg takes, the pause at each end, a phase offset, and a nudge for each end, its (x, y, z) displacement from the anchor, x and z in wall widths (across columns and rows) and y in floor widths (up), zero by default; two floors meeting at a grid line overlap by one wall width, so a nudge of 1.01 back along the travel leaves them just clear. Everything in the nested map rides along: floors, walls, ladders, plates, items, and its own nested maps. On the canvas the ends are numbered squares 1 and 2 with a band between them, and the nested map's footprint is outlined and named where it rests at each end, its nudge applied, solid where it starts and dashed where it arrives (a y nudge cannot be drawn on the plan, so it is written after the name); a red `name?` is a map file that is missing. Nothing checks for overlap with the map around it. Dragging from an end moves that end, clicking an end opens the entry's properties, and right-clicking an end offers the same in any tool, beside Erase.
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

- **Light** — Click a cell near a wall to add a wall light on that side; the hover ghost shows the side a click would use, and only where a wall accepts one. Right-click a light to erase it. Use **Edit → Auto-Place Lights** to fill the current level on a stride; **Edit → Clear Lights On Level** to start over.
- **Erase Lights** — Drag a rectangle to remove every light inside it on the current level.

## Pressure Plates

Different purposes may share a tile, including multiple barrier or bridge kinds. Right-click actions name each purpose and edit or erase only that plate.

- **Barrier Plate** — Choose Kind in the toolbar and left-click a cell to place a plate (square in the barrier kind's color). While enough plates of a kind are pressed — one fewer than the players alive, capped by the plate count — every barrier of that kind opens globally. Right-click a plate to change its kind or erase it.
- **Bridge Plate** — Choose Kind in the toolbar and left-click a cell to place a plate (diamond in the bridge kind's color). While enough plates of a kind are pressed — the same count as for barrier plates — every light bridge of that kind turns solid. Right-click a plate to change its kind or erase it.
- **Firework Plate** — Left-click a cell to place a firework plate (circle). When every player alive stands on a firework plate — or every plate is held when players outnumber the plates — the firework show starts. Right-click a plate to erase it.
- **Erase Pressure Plates** — Drag a rectangle to remove every plate inside it on the current level.

## Items

- **Item** — Choose the type in the toolbar (power-ups, health potion, cookie, or key — keys also pick a barrier kind), then left-click a floor cell to place it. Right-click an item to change its type or erase it. Placed items hide on pickup in-game and reappear after the map's per-type `placed_items.respawn_secs` delay from `config/server/gameplay.json`. Non-key items render as colored circles; keys as diamonds in their barrier-kind color.
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
| `L` | Toggle Show Adjacent Levels |
| Wheel / `Ctrl/Cmd+Plus` / `Ctrl/Cmd+Minus` | Zoom |
| Space-drag / middle-drag | Pan |
| `F` | Fit the whole map |
| `Ctrl/Cmd+Z` | Undo |
| `Ctrl/Cmd+Shift+Z` | Redo |
| `Ctrl/Cmd+C` | Copy selected tiles; ask level count |
| `Ctrl/Cmd+X` | Cut selected tiles; ask level count |
| `Ctrl/Cmd+V` | Replace destination with copied block |
| `Delete` / `Backspace` | Delete selected tiles; ask level count |
| `Ctrl/Cmd+A` | Select all tiles |
| `Esc` | Clear selection / cancel the current drag |
| `Alt/Option` + click/drag | Select, move, or resize a spawn zone; move a nested-map end |
| `Ctrl/Cmd+N` | New map |
| `Ctrl/Cmd+O` | Open |
| `Ctrl/Cmd+S` | Save |
| `Ctrl/Cmd+Shift+S` | Save As |
| `Ctrl/Cmd+Q` | Quit |
