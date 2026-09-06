# Editor review — September 6, 2026

The review covered canvas input and painting, placement and erasing, dialogs, map transformations, validation, persistence, undo, nested-map dependencies, and the Python tests. Map and configuration file formats are unchanged.

## Implemented

- Tile selection and system-clipboard copy/cut/paste, plus delete, select-all, deselect, native shortcuts, and a canvas preview of the paste footprint. Copy, cut, and delete ask for a level count starting at 1; paste replaces the destination and adds upper levels when necessary. Whole-object boundary rules live in [regions.py](regions.py), and the interactions are documented in [Tool Reference](tool_reference.md).
- Failed Save As preserves the original file identity, catalogs, dirty state, and recovery file. Save and Open include nested-map validation; Open reuses the data it already read. Save/Discard/Cancel is available when leaving unsaved work.
- Undoing back to the saved contents clears the dirty indicator. New files begin dirty. Canceled or failed file transitions retain the current recovery copy, and opening another map clears its predecessor's selection.
- Tool changes, level changes, and Escape cancel unfinished drags and clear stale hover state. Arrow-key tool/level shortcuts belong to the canvas, leaving toolbar controls their own keyboard navigation.
- Tool and level navigation avoid whole-map validation. Nested-map cycle detection visits each completed dependency only once.
- Resize warnings count every object family, including items, plates, bridges, barriers, and ladders. A 1×1 new map has an in-bounds spawn zone. The stale test expecting no barrier kinds in `obby` now matches its configuration.

## Follow-up changes implemented

1. **Explicit repairs.** [io.py](io.py) expands defaults without removing invalid records. [repairs.py](repairs.py) summarizes proposed repairs; accepting them is one undoable transaction. While a document has pending repairs, ordinary edits preserve records, including through coordinate and level transformations. Autosave and clipboard decoding preserve them too. Invalid ladder sides and nested nudges remain inspectable without breaking canvas interactions.

2. **Safe level insertion.** [structure.py](structure.py) refuses insertion between ramp endpoints unless their removal is explicitly accepted. The canvas highlights affected ramps; removal and insertion share one undo step. Crossing ladders extend to retain their upper landing.

3. **Shared viewport.** [viewport.py](viewport.py) drives painting and picking, pointer-anchored wheel zoom, space/middle-drag pan, Fit Map, and issue focusing. A 256-column map fits an ordinary window. Cell/edge rendering is culled to the visible region; fine hatching and labels are suppressed at overview scales. Canvas letter shortcuts leave searchable fields their text input.

4. **Purpose-specific plates.** Placement accepts distinct purposes on the same cell. Context-menu labels include the kind, and edits and erasures target the full `pressure_plate_key`, leaving other purposes untouched.

5. **Catalogs and mixed materials.** Actor placement has a searchable gameplay-backed picker and rejects unknown kinds. Validation uses the refreshed actor/material catalogs. Material dialogs summarize every selected segment; “Mixed / leave unchanged” preserves per-segment differences when another face is edited. The explicit “Use top-left materials” button fills all six fields from the first spatially selected element, for review before applying. Dialogs are split by concern under [dialogs/](dialogs/).

6. **Document transactions and pure transformations.** [document.py](document.py) owns commands, maintenance, undo, and dirty state; change signals notify the window. [editing.py](editing.py) and [erasing.py](erasing.py) separate map operations from prompts and selection state. [transforms.py](transforms.py) centralizes coordinate families, translation, resizing, and level remapping shared with clipboard operations. The release-tool dispatch table remains explicit.

7. **Longer sessions.** Typed validation issues drive a clickable dock, opened through the toolbar's conditional Issues button, that focuses the level and highlights the object. Tool properties sit inline beside the Tool picker and placement automatically reuses previous values, prompting only when a usable choice is missing. Transient canvas notices provide feedback without a bottom status bar. Untitled maps autosave to session-specific recovery files, with locks preventing recovery from another active editor. File notifications refresh nested shapes and catalogs, including files created later and atomic replacements.

8. **Window and placement behavior.** Window size, position, and maximized state persist in editor-wide preferences; map operations fit the canvas without resizing the window. Qt restores off-screen positions onto an available screen. Ladders, lights, plates, and items preview and commit a single target on release without drawing a range; Escape and off-grid release cancel.

## Verification and performance

The headless suite passes 117 tests, covering the clipboard and persistence workflows plus explicit repairs, document-owned transactions, transforms, ramp insertion, viewport input, independent plate actions, mixed materials, actor choices, issue navigation, session locks, and real dependency-file notifications. Compact toolbar layout, conditional controls, click-through notices, automatic reuse, top-left material copying, window geometry restoration, and single-target placement also have regression tests. Qt's offscreen rendering was visually inspected for tile selection, inline actor settings, canvas notices, and the materials dialog. Native desktop clipboard transfer between two running processes and physical multi-monitor changes were not manually exercised. On macOS, the native file-notification test needs to run outside the Codex sandbox.

All four shipped maps load without automatic repair proposals. A local twenty-iteration average for `hotel` edit maintenance was about 42 ms, including the check that protects pending repairs. This is a helper timing, not an end-to-end frame measurement. Spatial indexes, differential undo commands, and background validation remain profiling-driven options, not required workflow changes.

Run the suite with:

```bash
PYTHONPATH=tools QT_QPA_PLATFORM=offscreen python3 -m unittest discover -s tools/tests -p 'test_*.py'
```
