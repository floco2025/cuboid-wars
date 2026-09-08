# Asset provenance

Inventory checked September 8, 2026 against the repository, model generators, embedded GLB images, and Marc's local collection at `~/projects/assets/`. Paths below are relative to this directory unless stated otherwise. Local collection paths are source evidence, not runtime dependencies.

**Created here** means authored for Cuboid Wars. **Third-party** means supplied externally. **Modified third-party** includes conversions, packed texture channels, and retargeted animation. These describe provenance, not a transfer of ownership or a license grant. Project-created content follows the [asset notice](README.md); external content retains its own terms, including when embedded in a model.

When adding or replacing an asset, update its entry with the author, source URL or local source, license, generator, and external dependencies. Keep supplied license files beside the relevant assets. Record unknown details explicitly; filenames and a download being free do not establish a license. This register covers current files; removed assets remain in Git history.

## Models and animations

All current model geometry and rigs were created here through the following Blender scripts. None of these GLBs imports a downloaded example mesh. Their embedded texture images remain third-party derivatives. Shared texture preparation is in [model_materials.py](models/model_materials.py).

| Model | Generator | Embedded third-party dependencies |
| --- | --- | --- |
| `models/player.glb` | [player.py](models/player.py) | FreePBR scuffed plastic, brushed metal, synthetic rubber; retargeted CMU motion capture |
| `models/scuttler.glb` | [scuttler.py](models/scuttler.py) | FreePBR scuffed plastic, brushed metal, synthetic rubber |
| `models/bruiser.glb` | [bruiser.py](models/bruiser.py) | FreePBR brushed metal and synthetic rubber |
| `models/zapper.glb` | [zapper.py](models/zapper.py) | FreePBR scuffed plastic and brushed metal |
| `models/turret.glb` | [turret.py](models/turret.py) | FreePBR used stainless steel albedo and roughness |
| `models/wall_light_decorative.glb` | [wall_lights.py](models/wall_lights.py) | FreePBR brushed metal |
| `models/wall_light_utility.glb` | [wall_lights.py](models/wall_lights.py) | FreePBR brushed metal |

Actor animation and articulation are authored here. Player locomotion is modified third-party motion capture; finger poses and the stun reaction are authored here. The CMU source files (`models/mocap/cmu/*.amc` and `*.asf`), capture URLs, uses, attribution, and usage terms are listed in the [CMU notice](models/mocap/cmu/README.md). Author: Carnegie Mellon University Graphics Lab. The notice records permission to incorporate motion in commercial products but prohibits selling the capture data itself, including conversions.

[player_mocap.py](models/player_mocap.py) performs retargeting; [player_preview.py](models/player_preview.py) renders animation previews. Player material adjustments live in [player.materials.json](models/player.materials.json), documented in [player.materials.md](models/player.materials.md). These scripts and settings were created here. Generators embed resized, channel-converted and, where configured, tuned texture derivatives; embedding does not make the texture source project-created.

## Texture sets

Author/source: **Brian, FreePBR.com**, matched to `~/projects/assets/freepbr.com/`. All 19 current texture folders are covered below. Each entry includes every PNG in that folder, including the project's packed metallic/roughness derivatives.

License: FreePBR's own terms, not CC0. The [publisher's terms](https://freepbr.com/about-free-pbr/), checked September 8, 2026, allow free noncommercial use, require paid access for commercial use, and restrict redistribution of the texture sets. Marc's acquired license/access is **awaiting confirmation**; no purchase or download-time license record was found in the local collection. Current website terms do not establish which terms accompanied a past download.

| Folder under `textures/` | Source set | Additional use in models |
| --- | --- | --- |
| `art-deco-scales-wallpaper-ue/` | Art Deco scales wallpaper | — |
| `beige-carpet-worn1-ue/` | Beige carpet worn 1 | — |
| `bricks-mortar-ue/` | Bricks mortar | — |
| `brushed-metal-ue/` | Brushed metal | Player, scuttler, bruiser, zapper, both wall lights |
| `cheap-old-linoleum-ue/` | Cheap old linoleum | — |
| `damp-block-wall-ue/` | Damp block wall | — |
| `fiberous-plaster1-ue/` | Fiberous plaster 1 | — |
| `forest-wallpaper-ue/` | Forest wallpaper | — |
| `modern-brick1_ue/` | Modern brick 1 | — |
| `mud-with-vegetation-ue/` | Mud with vegetation | — |
| `patched-brickwork-ue/` | Patched brickwork | — |
| `rectangle-polished-tile-ue/` | Rectangle polished tile | — |
| `scuffed-plastic-1-Unreal-Engine/` | Scuffed plastic 1 | Player, scuttler, zapper |
| `smooth-temple-blocks-ue/` | Smooth temple blocks | — |
| `steelplate1-ue/` | Steel plate 1 | — |
| `stucco1_ue/` | Stucco 1 | — |
| `synth-rubber-unreal-engine/` | Synthetic rubber | Player, scuttler, bruiser |
| `used-stainless-steel-ue/` | Used stainless steel | Turret |
| `worn-walkway-metal-ue/` | Worn walkway metal | — |

Source color, normal and AO files were compared by SHA-256 with the local collection. Most match exactly. Carpet and linoleum albedo/normal files, modern brick AO, and stainless steel normal differ; their precise edits are not recorded. `synth-rubber-ao.png` has no same-name source in the collection, so its individual derivation is unconfirmed. The other synthetic rubber source images match.

[combine_metallic_roughness.sh](textures/combine_metallic_roughness.sh) is the project's channel-packing helper: green = roughness, blue = metallic, red unused. [multiply_intensity.sh](textures/multiply_intensity.sh) is the project's intensity-adjustment helper. Packed maps are modified third-party assets, not independent original textures. The existence of these helpers does not establish the exact processing history of every image.

## Sound effects

| Files under `sounds/` | Origin and author | Source / evidence | License |
| --- | --- | --- | --- |
| `Retro Charge StereoUP 12.wav`, `Retro Event Acute 08.wav`, `Retro Explosion Short 15.wav`, `Retro Impact Punch 07.wav`, `Retro Missile Launcher 01.wav`, `Retro Negative Short 23.wav`, `Retro PowerUP StereoUP 05.wav`, `Retro Turn Off 12.wav` | Third-party; Kronbits | [FreeSFX](https://kronbits.itch.io/freesfx); exact matches in `~/projects/assets/FreeSFX/GameSFX/` | CC0 1.0, as stated on the publisher's page; attribution optional |
| `impactGeneric_light_001.ogg`, `impactGlass_heavy_001.ogg`, `impactGlass_light_000.ogg`, `impactMetal_medium_003.ogg`, `impactPlank_medium_001.ogg`, `impactPunch_heavy_001.ogg`, `impactTin_medium_004.ogg` | Third-party; Kenney | [Impact Sounds](https://kenney.nl/assets/impact-sounds); exact matches in `~/projects/assets/kenney_impact-sounds/Audio/` | CC0; [supplied notice](sounds/Kenney-Impact-Sounds-LICENSE.txt) |
| `doorClose_2.ogg`, `doorOpen_1.ogg`, `metalClick.ogg`, `metalLatch.ogg` | Third-party; Kenney Vleugels | [RPG Audio](https://kenney.nl/assets/rpg-audio); exact matches in `~/projects/assets/kenney_rpg-audio/Audio/` | CC0; [supplied notice](sounds/Kenney-RPG-Audio-LICENSE.txt) |
| `freesound_community-laser-beam-76426.wav` | Modified third-party; peepholecircus (Freesound), distributed by Pixabay's freesound_community | [Laser beam](https://pixabay.com/sound-effects/film-special-effects-laser-beam-76426/); matching named MP3 in `~/projects/assets/pixabay.com/`, converted to WAV | Pixabay Content License, as listed on the asset page |
| `rain-on-umbrella-loop.wav` | Modified third-party; Vadim_Makes_Sound (identified by local source filename) | Local source: `~/projects/assets/pixabay.com/vadim_makes_sound-rain-on-umbrella-loop-gentle-rain-ambience-550562.mp3`; WAV conversion | Pixabay source identified; individual download/license record not retained, asset page verification pending |
| `portal-fizzle.wav` | Created here; synthesized for Cuboid Wars | [portal-fizzle.py](sounds/portal-fizzle.py); seeded filtered noise and a descending synthesized buzz, with no recordings or external inputs | Project asset notice |

The [Pixabay Content License summary](https://pixabay.com/service/license-summary/) permits adaptation and use without mandatory attribution, subject to its restrictions, including on standalone redistribution. The [full terms](https://pixabay.com/service/terms/) govern; this register does not relicense the recordings.

macOS download metadata on both source MP3s confirms Pixabay CDN origins. Regenerate the portal fizzle with `python3 client/assets/sounds/portal-fizzle.py` from the repository root; it uses only Python's standard library and produces a deterministic 0.55-second mono 44.1 kHz, 16-bit PCM WAV. Duration, peak level and random seed are tunable in the generator.

## Skybox, screenshots and symbols

| Files | Origin / author | Source and processing | License / dependencies |
| --- | --- | --- | --- |
| `Skybox_CoudySky_Day_3.png` | Modified third-party; Infinity Skyboxes Pack 2, publisher confirmation pending | Source in `~/projects/assets/fab.com/InfinitySkyboxesPack2_Universal/Textures/SkyClouds/`; source is 8192×4096, project copy is 4096×3072, converted for the skybox layout | Acquired Fab license and exact listing **awaiting confirmation**; no license file found in the local pack |
| `screenshot1.png`, `screenshot2.png`, `screenshot3.png`, `screenshot4.png`, `screenshot5.png` | Created here; Cuboid Wars gameplay captures | Repository README screenshots, recorded in project history; no generator | Project asset notice; depicted third-party art retains its own terms. Historical models shown in screenshots may differ from current GLBs |
| `symbols/items.json` | Created here; Cuboid Wars pickup and HUD outlines | Authored shape data consumed by `client/src/items/symbols.rs` and the map editor | Project asset notice; no external dependency identified |

Runtime-generated meshes and effects (pickups, missiles, particles and map geometry) live in the source code rather than binary assets. Their use of catalog textures is covered by the texture entries above. Documentation, license notices and generation scripts are support files, not additional imported art packs. Local caches and editor metadata are outside this inventory.

## Information to complete

- Record FreePBR commercial access and the applicable download/purchase terms.
- Record the skybox's exact Fab listing, publisher and acquired license.
- Verify the rain recording's individual listing/license record and the derivation of synthetic rubber AO.
