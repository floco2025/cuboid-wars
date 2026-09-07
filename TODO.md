# Follow-ups

## Enhancements

- **Puzzle design:** See [PUZZLES.md](PUZZLES.md) for the element inventory, puzzle patterns, zapper guards, and [next decisions](PUZZLES.md#next-decisions).

## Testing

- **Projectile pickups:** Check single-shot (one ball) and multishot (three balls) pickups, pickup-only weapon availability, weapon cycling, multishot-first fallback, single-shot pickups preserving multishot selection, and eraser/death reset. Check equal-width HUD power-up slots in single-shot, multishot, portal, speed, low-gravity order, with gaps before missiles and keys. Single-shot is in Hotel’s random pool and Obby’s pickup row. The user handles in-game testing.
- **Window settings:** Check mixed-DPI restoration on macOS and Windows/X11, size/fullscreen restoration with compositor-controlled placement on Wayland, and saving after a drag or an immediate window close. The user handles in-game testing.
- **Energy fields and erasers:** Check zapper beam clipping, blast shielding, projectile absorption, and erasers clearing weapons and power-ups while keeping keys. Eraser entry sounds should play only when equipment is removed. The user handles in-game testing.

- **Playtest Switchyard:** Check the customs portal route and raised seal balcony, moving bridge switch, and ladder shuttle solo and with teammates. The user handles in-game testing.
- **Actor ladders:** Visually check intermediate landings, moving carriers, and mines approaching from opposite sides. The user handles in-game testing.
- **Sliding-carrier pushing:** Let the moving cabin's wall push you while standing still, walking against it, and stepping sideways out of its path. Confirm open space is safe, being pinned against another wall still crushes, and boarding moving platforms remains safe. Crushed actors should play their normal explosion animation and sound.
- **Playtest and refine Relay:** The prototype is playable. The user handles in-game testing; refine the puzzles based on their feedback.
