#!/usr/bin/env python3
"""Make fixtures/audio/silero_synth.wav (+ silero_synth_ref.csv) for speech-vad-silero.

Usage (a throwaway uv project with onnxruntime and numpy):
  uv run --project ~/rs-wt/v-silero/ref python dev/silero-fixture.py <silero_vad.onnx v6.2.1> fixtures/audio

The WAV is deterministic (fixed seed): 16 kHz mono S16, silence, a voiced-like harmonic burst
with a syllable envelope, noise, a second burst, silence. No real recordings. The CSV is the
onnxruntime reference: one row per 512-sample frame, `frame,probability` (probability to 6
decimals), computed as snakers4/silero-vad's OnnxWrapper does (64-sample context, state zeroed).
"""
import csv, struct, sys, wave
import numpy as np
import onnxruntime as ort

SR = 16000
rng = np.random.default_rng(20261006)

def silence(sec): return rng.normal(0, 3, int(SR * sec))

def burst(sec, f0):
    t = np.arange(int(SR * sec)) / SR
    f = f0 * (1 + 0.08 * np.sin(2 * np.pi * 3 * t))
    ph = 2 * np.pi * np.cumsum(f) / SR
    x = sum(np.sin(h * ph) / h for h in range(1, 20))
    env = np.clip(np.sin(np.pi * t / sec * 3) ** 2 + 0.2, 0, 1) * np.minimum(1, t * 40) * np.minimum(1, (sec - t) * 40)
    return 6000 * x * env / 2.5 + rng.normal(0, 20, t.size)

def noise(sec): return rng.normal(0, 1500, int(SR * sec))

pcm = np.concatenate([silence(0.6), burst(0.9, 120), silence(0.4), noise(0.6), burst(0.8, 200), silence(0.5)])
pcm = np.clip(pcm, -32768, 32767).astype("<i2")
pcm = pcm[: len(pcm) // 512 * 512]
out = sys.argv[2]
with wave.open(f"{out}/silero_synth.wav", "wb") as w:
    w.setnchannels(1); w.setsampwidth(2); w.setframerate(SR); w.writeframes(pcm.tobytes())

sess = ort.InferenceSession(sys.argv[1], providers=["CPUExecutionProvider"])
state = np.zeros((2, 1, 128), np.float32)
ctx = np.zeros((1, 64), np.float32)
rows = []
for i in range(len(pcm) // 512):
    x = (pcm[i * 512:(i + 1) * 512].astype(np.float32) / 32768.0)[None, :]
    inp = np.concatenate([ctx, x], axis=1)
    p, state = sess.run(None, {"input": inp, "state": state, "sr": np.array(SR, np.int64)})
    ctx = inp[:, -64:]
    rows.append((i, f"{float(p.reshape(-1)[0]):.6f}"))
with open(f"{out}/silero_synth_ref.csv", "w", newline="") as f:
    wr = csv.writer(f, lineterminator="\n"); wr.writerow(["frame", "probability"]); wr.writerows(rows)
print(len(rows), "frames", min(float(r[1]) for r in rows), max(float(r[1]) for r in rows))
