# CMU motion captures

Source: [Carnegie Mellon University Graphics Lab Motion Capture Database](http://mocap.cs.cmu.edu/), downloaded September 8, 2026. The database was created with funding from NSF EIA-0196217.

CMU permits use of these captures, including incorporation in commercial products. Selling the capture data itself, including format conversions, is prohibited. These files retain CMU's terms rather than this repository's source-code or general asset license. See the [database's usage terms](http://mocap.cs.cmu.edu/).

| Capture | Use | Source |
| --- | --- | --- |
| `16_15.amc` | Walk | [AMC](http://mocap.cs.cmu.edu/subjects/16/16_15.amc) |
| `16_35.amc` | Run | [AMC](http://mocap.cs.cmu.edu/subjects/16/16_35.amc) |
| `16_01.amc` | Idle | [AMC](http://mocap.cs.cmu.edu/subjects/16/16_01.amc) |
| `143_05.amc` | Jump, descent and landing from a distance jump | [AMC](http://mocap.cs.cmu.edu/subjects/143/143_05.amc) |
| `13_33.amc` | Ladder climb | [AMC](http://mocap.cs.cmu.edu/subjects/13/13_33.amc) |
| `143_40.amc` | Sideways walking; mirrored for the opposite direction | [AMC](http://mocap.cs.cmu.edu/subjects/143/143_40.amc) |

Matching skeletons: [16.asf](http://mocap.cs.cmu.edu/subjects/16/16.asf), [13.asf](http://mocap.cs.cmu.edu/subjects/13/13.asf), [143.asf](http://mocap.cs.cmu.edu/subjects/143/143.asf).

`../../player_robot_mocap.py` selects frame ranges, retargets the 120 Hz recordings to the robot's proportions, centres lateral posture while retaining sway, removes travel, and closes looping clips. Ladder hand heights follow the recording with arm IK to reach vertical rungs. Idle, jump and descent are retimed excerpts; descent reaches a landing-ready pose and holds during longer falls. Finger poses and the stun reaction are authored, not motion capture.

Rebuild from the repository root with:

```sh
/opt/homebrew/bin/blender --background --python client/assets/models/player_robot.py
```

The resulting `player_robot.glb` contains the baked animations and embedded textures. The game does not load these source captures or run the retargeter. Add `-- --preview` for textured stills in `/tmp`; `player_robot_preview.py` renders a 24 fps motion study to `/tmp/player-robot-mocap-preview/`, jump phases with `-- --jump`, or a hand close-up with `-- --hands`.
