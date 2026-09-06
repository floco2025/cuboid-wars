# Editor review — September 6, 2026

The review covered canvas input and painting, placement and erasing, dialogs, map transformations, validation, persistence, undo, nested-map dependencies, and the Python tests. Map and configuration file formats are unchanged.

## Implemented

- Tile selection and system-clipboard copy/cut/paste, plus delete, select-all, deselect, native shortcuts, and a canvas preview of the paste footprint. Copy, cut, and delete ask for a level count starting at 1; paste replaces the destination and adds upper levels when necessary. Whole-object boundary rules live in [regions.py](regions.py), and the interactions are documented in [Tool Reference](tool_reference.md).
- Failed Save As preserves the original file identity, catalogs, dirty state, and recovery file. Save and Open include nested-map validation; Open reuses the data it already read. Save/Discard/Cancel is available when leaving unsaved work.
- Undoing back to the saved contents clears the dirty indicator. New files begin dirty. Canceled or failed file transitions retain the current recovery copy, and opening another map clears its predecessor's selection.
- Tool changes, level changes, and Escape cancel unfinished drags and clear stale hover state. Arrow-key tool/level shortcuts belong to the canvas, leaving toolbar controls their own keyboard navigation.
- Most edits canonicalize the map once instead of five times. Tool and level navigation no longer repeat whole-map validation. Nested-map cycle detection visits each completed dependency only once.
- Resize warnings count every object family, including items, plates, bridges, barriers, and ladders. A 1×1 new map has an in-bounds spawn zone. The stale test expecting no barrier kinds in `obby` now matches its configuration.

## Recommended next changes

1. **Make repairs during loading visible.** [io.py](io.py)'s `read_map` runs `canonicalize_map` before validation. [normalization.py](normalization.py) removes unsupported items, lights, invalid ladder spans, conflicting barriers, and invalid nested placements. A malformed input can therefore lose records before the editor reports its issues. Separate parsing/default expansion from repair, report proposed removals, and make an accepted repair undoable. This needs no schema change.

2. **Handle insertion through ramps explicitly.** [structure.py](structure.py)'s `insert_level_data` shifts a ramp only when its lower level is at or above the insertion. Inserting between its endpoints leaves it connected to the inserted level while the original upper level moves away. The file format expresses a ramp between adjacent levels only. Show the affected ramps and either refuse that insertion or offer their removal as part of the same undoable edit.

3. **Add zoom, pan, and Fit Map.** [canvas.py](canvas.py)'s `cell_size` clamps tiles to at least 12 pixels; there is no viewport transform or scrolling. A 256-column map needs at least 3,072 pixels of canvas width, making parts unreachable in a smaller window. One shared world-to-view transform should drive painting and hit testing, with wheel zoom, space-drag pan, and a Fit Map action. Paint only the visible region once this exists.

4. **Align pressure-plate editing with purpose identity.** [normalization.py](normalization.py)'s `pressure_plate_key` permits different purposes on one cell, including two barrier kinds. [placement.py](placement.py)'s `_add_plate` checks type alone, and `edit_pressure_plate_at` changes every plate of that type on the cell. Use the complete purpose in hit targets and menu labels, so editing one kind cannot rewrite another.

5. **Use catalog-backed actor choices and mixed-value material fields.** `ActorSpawnFieldsDialog` accepts arbitrary text, and [validation.py](validation.py) checks only that it is nonempty; the server rejects unknown actor kinds in `server/src/config/validation.rs`. Populate a searchable actor picker from gameplay settings. Material dialogs currently seed every field from the first selected segment, so accepting an unrelated face edit overwrites differences on other faces. Add an explicit “Mixed / leave unchanged” value and apply only edited fields.

6. **Let the document own edit transactions.** `SetMapCommand` holds an entire window and invokes `window.set_map`; domain mixins rely on a large implicit window interface. Keep widget orchestration in the window, put pure transformations beside their domains, and route transactions through `MapDocument` with a change signal. The volume operations provide a useful starting pattern. Keep the existing release-tool dispatch table; a general plugin framework would add little here.

7. **Improve feedback and persistence for longer sessions.** Make validation issues clickable so they focus the relevant level and highlight the object on the canvas. Remember property choices in a compact inspector for repeated placement. [document.py](document.py) currently skips autosave for an untitled map, and [nested_maps.py](nested_maps.py)'s shape cache stays stale until an explicit reset: a session recovery path and dependency-file change notifications would address those gaps.

## Verification and performance

The headless suite passes 70 tests, covering all clipboard object families, vertical translation, replacement of empty cells, boundaries, cancellation, keyboard/mouse input, undo/redo, file transitions, failed saves, and resize warnings. The selection and paste outline were also rendered and visually inspected with Qt's offscreen backend. Native desktop clipboard transfer between two running processes was not manually exercised.

Local ten-iteration averages were about 8.9 ms to canonicalize `hotel` and 2.9 ms to validate it; `obby` took about 0.5 ms and 0.2 ms respectively. These are helper timings, not end-to-end frame measurements. Prioritize the workflow issues above; profile repainting and undo-memory growth before introducing spatial indexes, differential undo commands, or background validation.

Run the suite with:

```bash
PYTHONPATH=tools QT_QPA_PLATFORM=offscreen python3 -m unittest discover -s tools/tests -p 'test_*.py'
```
