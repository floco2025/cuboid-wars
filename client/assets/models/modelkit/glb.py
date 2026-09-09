"""GLB post-processing shared by the animated generators."""

import json
import struct

JSON_CHUNK = 0x4E4F534A


def rewrite_glb_json(path, edit):
    """Apply `edit(document)` to the JSON chunk of the GLB at `path` in place.

    Clip indices are an asset contract (assets.json names them by index); Blender's
    action ordering is not, so generators reorder and filter clips here after export.
    """
    raw = path.read_bytes()
    length = struct.unpack_from("<I", raw, 12)[0]
    document = json.loads(raw[20 : 20 + length])
    edit(document)
    encoded = json.dumps(document, separators=(",", ":")).encode()
    encoded += b" " * (-len(encoded) % 4)
    binary = raw[20 + length :]
    header = struct.pack(
        "<4sIIII", b"glTF", 2, 20 + len(encoded) + len(binary), len(encoded), JSON_CHUNK
    )
    path.write_bytes(header + encoded + binary)


def channel_target(document, channel):
    """Name of the node an animation channel drives."""
    return document["nodes"][channel["target"]["node"]]["name"]
