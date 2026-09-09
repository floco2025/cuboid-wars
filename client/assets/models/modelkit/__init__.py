"""Shared library behind the model generators in the parent directory."""

from .glb import channel_target, rewrite_glb_json
from .materials import ModelMaterials, plain_material, project_uv
from .primitives import (
    box,
    child_of,
    cylinder,
    empty,
    finish,
    label,
    rigged,
    rod,
    sphere,
)
from .wear import bake_articulated_wear, bake_wear

__all__ = [
    "ModelMaterials",
    "bake_articulated_wear",
    "bake_wear",
    "box",
    "channel_target",
    "child_of",
    "cylinder",
    "empty",
    "finish",
    "label",
    "plain_material",
    "project_uv",
    "rewrite_glb_json",
    "rigged",
    "rod",
    "sphere",
]
