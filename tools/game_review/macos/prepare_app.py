import argparse
import json
import os
import plistlib
import shlex
import shutil
import tempfile
from pathlib import Path


def prepare_app(repo, game_args):
    cargo = shutil.which("cargo")
    if cargo is None:
        raise SystemExit("cargo missing from PATH")

    review = Path(tempfile.mkdtemp(prefix="cuboid-macos-review-", dir="/tmp"))
    app = review / "Cuboid Review.app"
    contents = app / "Contents"
    (contents / "MacOS").mkdir(parents=True)
    log = review / "game.log"
    bundle_id = f"local.cuboid.review.{review.name.removeprefix('cuboid-macos-review-').replace('_', '-')}"
    (contents / "Info.plist").write_bytes(
        plistlib.dumps(
            {
                "CFBundleExecutable": "review",
                "CFBundleIdentifier": bundle_id,
                "CFBundleName": "Cuboid Review",
                "CFBundlePackageType": "APPL",
                "NSHighResolutionCapable": True,
            }
        )
    )
    command = shlex.join([str(Path(cargo).absolute()), "run", "--release", "--", *game_args])
    launcher = contents / "MacOS" / "review"
    launcher.write_text(
        "#!/bin/sh\nset -eu\n"
        f"export PATH={shlex.quote(os.environ['PATH'])}\n"
        f"cd {shlex.quote(str(repo))}\n"
        f"exec {command} > {shlex.quote(str(log))} 2>&1\n",
        encoding="utf-8",
    )
    launcher.chmod(0o755)

    settings = repo / "config/client/client_local.json"
    backup = review / "client_local.before.json"
    if settings.exists():
        shutil.copyfile(settings, backup)
    return {
        "app": str(app),
        "bundle_id": bundle_id,
        "log": str(log),
        "settings_backup": str(backup) if backup.exists() else None,
    }


def main():
    parser = argparse.ArgumentParser(description="Prepare a temporary macOS review app; does not launch the game.")
    parser.add_argument("game_args", nargs=argparse.REMAINDER, help="game arguments after --")
    args = parser.parse_args().game_args
    if args[:1] == ["--"]:
        args = args[1:]
    if not args:
        args = [
            "--map",
            "hotel",
            "--god",
            "--peace",
            "--name",
            "Reviewer",
            "--volume",
            "0",
            "--windowed",
            "--resolution",
            "1200x800",
        ]
    print(json.dumps(prepare_app(Path(__file__).resolve().parents[3], args), indent=2))


if __name__ == "__main__":
    main()
