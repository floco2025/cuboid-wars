# Project review

Reviewed on 2026-09-18 against `2b4977b0` (`Standardize JSON absence and add always-active power-ups`). The checkout was refreshed during the review, and the Rust and editor checks were repeated against that revision.

This review covers correctness, structure, simplification, maintainability, assets, tooling, and selected runtime behavior. The initial review produced this report and the follow-ups in [TODO.md](TODO.md), without changing production behavior. All five defects (R1 and R5–R8) have since been fixed, and R2–R4 have been reclassified as accepted behavior, as noted below. The evidence and validation table describe the original review baseline. Temporary diagnostic harnesses and captures remain outside the repository.

## Assessment

Keep the executable plus `common`, `server`, and `client` workspace structure. The shared motor, separate owner/server policies, direct network dispatch, and domain-owned resources have clear responsibilities. A broad rewrite or further crate split would add migration work without addressing the defects found here.

The established correctness gaps concern render-frame input versus fixed simulation, pickup eligibility after earlier pickups, and validation versus narrower runtime representations. Temporary inconsistencies between network cues and snapshots are accepted by the [self-repairing protocol contract](common/src/protocol.rs); this review did not establish failure to converge or a gameplay problem beyond that trade-off. Existing unit coverage is substantial, but successful isolated tests do not exercise all of those interactions.

Eight numbered observations follow: five defects and three accepted behaviors (R2–R4, whose original P2 classifications and fix recommendations are withdrawn). P2 means ordinary corrective work; P3 means a low-impact tooling or configuration edge case. No P0 or P1 issue was established. Existing portal traversal and client-memory follow-ups remain relevant.

## Findings

### R1 — P2: fixed simulation consumes movement input from the previous frame

**Status: implemented.** The movement/view input chain now runs in `PreUpdate`, after focus handling and console/menu decisions. Cursor capture, camera toggles, zoom, mouse orientation, and movement retain their dependencies. The existing jump velocity preserves a jump through a frame without fixed steps, so no additional input queue was needed. Regression tests cover release during catch-up, current mouse direction and facing lock, overlay gating, and one jump across zero/multiple-step frames.

**Locations:** [input registration](client/src/input/plugin.rs), [fixed simulation](client/src/characters/plugin.rs), [movement input](client/src/input/movement.rs).

`input_movement_system` updates intent in `Update`, after the frame's `FixedUpdate` iterations. Focus loss is cleared in `PreUpdate`, but ordinary key releases are not translated into movement intent there. A frame with several catch-up steps therefore keeps walking with the previous intent before processing the release.

**Evidence:** a temporary Bevy harness used the real movement-input system, a 30 Hz fixed clock, and a fixed-step intent recorder. It pressed W for one frame, released W before the next update, and advanced that frame by 250 ms. All seven fixed steps still observed `Walking`; the subsequent input update cleared it. This establishes a scheduling delay, not an explanation for arbitrary multi-second movement after release.

**Change:** sample keyboard movement and latch one-shot actions before fixed simulation, after Bevy input processing and applicable focus/menu gating. Preserve the camera and mouse dependencies of the existing input path; moving the entire mixed-purpose system blindly would introduce different ordering problems.

**Verification:** release during a multi-step frame, press/release across a frame with no fixed step, jump consumed once across multiple fixed steps, and focus/menu transitions. Confirm facing and camera-relative movement still use the intended orientation.

### R2 — Accepted behavior: temporary inventory inconsistency within the same body

**Status: accepted trade-off; fix recommendation withdrawn.** The self-repairing protocol permits temporary state changes across cues and snapshots. Additional status revisions or missile acknowledgements are not required merely to eliminate this interval.

**Locations:** [`SPlayerStatus`](common/src/protocol.rs), [status handler](client/src/network/players/handlers.rs), [`PlayerInfo::apply_status` and `apply_snapshot`](client/src/players/resources.rs).

Status cues carry a body generation but no state revision or tick. Both cues and snapshots replace power-ups, stun, keys, and missile counts unconditionally. The body-generation guard correctly rejects another life, but cannot reject older state within the current life. The snapshot guard only orders snapshots against snapshots.

For example, deliver a snapshot showing zero missiles after erasure, then an older pickup status showing three. The older status restores three locally until another fresh snapshot arrives. Conversely, a recent pickup cue can be overwritten by an earlier snapshot that is newer than the last snapshot already received. Equipment can affect owner simulation during that interval; the contract accepts temporary inconsistencies and does not promise to undo every action taken before repair.

**Evidence:** the real client `apply_status` method restored the missile count from zero to three in a temporary harness. Source inspection confirmed that the normal message handler adds no freshness check beyond generation. This was a state-application probe, not a captured packet-order reproduction.

### R3 — Accepted behavior: temporary portal placement inconsistency

**Status: accepted trade-off; fix recommendation withdrawn.** Portal state is repaired by subsequent fresh snapshots. This observation alone does not justify placement revisions or removal tracking.

**Locations:** [`SPortalOpened`](common/src/protocol.rs), [portal cue handler](client/src/network/portals/handlers.rs), [portal snapshot reconciliation](client/src/network/portals/sync.rs).

The cue and snapshot paths share an equality-based upsert, but equality only deduplicates the same placement. `SPortalOpened` has no revision, and the handler installs any different placement. Snapshots also replace or remove portal ends without comparing against the cue that last changed them.

Two legal delivery sequences produce a temporary inconsistency: receive a new placement and then an earlier, globally acceptable snapshot; or receive a newer snapshot that removes/moves an end and then a delayed old placement cue. In either case the client restores older portal state. Both paths rebuild the `PortalSet` used for local traversal, so a temporary placement can influence movement before a later snapshot repairs portal state. No gameplay problem beyond the accepted trade-off was established.

**Evidence:** traced the complete cue/upsert/snapshot paths against the explicitly unordered delivery contract in the protocol header. No controlled visual packet-reordering reproduction was performed.

### R4 — Accepted behavior: temporary actor reappearance after a death cue

**Status: accepted trade-off; fix recommendation withdrawn.** A temporary reappearance repaired by a fresh snapshot is permitted. Additional actor retirement tracking is not required merely to prevent it.

**Locations:** [actor death handler](client/src/network/actors/handlers.rs), [actor snapshot reconciliation](client/src/network/actors/sync.rs).

The death handler removes the actor from `ActorMap`. A subsequent snapshot creates every listed actor whose ID is absent, without remembering the death. If the last accepted snapshot is tick 10, an actor dies at tick 12, and its death cue arrives before the tick-11 snapshot, that snapshot is globally fresh and recreates the dead actor. The next post-death snapshot removes it again.

**Evidence:** source-path analysis of death removal, the snapshot tick guard, and missing-actor creation. Existing player-generation and missile lifecycle guarantees remain applicable to those domains; they do not establish a general requirement to prevent every temporary actor reappearance. This sequence was not exercised through rendered network clients.

### R5 — P2: simultaneous pickups consume items that no longer have an effect

**Status: implemented.** The collection loop now chooses an eligible recipient from current health and equipment immediately before each pickup. An item made redundant by an earlier pickup remains available or goes to another eligible overlapping player. Regression tests cover potions, packs, keys, permanent power-ups, placed/random items, multiple players, and successive pickups that remain useful.

**Location:** [`item_collection_system`](server/src/items/collection.rs).

The overlap pass builds the entire collection batch against the player's initial health and equipment. The application pass rechecks permanent power-ups only, then consumes each queued item. An earlier potion, missile pack, or key can make the next queued pickup useless, but the second item is still consumed.

**Evidence:** a temporary harness ran the real collection system with two pickups at x = −0.6 m and +0.6 m around one player. With health one below maximum, missiles one below capacity, or two keys of the same previously unheld kind, both placed items became hidden although only the first changed state. The permanent-power-up recheck shows that the intended behavior is already recognized for one item family.

**Change:** re-evaluate the existing `pickup_has_effect` rule against current health and equipment immediately before consumption. Keep the eligibility policy in one function. If another overlapping player can use an item, avoid allowing an obsolete reservation to consume it or prevent later collection.

**Verification:** each of the three reproduced item kinds, an item that remains useful after the first pickup, permanent versus timed power-ups, and two overlapping players with different inventories.

### R6 — P2: a validated 256-level map exceeds the bootstrap representation

**Status: implemented.** Loading rejects more than 255 levels in the root and every named geometry, including unplaced definitions, before compilation. The editor reports the same limit with the affected geometry's name and preserves the authored levels. Tests cover compilation and bootstrap count conversion at 255 levels, the highest valid storey and nested placement endpoint, and rejection of 256/257 levels in root, placed, and unplaced geometry.

**Locations:** [map validation](server/src/map/definition/validation.rs), [level conversion](server/src/map/definition/geometry.rs), [bootstrap construction](server/src/app.rs).

Map validation requires at least one level but does not enforce the runtime count limit. Bootstrap construction converts `grid.levels.len()` to `u8` with `expect("map level count exceeds u8")`. A 256-level grid therefore gets through map compilation and cannot be represented at startup. Counts and zero-based level indexes have different upper bounds; allowing an index of 255 does not make a count of 256 fit in `u8`.

**Evidence:** a temporary 1×1 map with 256 levels, a valid floor, and a start checkpoint compiled successfully through `generate_map`. Converting its compiled level count exactly as bootstrap construction does failed. The harness did not launch a separate game process with that map; the panic follows from the inspected `expect` path. Larger maps also encounter saturating level-tag conversions, making early rejection preferable.

**Change:** establish one supported bound for every root and nested grid. Under the current wire representation, reject more than 255 levels with a useful map-validation error and mirror the constraint in editor diagnostics. Supporting 256 or more instead requires widening and auditing the count representation.

**Verification:** the maximum supported count, the first unsupported count, nested definitions and placement reach, and an error returned before bootstrap construction or geometry truncation.

### R7 — P3: audio freshness checking fails on insignificant numeric differences

**Status: implemented.** The check uses absolute tolerances of 0.0001 dB for measured levels/gain and 0.000001 seconds for duration, below one analysis sample frame. Hashes, file membership, settings, schema, and discrete metadata remain exact; nulls stay distinct from missing fields and zero. Failures identify the affected file/field and differing values. All 10 audio tests passed, and `--check` passed for all 57 audio files without rewriting the saved analysis.

**Location:** [`analyze_audio.py --check`](client/assets/sounds/analyze_audio.py).

The check compares the entire regenerated JSON value using exact equality. On this machine it reports stale analysis even though every audio hash and all nonnumeric metadata match. There are 33 changed numeric fields across 28 files, with a maximum difference of approximately 0.000002 dB. The observed discrepancy is numeric reproducibility, not changed source audio or a meaningful normalization change; its precise decoder/toolchain cause was not isolated.

**Change:** compare hashes, file membership, schema, and discrete metadata exactly, but compare measured floating-point fields using explicitly small, field-appropriate tolerances. For dB values, 0.0001 dB would comfortably cover the observed noise. Keep missing/null values meaningful and still fail material gain changes. Report the differing files/fields instead of only saying the analysis is stale.

**Verification:** harmless last-decimal differences pass; changed audio hashes, added/removed files, changed analysis settings, and meaningful measurement differences fail. Regenerating the catalog alone would hide this instance without making the check reproducible elsewhere.

### R8 — P3: accepted server rates can produce a zero-duration tick

**Status: implemented.** Shared network validation requires a positive rate whose derived tick lasts at least one nanosecond, rejecting rates above 1,000,000,000 Hz. The same check covers loaded settings, CLI overrides, and client bootstrap. Tests cover zero, ordinary rates, the maximum representable rate, the next rate, `u32::MAX`, and rejection of invalid overrides before server app creation; existing update/snapshot rate checks remain in place.

**Location:** [`NetworkConfig::validate` and `tick_duration`](common/src/config/network.rs).

The validator checks positivity and the relative send rates. `tick_duration` computes integer nanoseconds as `1_000_000_000 / server_hz`; accepted rates greater than one billion produce zero. The CLI accepts the same positive `u32` range.

**Evidence:** `server_hz = u32::MAX`, `update_hz = 30`, and `snapshot_hz = 4` passed validation and yielded `0ns` in a temporary harness. Ordinary shipped rates are unaffected. No zero-duration server was launched.

**Change:** reject unsupported rates at the shared configuration boundary, ensuring the derived duration is nonzero. A documented practical maximum is also reasonable if the project wants one.

**Verification:** zero, the chosen maximum, the next value, `u32::MAX`, normal rates, and movement/snapshot rates above the server rate.

## Structure, simplification, and cleanup

| Area | Assessment and next step |
| --- | --- |
| Workspace boundaries | Keep the current executable and three libraries. Shared geometry/motor code belongs in `common`; client-owned flight and server-owned outcomes have different policies and should remain separate. |
| Network state | Preserve the [self-repairing protocol contract](common/src/protocol.rs): temporary inconsistencies are accepted. Test eventual convergence and existing explicit guarantees; additional ordering machinery needs evidence of a problem beyond that trade-off. |
| Input and scheduling | Resolve R1 by separating input sampling from camera/presentation work. Keep the fixed-step sequence visible in one registration point so its tick, carrier, movement, transit, and reporting dependencies remain reviewable. |
| Pickup handling | Generalize the existing eligibility predicate at the consumption boundary instead of growing one special-case recheck per item kind. |
| Map editor and server | Start the existing shared-map-core investigation with a small contract corpus run through both implementations: absence/null rules, nesting, transformations, validation failures, and level limits. Preserve invalid authored records and structured diagnostics. Extract pure rules incrementally before considering a Python binding or an editor rewrite. |
| Dependencies | Consider `[workspace.dependencies]` for versions repeated across the four manifests, especially Bevy, serde, bincode, renet, and Rapier. Preserve each consumer's feature selections, including disabled defaults. This is maintenance cleanup, not a recommendation to upgrade versions. |
| Editor undo | Whole-document snapshots are simple and capped. Measure transaction latency and retained memory on large nested documents before replacing them with patch-based history; no editor memory defect was established here. |
| Client memory | The existing texture-loading/allocator follow-up remains the useful target. Measure peak and settled memory while experimenting with bounded decode/mipmap batches; avoid adding unrelated caches. This review did not reproduce the earlier allocator live-set experiment. |
| CI | Current CI covers Rust formatting, Clippy, Rust tests, and editor tests. Add the existing audio unit suite, the audio freshness check (now reproducible after R7), and an exact-case asset-path check. The Linux input-helper parser test can run without Qt or a desktop. |
| Documentation | Keep README player-facing. Keep implementation ownership in AGENTS and outstanding work in TODO. This report records a dated baseline and should not become another architecture specification. |

No dependency vulnerability audit or recommendation to expose the server publicly is implied. Cooperative private/LAN clients remain the documented threat model; intentionally excluded abuse-hardening work is not listed as a new defect. Wire/config versioning is likewise not recommended merely for a pre-release project.

## Validation and coverage

### Automated checks

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed after the pull. |
| `cargo clippy --release --workspace --all-targets -- -D warnings` | Passed after the pull. |
| `cargo test --release --workspace` | Passed after the pull: 520 client, 350 common, 10 executable, and 721 server tests; 1,601 total. |
| Editor `unittest` discovery with `PYTHONPATH=tools` and `QT_QPA_PLATFORM=offscreen` | 441 tests passed after the pull. |
| Audio analysis unit tests | 3 tests passed. |
| `python3 client/assets/sounds/analyze_audio.py --check` | Failed; isolated to the numeric differences in R7. |
| Linux input-helper parser with UndefinedBehaviorSanitizer | Built and passed. |
| Asset-catalog file existence on case-sensitive Linux | All 154 references resolved to 143 unique files, after separating glTF subasset fragments such as `#Scene0`. This checks files, not every subasset's content. |
| Temporary focused Rust probes | Five probes reproduced the behavior described in R1, R2, R5, R6, and R8. R2 is accepted behavior; the others confirmed defects. These are diagnostic probes, not regression tests asserting a fix. |
| Temporary local network integration | Two cases passed using the real server app and client link: dedicated Obby with 80 ms one-way delay, 0.5 jitter, and 5% unreliable loss; hosted Hotel with a local queue client plus a remote link. Login, repeated snapshots/pongs, and remote disconnect cleanup succeeded. |

An initial sandboxed Rust run failed its UDP loopback test because socket creation was denied. The full run with socket access passed; the initial failure is an environment limitation, not a game defect. The temporary networking checks used library apps and links, not multiple rendered processes. They did not specifically verify convergence after the deliberately reordered sequences in R2–R4; those temporary intermediate inconsistencies are accepted.

### Runtime observations

Linux KDE Plasma Wayland, Intel Core i9-14900KF, NVIDIA RTX 4090/Vulkan. Both Hotel and Obby were launched from the refreshed checkout using `cargo run --release`, with session-only window/volume overrides and the neutral name `Reviewer`. Each review instance was stopped through its launching process.

Hotel's exterior baseline, settings panel, and changed movement/camera view rendered successfully. A settled stationary recording showed no obvious visual defect in the inspected frames. With VSync disabled, the deferred renderer, FOV 90, a 1280×720 logical window and 1920×1080 render resolution, three HUD samples showed 210, 211, and 219 FPS. The stored MSAA preference was 4×, but deferred camera setup disables MSAA. These are HUD readings on one high-end machine, not frame-time percentiles or a hardware-independent performance claim. Settled Hotel RSS was 1,938,472 KiB, approximately 1.85 GiB, consistent with the existing memory follow-up.

Obby's initial scene rendered its platforms, fields, pickups, terrain, and HUD without an obvious missing-asset failure; its captured HUD showed 264 FPS. This was a startup visual smoke check, not a traversal of the whole course. Input injection succeeded, but unrecorded transient jump/fire actions are not counted as visually verified gameplay. Audio was muted, so playback quality, spatial occlusion, and perceived loudness were not reviewed by listening.

Both launches logged a Wayland cursor-position error at startup; later focused input worked. It merits reproduction alongside focus/recapture testing before changing cursor handling. No crash was observed.

### Inspection scope and limits

| Area | Coverage |
| --- | --- |
| Executable and lifecycle | CLI, embedded host, app construction, startup configuration, and shutdown paths inspected; single-player render launches plus local hosted/dedicated link checks. |
| Common code | Protocol, transport definition, clock/cadence, representative configuration validation, carrier/portal geometry, and shared character movement inspected; full common suite run. |
| Server | Scheduling, ingress/dispatch, item collection, player lifecycle/checkpoints, representative combat/quest paths, actor movement/navigation, and map compilation/validation inspected; full server suite run. |
| Client | Input/fixed schedule, snapshot and domain handlers, interpolation, portal rendering, camera/focus, audio/settings, and representative rendering/asset paths inspected; full client suite run and two rendered smoke checks. |
| Editor | Document persistence/recovery, undo transactions, normalization/validation, transforms, level editing, dependency reload, and selection operations sampled; full headless suite run. No complete interactive editor session. |
| Assets and tooling | Catalog paths and audio measurements checked; existing model compatibility tests run as part of the client suite; Linux helper built/tested. Blender assets were not regenerated. |
| Not exercised end to end | macOS/Windows, WAN sessions, long multiplayer soak/load, full Obby completion, all portal orientations/carrier crossings, day/night/rain under both renderers, audio listening, and crash recovery under real OS failure. |

The review inventories the repository and checks the areas above; it is not a claim that every source line, asset, platform, or gameplay combination was independently verified. Remaining high-value verification belongs in TODO rather than being silently treated as passed.

## Existing follow-ups and implementation order

The floor-portal walking failure and memory work remain in Fixes. Pressure-plate support geometry, stairs rendering, Obby speed tuning, and the Rapier workaround review remain Enhancements. Sliding-carrier pushing remains Testing.

Two portal-visual entries described mechanisms already present in this revision: `portal_body_clipping_system` preserves a mapped pose during handoff, and `straddled_gate` uses rendered carrier frames. The tests `a_floor_handoff_starts_the_body_inverted_about_its_centre` and `a_carried_gate_is_straddled_where_it_is_drawn` pass. TODO now asks for integrated visual verification, including fast crossings and frame stalls, instead of requesting those mechanisms again. This does not assert that every remaining visual symptom is resolved.

R1 and R5–R8 have been implemented with focused behavioral regressions and removed from TODO. R2–R4 are accepted behavior and have also been removed from Fixes; network testing now targets convergence and existing explicit guarantees. The next review follow-ups are CI coverage for the existing tools, including audio freshness checking, and dependency centralization. Keep any shared-map extraction separate so its effects remain easy to assess.

Follow-up validation: all 1,608 release workspace tests passed (524 client, 350 common, 10 executable, 724 server), including seven added regressions. Clippy passed with warnings treated as errors. The input tests use the production movement-input registration, with a fixed-step recorder and overlay-state transitions; they are headless checks, not a new rendered playtest.

Validation-boundary follow-up: all 1,612 release workspace tests and 443 editor tests passed, including four new Rust regressions and two editor regressions for R6/R8.
