"""Rotation and reflection of clipboard geometry and its nested definitions."""

import copy

from .constants import FACES, ZONE_LISTS
from .geometry import normalized_wall
from .transforms import EDGE_LISTS, record_lists


class PlanTransform:
    def __init__(self, width, height, operation):
        if operation not in ("rotate", "mirror_x", "mirror_y"):
            raise ValueError(f"Unknown transform: {operation}")
        self.width, self.height, self.operation = width, height, operation

    def point(self, x, y):
        if self.operation == "rotate":
            return [self.height - y, x]
        if self.operation == "mirror_x":
            return [self.width - x, y]
        return [x, self.height - y]

    def rect(self, x, y, width, height):
        corners = [self.point(cx, cy) for cx in (x, x + width) for cy in (y, y + height)]
        return [min(p[0] for p in corners), min(p[1] for p in corners)]

    def vector(self, x, y):
        if self.operation == "rotate":
            return [-y, x]
        return [-x, y] if self.operation == "mirror_x" else [x, -y]

    def direction(self, side):
        directions = {"N": (0, -1), "E": (1, 0), "S": (0, 1), "W": (-1, 0)}
        if side not in directions:
            return side
        transformed = tuple(self.vector(*directions[side]))
        return next(key for key, value in directions.items() if value == transformed)


def transform_block(block, operation, definitions):
    additions = {}
    names = {}
    visiting = set()

    def transform_definition(name):
        if name in visiting:
            raise ValueError("Cannot transform cyclic nested geometry.")
        if name not in definitions:
            raise ValueError(f"Nested geometry {name!r} is missing.")
        if name not in names:
            suffix = {"rotate": "rotated", "mirror_x": "mirrored_x", "mirror_y": "mirrored_y"}[operation]
            stem = f"{name}_{suffix}"
            candidate = stem
            index = 2
            while candidate in definitions or candidate in additions or candidate in names.values():
                candidate = f"{stem}_{index}"
                index += 1
            names[name] = candidate
            visiting.add(name)
            additions[candidate] = transform_geometry(definitions[name])
            visiting.remove(name)
        return names[name]

    def transform_geometry(data):
        result = copy.deepcopy(data)
        transform = PlanTransform(data["grid_cols"], data["grid_rows"], operation)
        if operation == "rotate":
            result["grid_cols"], result["grid_rows"] = data["grid_rows"], data["grid_cols"]
        for (_, name), entries in record_lists(result):
            for entry in entries:
                if name in ZONE_LISTS:
                    x0, x1 = entry["cols"]
                    y0, y1 = entry["rows"]
                    x, y = transform.rect(x0, y0, x1 - x0, y1 - y0)
                    width, height = (y1 - y0, x1 - x0) if operation == "rotate" else (x1 - x0, y1 - y0)
                    entry["cols"], entry["rows"] = [x, x + width], [y, y + height]
                elif name in EDGE_LISTS:
                    a = transform.point(entry["c0"], entry["r0"])
                    b = transform.point(entry["c1"], entry["r1"])
                    entry.update(zip(("c0", "r0", "c1", "r1"), normalized_wall([*a, *b])))
                elif name == "ramps":
                    entry["low"] = transform.point(*entry["low"])
                    entry["high"] = transform.point(*entry["high"])
                elif name == "nested_maps":
                    original = entry["map"]
                    if original not in definitions:
                        raise ValueError(f"Nested geometry {original!r} is missing.")
                    child = definitions[original]
                    width, height = child["grid_cols"], child["grid_rows"]
                    for end in ("from", "to"):
                        x, y = entry[end]
                        if not (
                            0 <= x and x + width <= data["grid_cols"] and 0 <= y and y + height <= data["grid_rows"]
                        ):
                            raise ValueError("Include the full nested-map footprints at both ends before transforming.")
                        entry[end] = transform.rect(x, y, width, height)
                        nx, ny, nz = entry[f"{end}_nudge"]
                        nx, nz = transform.vector(nx, nz)
                        entry[f"{end}_nudge"] = [nx, ny, nz]
                    entry["map"] = transform_definition(original)
                else:
                    entry["col"], entry["row"] = transform.rect(entry["col"], entry["row"], 1, 1)
                if "side" in entry:
                    entry["side"] = transform.direction(entry["side"])
                face_names = {"north": "N", "east": "E", "south": "S", "west": "W"}
                old_faces = {face: entry[face] for face in FACES if face in entry}
                for face, side in face_names.items():
                    if face in old_faces:
                        new_side = transform.direction(side)
                        new_face = next(key for key, value in face_names.items() if value == new_side)
                        entry[new_face] = old_faces[face]
        return result

    transformed = transform_geometry(block)
    return transformed, additions
