"""`mapauthor` commands: build, describe, where, surface, jump, fling, ranges, proof."""

from __future__ import annotations

import argparse
import json
import runpy
import sys
from pathlib import Path

from map_editor.catalogs import map_layout_path

from .context import MapContext
from .describe import plan, summary
from .measure import jump, parse_surface, parse_takeoff, ranges, surface, where
from .proof import summarize


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="mapauthor", description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)

    build = commands.add_parser("build", help="run the map's build.py, which writes its layout.json")
    build.add_argument("map")

    describe = commands.add_parser("describe", help="the plan of every level and the structural summary")
    describe.add_argument("map")
    describe.add_argument("--level", type=int, help="one level's plan only")
    describe.add_argument("--plan", action="store_true", help="the plan only")
    describe.add_argument("--summary", action="store_true", help="the summary only")

    point = commands.add_parser("where", help="convert a grid point to world metres, or a world point to a cell")
    point.add_argument("map")
    point.add_argument("point", nargs="?", help="L<level>:<x>,<z> in cells")
    point.add_argument("--world", help="x,y,z in world metres")

    surf = commands.add_parser("surface", help="whether a wall or floor takes a portal, and where it is")
    surf.add_argument("map")
    surf.add_argument("surface", help="wall:L<level>:<col>,<row>:<side> or floor:L<level>:<x>,<z>")

    for name, help_text in (("jump", "a flight from a cell edge"), ("fling", "a flight through a portal pair")):
        flight = commands.add_parser(name, help=help_text)
        flight.add_argument("map")
        flight.add_argument("--from", dest="takeoff", required=True, help="L<level>:<col>,<row>:<side>[:<along>]")
        flight.add_argument("--walk", action="store_true", help="walk off instead of jumping")
        flight.add_argument("--late", type=float, default=0.0, help="jump this many seconds after the edge")
        flight.add_argument("--air-control", action="store_true")
        if name == "fling":
            flight.add_argument("--entry", required=True, help="portal 1, the one the flight enters")
            flight.add_argument("--exit", help="portal 2; without it the flight ends at portal 1")
            flight.add_argument(
                "--shooter", help="<x>,<z> in cells, where portal 1 is shot from (default: the takeoff)"
            )
            flight.add_argument("--into", action="store_true", help="run into the entry wall from the takeoff cell")

    reach = commands.add_parser("ranges", help="how far jumps and walk-offs carry at this map's physics")
    reach.add_argument("map")

    proof = commands.add_parser("proof", help="summarize an experiment report (a file, or - for stdin)")
    proof.add_argument("map")
    proof.add_argument("report")

    args = parser.parse_args(argv)
    try:
        return run(args)
    except (ValueError, FileNotFoundError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


def run(args) -> int:
    if args.command == "build":
        script = map_layout_path(args.map).with_name("build.py")
        if not script.exists():
            raise FileNotFoundError(f"{script} does not exist; an AI-authored map keeps its build script there")
        runpy.run_path(str(script), run_name="__main__")
        return 0
    ctx = MapContext.load(args.map)
    if args.command == "describe":
        parts = []
        if not args.summary:
            parts.append(plan(ctx, args.level))
        if not args.plan and args.level is None:
            parts.append(summary(ctx))
        print("\n\n".join(parts))
    elif args.command == "where":
        if (args.point is None) == (args.world is None):
            raise ValueError("give a grid point or --world x,y,z")
        print(where(ctx, args.point, args.world))
    elif args.command == "surface":
        print(surface(ctx, parse_surface(args.surface)))
    elif args.command in ("jump", "fling"):
        entry = parse_surface(args.entry) if args.command == "fling" else None
        exit = parse_surface(args.exit) if args.command == "fling" and args.exit else None
        shooter = None
        if args.command == "fling" and args.shooter:
            shooter = tuple(float(v) for v in args.shooter.split(","))
        print(
            jump(
                ctx,
                parse_takeoff(args.takeoff),
                walk=args.walk,
                late=args.late,
                air_control=args.air_control,
                entry=entry,
                exit=exit,
                shooter=shooter,
                into=args.command == "fling" and args.into,
            )
        )
    elif args.command == "ranges":
        print(ranges(ctx))
    elif args.command == "proof":
        text = sys.stdin.read() if args.report == "-" else Path(args.report).read_text(encoding="utf-8")
        print(summarize(json.loads(text), ctx.frame))
    return 0
