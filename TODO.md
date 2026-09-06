# Follow-ups

## Fixes

- **Push bodies along a sliding carrier before crushing:** A carried wall or raised slab moving horizontally into a body that is not riding it is not pushed along; it is crushed once the penetration exceeds the contact offset, which at slider speed is one tick (`CollisionWorld::character_crushed`). Treating an overlapping carrier as the carry, moved through the character controller so static walls still block, and crushing only then would give players a chance to step aside.

## Enhancements

- **Per-actor ladder access and navigation:** Make ladder use configurable per actor kind. Disabled actors must not climb incidentally through shared movement physics; enabled actors should deliberately plan and follow routes using ladders, including on moving carriers. Actor navigation already supports ramps, including transitions between levels; preserve that support.
- **Collectible portal guns and equipment erasers:** Make portal guns power-up pickups and configure power-up durations per map, with `0` meaning never expires. Erasers remove all power-ups, keys, missile ammo, and the portal ends controlled by the affected player; other players' portal ends remain. Portal-shot blocking is configurable like other barriers. Portal cleanup on gun expiry or death still needs a decision.
- **Generic puzzle-mechanic tests:** Extract useful coverage from `server/src/map/relay_tests.rs` into generic behavior tests with tiny inline maps. Remove redundant Relay-specific assertions and retain general map validation. Do not introduce a per-map testing framework.

## Testing

- **Nested-map missile guidance:** Fire at cabin actors from outside in obby, both while the cabin rests and during travel, from several angles. Routes use each map's own airspace and current collision geometry; blocked routes replan, and steering checks turns, lead, and weave. Confirm missiles navigate openings and interior walls without clipping them.
- **Playtest and refine Relay:** The prototype is playable. The user handles in-game testing; refine the puzzles based on their feedback.
