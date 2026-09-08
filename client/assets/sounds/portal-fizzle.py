"""Synthesize portal-fizzle.wav: python3 client/assets/sounds/portal-fizzle.py."""

import math
import random
import struct
import wave
from pathlib import Path

SAMPLE_RATE = 44_100
DURATION = 0.55
PEAK = 0.65
SEED = 7319


def synthesize():
    randomizer = random.Random(SEED)
    count = round(SAMPLE_RATE * DURATION)
    samples = []
    low_noise = 0.0
    noise_baseline = 0.0
    phase = 0.0
    baseline_filter = 1.0 - math.exp(-math.tau * 180.0 / SAMPLE_RATE)

    for index in range(count):
        time = index / SAMPLE_RATE
        progress = index / (count - 1)
        attack = min(time / 0.006, 1.0)
        release = min((DURATION - time) / 0.09, 1.0)
        envelope = attack * release * math.exp(-5.0 * progress)

        cutoff = 700.0 + 6500.0 * (1.0 - progress) ** 2
        noise_filter = 1.0 - math.exp(-math.tau * cutoff / SAMPLE_RATE)
        low_noise += noise_filter * (randomizer.uniform(-1.0, 1.0) - low_noise)
        noise_baseline += baseline_filter * (low_noise - noise_baseline)
        hiss = low_noise - noise_baseline

        frequency = 160.0 + 1500.0 * math.exp(-9.0 * progress)
        phase += math.tau * frequency / SAMPLE_RATE
        buzz = math.sin(phase + 0.8 * math.sin(phase * 2.13))
        flutter = 0.8 + 0.2 * math.sin(math.tau * 57.0 * time)
        samples.append(envelope * (0.78 * hiss * flutter + 0.22 * buzz))

    # Fade after removing DC so both endpoints remain silent.
    mean = sum(samples) / count
    samples = [
        (sample - mean) * min(index / 64.0, (count - 1 - index) / 64.0, 1.0)
        for index, sample in enumerate(samples)
    ]
    gain = PEAK * 32767 / max(abs(sample) for sample in samples)
    return b"".join(struct.pack("<h", round(sample * gain)) for sample in samples)


def main():
    output = Path(__file__).with_suffix(".wav")
    with wave.open(str(output), "wb") as sound:
        sound.setnchannels(1)
        sound.setsampwidth(2)
        sound.setframerate(SAMPLE_RATE)
        sound.writeframes(synthesize())
    print(f"Generated {output.name}: {DURATION}s, mono, {SAMPLE_RATE} Hz, 16-bit PCM")


if __name__ == "__main__":
    main()
