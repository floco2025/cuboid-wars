# Follow-ups

## Fixes

## Enhancements

- **Per-actor ladder access and navigation:** Make ladder use configurable per actor kind. Disabled actors must not climb incidentally through shared movement physics; enabled actors should deliberately plan and follow routes using ladders, including on moving carriers. Actor navigation already supports ramps, including transitions between levels; preserve that support.
- **Collectible portal guns and equipment erasers:** Make portal guns power-up pickups and configure power-up durations per map, with `0` meaning never expires. Erasers remove all power-ups, keys, missile ammo, and the portal ends controlled by the affected player; other players' portal ends remain. Portal-shot blocking is configurable like other barriers. Portal cleanup on gun expiry or death still needs a decision.

## Testing

- **Sliding-carrier pushing:** Let the moving cabin's wall push you while standing still, walking against it, and stepping sideways out of its path. Confirm open space is safe, being pinned against another wall still crushes, and boarding moving platforms remains safe.
- **Playtest and refine Relay:** The prototype is playable. The user handles in-game testing; refine the puzzles based on their feedback.
