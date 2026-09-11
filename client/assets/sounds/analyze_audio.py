"""Measure game audio and write analysis.json. Requires ffmpeg and ffprobe on PATH."""

import argparse
import array
import hashlib
import json
import math
import subprocess
import sys
from pathlib import Path

ASSETS = Path(__file__).resolve().parent.parent
EXTENSIONS = {".ogg", ".wav", ".mp3", ".flac"}
SAMPLE_RATE = 48000
TARGET_DBFS = -24.0
PEAK_CEILING_DBFS = -6.0


def decode(path):
    metadata = json.loads(
        subprocess.check_output(
            [
                "ffprobe",
                "-v",
                "error",
                "-select_streams",
                "a:0",
                "-show_entries",
                "stream=channels,sample_rate",
                "-of",
                "json",
                str(path),
            ]
        )
    )["streams"][0]
    raw = subprocess.check_output(
        ["ffmpeg", "-v", "error", "-i", str(path), "-map", "0:a:0", "-f", "f32le", "-ar", str(SAMPLE_RATE), "pipe:1"]
    )
    samples = array.array("f")
    samples.frombytes(raw)
    if sys.byteorder != "little":
        samples.byteswap()
    return samples, metadata


def db(amplitude):
    return 20 * math.log10(amplitude) if amplitude > 0 else None


def measure(samples, channels, rate=SAMPLE_RATE):
    if not samples or len(samples) % channels:
        raise ValueError("Audio has no complete sample frames")
    if any(not math.isfinite(value) for value in samples):
        raise ValueError("Audio contains non-finite samples")
    energy = [
        sum(value * value for value in samples[i : i + channels]) / channels for i in range(0, len(samples), channels)
    ]
    window = min(round(rate * 0.05), len(energy))
    running = sum(energy[:window])
    strongest = running
    for i in range(window, len(energy)):
        running += energy[i] - energy[i - window]
        strongest = max(strongest, running)
    peak = db(max(map(abs, samples)))
    rms50 = db(math.sqrt(strongest / window))
    gain = min(0.0, TARGET_DBFS - rms50, PEAK_CEILING_DBFS - peak) if peak is not None else 0.0
    return {
        "duration_secs": len(energy) / rate,
        "peak_dbfs": peak,
        "rms_dbfs": db(math.sqrt(sum(energy) / len(energy))),
        "strongest_50ms_rms_dbfs": rms50,
        "suggested_gain_db": gain,
    }


def analyze(path):
    samples, metadata = decode(path)
    result = measure(samples, metadata["channels"])
    return {
        "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        "sample_rate": int(metadata["sample_rate"]),
        "channels": metadata["channels"],
        **{key: round(value, 6) if value is not None else None for key, value in result.items()},
    }


def catalog(assets):
    return {
        "version": 1,
        "analysis_sample_rate": SAMPLE_RATE,
        "target_50ms_rms_dbfs": TARGET_DBFS,
        "peak_ceiling_dbfs": PEAK_CEILING_DBFS,
        "sounds": {
            path.relative_to(assets).as_posix(): analyze(path)
            for path in sorted(assets.rglob("*"))
            if path.suffix.lower() in EXTENSIONS
        },
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Fail if the saved analysis differs from the audio assets")
    args = parser.parse_args()
    result = catalog(ASSETS)
    destination = ASSETS / "sounds" / "analysis.json"
    if args.check:
        if not destination.exists() or json.loads(destination.read_text()) != result:
            raise SystemExit("Audio analysis is stale; run client/assets/sounds/analyze_audio.py")
    else:
        destination.write_text(json.dumps(result, indent=2, allow_nan=False) + "\n")
    print(f"{'Checked' if args.check else 'Analyzed'} {len(result['sounds'])} audio files")


if __name__ == "__main__":
    main()
