"""Read CMU Acclaim captures and retarget their joint rotations to the player rig."""

import math
from pathlib import Path

import bpy
import numpy as np
from mathutils import Euler, Matrix, Quaternion, Vector

DATA = Path(__file__).resolve().parent / "mocap" / "cmu"
SOURCE_FPS = 120
UP = Vector((0, 0, 1))
CONVERT = Matrix.Rotation(math.pi / 2, 3, "X")


def rotation(values):
    return Euler(tuple(math.radians(v) for v in values), "XYZ").to_matrix()


class Capture:
    def __init__(self, name):
        lines = (DATA / (name.split("_")[0] + ".asf")).read_text().splitlines()
        self.bones = {}
        self.parents = {}
        section = None
        for line in lines:
            words = line.split()
            if not words:
                continue
            key, *values = words
            if key.startswith(":"):
                section = key
            elif section == ":bonedata":
                if key == "begin":
                    bone = {"dof": []}
                elif key == "end":
                    self.bones[bone["name"]] = bone
                elif key == "name":
                    bone[key] = values[0]
                elif key == "direction":
                    bone[key] = Vector(tuple(map(float, values)))
                elif key == "axis":
                    bone[key] = rotation(list(map(float, values[:3])))
                elif key == "length":
                    bone[key] = float(values[0])
                elif key == "dof":
                    bone[key] = values
            elif section == ":hierarchy" and key not in ("begin", "end"):
                for child in values:
                    self.parents[child] = key
        self.frames = []
        for line in (DATA / (name + ".amc")).read_text().splitlines():
            words = line.split()
            if not words or words[0][0] in "#:!":
                continue
            if words[0].isdigit():
                self.frames.append({})
            else:
                self.frames[-1][words[0]] = list(map(float, words[1:]))
        self.positions = []
        self.rotations = []
        for frame in self.frames:
            root = frame["root"]
            positions = {"root": Vector(root[:3])}
            rotations = {"root": rotation(root[3:])}

            for bone_name in self.bones:
                self.solve(bone_name, frame, positions, rotations)
            self.positions.append(positions)
            self.rotations.append(rotations)

    def solve(self, name, frame, positions, rotations):
        if name in rotations:
            return
        parent = self.parents[name]
        self.solve(parent, frame, positions, rotations)
        bone = self.bones[name]
        angles = [0, 0, 0]
        for channel, value in zip(bone["dof"], frame.get(name, [])):
            angles["xyz".index(channel[1])] = value
        basis = bone["axis"]
        rotations[name] = rotations[parent] @ basis @ rotation(angles) @ basis.transposed()
        positions[name] = positions[parent] + rotations[name] @ (bone["direction"] * bone["length"])

    def sample(self, frame):
        index = min(int(frame), len(self.frames) - 2)
        blend = frame - index
        positions = {
            name: pos.lerp(self.positions[index + 1][name], blend) for name, pos in self.positions[index].items()
        }
        rotations = {
            name: rot.to_quaternion().slerp(self.rotations[index + 1][name].to_quaternion(), blend)
            for name, rot in self.rotations[index].items()
        }
        return positions, rotations


TAKES = {
    "Idle": ("16_01", 0, 48, True),
    "Walk": ("16_15", 60, 197, True),
    "Run": ("16_35", 58, 154, True),
    "Climb": ("13_33", 264, 552, True),
    "Jump": ("143_05", 214, 230, False),
    "Fall": ("143_05", 230, 244, False),
    "Land": ("143_05", 244, 279, False),
    "StrafeLeft": ("143_40", 663, 823, True),
    "StrafeRight": ("143_40", 663, 823, True),
}


class PlayerMocap:
    def __init__(self, rig):
        self.rig = rig
        self.captures = {name: Capture(name) for name in {take[0] for take in TAKES.values()}}
        self.rest = {bone.name: bone.matrix_local.to_quaternion() for bone in rig.data.bones}
        self.mapping = {"Root": "root", "Torso": "thorax", "Head": "head"}
        self.align = {}
        self.loop_corrections = {}
        self.hand_height_centers = {}
        self.scales = {}
        self.durations = {clip: (end - start) / SOURCE_FPS for clip, (_, start, end, _) in TAKES.items()}
        self.durations.update(Idle=3.2, Jump=0.32, Fall=0.30)
        for side, prefix in (("L", "r"), ("R", "l")):
            self.mapping.update(
                {
                    "Thigh." + side: prefix + "femur",
                    "Shin." + side: prefix + "tibia",
                    "Foot." + side: prefix + "foot",
                    "UpperArm." + side: prefix + "humerus",
                    "Forearm." + side: prefix + "radius",
                }
            )
        for clip, (name, start, end, _) in TAKES.items():
            capture = self.captures[name]
            self.scales[clip] = 0.785 / (capture.bones["rfemur"]["length"] + capture.bones["rtibia"]["length"])
            self.loop_corrections[clip] = {
                bone: capture.rotations[end][bone]
                .to_quaternion()
                .rotation_difference(capture.rotations[start][bone].to_quaternion())
                for bone in capture.rotations[start]
            }
            self.hand_height_centers[clip] = {
                side: np.mean([p[prefix + "wrist"].y - p["root"].y for p in capture.positions[start : end + 1]])
                for side, prefix in (("L", "r"), ("R", "l"))
            }
            align = {}
            for target, source in self.mapping.items():
                if target.startswith(("Thigh.", "Shin.", "UpperArm.", "Forearm.")):
                    child = (
                        {
                            "Thigh": "Shin",
                            "Shin": "Foot",
                            "UpperArm": "Forearm",
                            "Forearm": "Hand",
                        }[target.split(".")[0]]
                        + "."
                        + target.split(".")[1]
                    )
                    direction = rig.data.bones[child].head_local - rig.data.bones[target].head_local
                    align[target] = direction.rotation_difference(CONVERT @ capture.bones[source]["direction"])
                elif target.startswith("Foot."):
                    align[target] = (CONVERT @ capture.bones[source]["axis"]).to_quaternion()
                else:
                    align[target] = Quaternion()
            self.align[clip] = align

        self.posture_corrections = {}
        for clip in TAKES:
            directions = {bone: Vector() for bone in ("Root", "Torso", "Head")}
            for sample in range(48):
                _, world = self.world_pose(clip, sample / 48)
                for bone in directions:
                    directions[bone] += world[bone] @ UP
            # Remove persistent capture bias while retaining the recorded sway.
            self.posture_corrections[clip] = {
                bone: Quaternion((0, 1, 0), -math.atan2(up.x, up.z)) for bone, up in directions.items()
            }

    def world_pose(self, clip, t):
        name, start, end, looping = TAKES[clip]
        capture = self.captures[name]
        positions, rotations = capture.sample(start + (end - start) * t)
        if looping:
            weight = t * t * (3 - 2 * t)
            for bone in rotations:
                rotations[bone] = rotations[bone] @ Quaternion().slerp(self.loop_corrections[clip][bone], weight)
        forward = CONVERT @ capture.rotations[start]["root"] @ Vector((0, 0, 1))
        facing = Quaternion(UP, -math.atan2(forward.x, -forward.y))
        convert = CONVERT.to_quaternion()
        world = {}
        for target, source in self.mapping.items():
            world[target] = facing @ convert @ rotations[source] @ convert.inverted() @ self.align[clip][target]
        return positions, world

    def apply(self, clip, t):
        name, start, end, _ = TAKES[clip]
        capture = self.captures[name]
        positions, world = self.world_pose(clip, t)
        for bone, correction in self.posture_corrections[clip].items():
            world[bone] = correction @ world[bone]
        if clip in ("Run", "Land"):
            # Rigid hands and forearms need clearance from the pelvic armour.
            angle = 0.30 if clip == "Land" else 0.18
            for side, sign in (("L", 1), ("R", -1)):
                clearance = Quaternion((0, 1, 0), sign * angle)
                for part in ("UpperArm.", "Forearm."):
                    world[part + side] = clearance @ world[part + side]
        if clip == "StrafeRight":
            reflection = Matrix.Diagonal((-1.0, 1.0, 1.0))
            mirrored = {}
            for target in world:
                opposite = target
                if target.endswith((".L", ".R")):
                    opposite = target[:-1] + ("R" if target[-1] == "L" else "L")
                mirrored[target] = (reflection @ world[opposite].to_matrix() @ reflection).to_quaternion()
            world = mirrored
        for target, rotation in world.items():
            bone = self.rig.pose.bones[target]
            parent_rotation = world.get(bone.parent.name, Quaternion()) if bone.parent else Quaternion()
            rest = self.rest[target]
            bone.rotation_quaternion = rest.inverted() @ parent_rotation.inverted() @ rotation @ rest
        if clip == "Climb":
            self.climb_hands(clip, positions, world, t)
        if clip == "Run":
            feet = ("lfoot", "rfoot", "ltoes", "rtoes")
            floor = np.percentile(
                [min(p[foot].y for foot in feet) for p in capture.positions[start : end + 1]],
                20,
            )
            scale = 0.91 / np.mean([p["root"].y - floor for p in capture.positions[start : end + 1]])
            first = min(capture.positions[start][foot].y for foot in feet)
            last = min(capture.positions[end][foot].y for foot in feet)
            return (
                max(
                    0,
                    min(positions[foot].y for foot in feet) - floor + (first - last) * t * t * (3 - 2 * t),
                )
                * scale
            )
        return 0.0

    def climb_hands(self, clip, positions, world, t):
        # Captured stepladder handholds are raised to the game's vertical ladder rungs.
        bpy.context.view_layer.update()
        for side, prefix, sign in (("L", "r", -1), ("R", "l", 1)):
            upper, lower, hand = (name + "." + side for name in ("UpperArm", "Forearm", "Hand"))
            shoulder = self.rig.pose.bones[upper].head.copy()
            relative_height = positions[prefix + "wrist"].y - positions["root"].y
            name, start, end, _ = TAKES[clip]
            capture = self.captures[name]
            first, last = (capture.positions[frame] for frame in (start, end))
            correction = first[prefix + "wrist"].y - first["root"].y - last[prefix + "wrist"].y + last["root"].y
            relative_height += correction * t * t * (3 - 2 * t)
            hand_height = 1.68 + (relative_height - self.hand_height_centers[clip][side]) * self.scales[clip]
            target = Vector((sign * 0.24, -0.32, hand_height))
            upper_rest = self.rig.data.bones[lower].head_local - self.rig.data.bones[upper].head_local
            lower_rest = self.rig.data.bones[hand].head_local - self.rig.data.bones[lower].head_local
            axis = (target - shoulder).normalized()
            distance = min(
                (target - shoulder).length,
                upper_rest.length + lower_rest.length - 0.005,
            )
            along = (upper_rest.length_squared - lower_rest.length_squared + distance * distance) / (2 * distance)
            pole = Vector((sign * 0.10, 0.08, -1.0))
            bend = (pole - axis * pole.dot(axis)).normalized()
            elbow = shoulder + axis * along + bend * math.sqrt(max(0, upper_rest.length_squared - along * along))
            target = shoulder + axis * distance
            upper_rotation = (world["Torso"] @ upper_rest).rotation_difference(elbow - shoulder) @ world["Torso"]
            lower_rotation = (upper_rotation @ lower_rest).rotation_difference(target - elbow) @ upper_rotation
            hand_rotation = Euler((0.15, math.pi, 0)).to_quaternion()
            for name, rotation, parent in (
                (upper, upper_rotation, world["Torso"]),
                (lower, lower_rotation, upper_rotation),
                (hand, hand_rotation, lower_rotation),
            ):
                rest = self.rest[name]
                self.rig.pose.bones[name].rotation_quaternion = rest.inverted() @ parent.inverted() @ rotation @ rest
