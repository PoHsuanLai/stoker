#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["qwen-vl-utils"]
# ///
"""Writes fixtures/smart_resize.csv: the reference `smart_resize` of qwen-vl-utils (Apache-2.0)
for every (window, factor) pair below. Run by hand; nothing in CI generates fixtures.

Columns: height,width,factor,min_pixels,max_pixels,out_height,out_width (out_* are empty when
the reference raises, which is the AspectTooExtreme case).

    uv run dev/smart-resize-vectors.py > fixtures/smart_resize.csv
"""
import sys
from unittest import mock

# vision_process imports torch and friends at module level; smart_resize is pure arithmetic, so
# the heavy imports are stubbed rather than installed.
for name in ("numpy", "torch", "torchvision", "torchvision.io", "torchvision.transforms",
             "torchvision.transforms.functional", "torchcodec", "decord"):
    sys.modules.setdefault(name, mock.MagicMock())

from qwen_vl_utils.vision_process import smart_resize  # noqa: E402

SIZES = [  # (width, height) in device pixels
    (2560, 1600), (1366, 768), (1920, 1080), (1280, 720), (3840, 2160), (2880, 1800),
    (1024, 768), (800, 600), (640, 480), (320, 240), (100, 100), (28, 28), (1, 1), (50, 30),
    (4000, 20), (20, 4000), (5000, 25), (200, 1), (1, 201), (3413, 1920),
    (2049, 1153), (1707, 960), (4096, 2304), (7680, 4320), (500, 1500),
]
RULES = [  # (factor, min_pixels, max_pixels)
    (28, 56 * 56, 28 * 28 * 1280),            # Qwen2.5-VL defaults
    (28, 4 * 28 * 28, 16384 * 28 * 28),      # the library defaults (4 to 16384 tokens)
    (32, 65536, 16777216),                    # Holo 3.1 (preprocessor_config.json)
    (32, 3136, 1003520),
]

print("height,width,factor,min_pixels,max_pixels,out_height,out_width")
for factor, lo, hi in RULES:
    for w, h in SIZES:
        try:
            oh, ow = smart_resize(h, w, factor=factor, min_pixels=lo, max_pixels=hi)
        except ValueError:
            oh = ow = ""
        print(f"{h},{w},{factor},{lo},{hi},{oh},{ow}")
