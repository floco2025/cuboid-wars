"""Synthesize the actor movement loops: python3 client/assets/sounds/actor-movement.py."""

import math
import random
import struct
import wave
from pathlib import Path

SAMPLE_RATE = 44_100
DURATION = 6.0
CROSSFADE = 0.18
PEAK = 0.7


def filtered_noise(randomizer, count, low, high):
    bass = treble = 0.0
    low_gain = 1.0 - math.exp(-math.tau * low / SAMPLE_RATE)
    high_gain = 1.0 - math.exp(-math.tau * high / SAMPLE_RATE)
    samples = []
    for _ in range(count):
        treble += high_gain * (randomizer.uniform(-1.0, 1.0) - treble)
        bass += low_gain * (treble - bass)
        samples.append(treble - bass)
    rms = math.sqrt(sum(value * value for value in samples) / count)
    return [value / rms for value in samples]


def tank(count, heavy):
    randomizer = random.Random(8129 if heavy else 4517)
    combustion = filtered_noise(randomizer, count, 28 if heavy else 55, 380 if heavy else 650)
    tread = filtered_noise(randomizer, count, 480 if heavy else 850, 3800 if heavy else 5200)
    exhaust = filtered_noise(randomizer, count, 35, 150 if heavy else 230)
    firing_hz = 42 if heavy else 67
    track_hz = 9 if heavy else 15
    engine_phase = track_phase = 0.0
    samples = []
    for index in range(count):
        time = index / SAMPLE_RATE
        revs = 1.0 + 0.014 * math.sin(math.tau * 0.67 * time) + 0.006 * math.sin(math.tau * 2.3 * time)
        engine_phase += math.tau * firing_hz * revs / SAMPLE_RATE
        track_phase += math.tau * track_hz * (1.0 + 0.018 * math.sin(math.tau * 0.83 * time)) / SAMPLE_RATE
        firing = (0.5 + 0.5 * math.cos(engine_phase)) ** 5
        engine = sum(math.sin(engine_phase * harmonic + 0.23 * harmonic) / harmonic for harmonic in range(1, 8))
        rumble = 0.30 * engine + (0.22 + 0.40 * firing) * combustion[index] + 0.22 * exhaust[index]
        left_track = (track_phase / math.tau) % 1.0
        right_track = (track_phase / math.tau * 1.017 + 0.43) % 1.0
        clatter = 0.0
        for phase in (left_track, right_track):
            age = phase / track_hz
            strike = (1.0 - math.exp(-age * 2200.0)) * math.exp(-age * (65 if heavy else 105))
            metal = sum(
                math.sin(math.tau * frequency * age) * gain
                for frequency, gain in ((310 if heavy else 590, 0.6), (870 if heavy else 1480, 0.3), (1930, 0.15))
            )
            clatter += strike * (0.55 * tread[index] + metal)
        gear = 0.055 * math.sin(engine_phase * (6.7 if heavy else 8.3) + 0.8 * math.sin(engine_phase))
        samples.append(math.tanh(0.85 * (rumble + 0.65 * clatter + gear + 0.045 * tread[index])))
    return samples


def quadcopter(count):
    randomizer = random.Random(9371)
    wash = filtered_noise(randomizer, count, 350, 6500)
    body = filtered_noise(randomizer, count, 70, 650)
    phases = [0.0, 1.3, 2.8, 4.4]
    samples = []
    for index in range(count):
        time = index / SAMPLE_RATE
        rotors = flutter = 0.0
        for rotor, blade_hz in enumerate((174.0, 181.0, 187.0, 193.0)):
            throttle = 1.0 + 0.014 * math.sin(math.tau * (0.53 + rotor * 0.17) * time + rotor)
            phases[rotor] += math.tau * blade_hz * throttle / SAMPLE_RATE
            phase = phases[rotor]
            rotors += sum(
                math.sin(harmonic * phase + 0.18 * rotor * harmonic) / harmonic**1.25 for harmonic in range(1, 11)
            )
            flutter += (0.5 + 0.5 * math.sin(phase)) ** 3
        samples.append(0.17 * rotors + wash[index] * (0.12 + 0.055 * flutter) + 0.055 * body[index])
    return samples


def encode_loop(samples):
    count = round(SAMPLE_RATE * DURATION)
    overlap = round(SAMPLE_RATE * CROSSFADE)
    # Overlap the continuation with the head, keeping a full-volume seam on every repeat.
    for index in range(overlap):
        weight = 0.5 - 0.5 * math.cos(math.pi * index / (overlap - 1))
        samples[index] = samples[count + index] * (1.0 - weight) + samples[index] * weight
    samples = samples[:count]
    mean = sum(samples) / count
    samples = [sample - mean for sample in samples]
    gain = PEAK * 32767 / max(abs(sample) for sample in samples)
    return b"".join(struct.pack("<h", round(sample * gain)) for sample in samples)


def main():
    count = round(SAMPLE_RATE * (DURATION + CROSSFADE))
    for name, synthesize in (
        ("scuttler-movement.wav", lambda: tank(count, heavy=False)),
        ("bruiser-movement.wav", lambda: tank(count, heavy=True)),
        ("zapper-movement.wav", lambda: quadcopter(count)),
    ):
        with wave.open(str(Path(__file__).with_name(name)), "wb") as sound:
            sound.setnchannels(1)
            sound.setsampwidth(2)
            sound.setframerate(SAMPLE_RATE)
            sound.writeframes(encode_loop(synthesize()))
        print(f"Generated {name}: {DURATION}s, mono, {SAMPLE_RATE} Hz, 16-bit PCM")


if __name__ == "__main__":
    main()
