#!/usr/bin/env bash
# One-time by-hand build of the sherpa-onnx C API (shared libs, TTS off) for `speech-host-sherpa`.
# The cargo build never downloads: it only links what this script installs, through
# SHERPA_ONNX_LIB_DIR. Usage: dev/build-sherpa.sh [cache-dir]   (default ~/rs-wt/v-host/cache)
# Prints the SHERPA_ONNX_LIB_DIR to export. TTS stays OFF so no GPL espeak-ng is built or linked.
set -euo pipefail
cache="${1:-$HOME/rs-wt/v-host/cache}"
tag=v1.13.8
ort=onnxruntime-linux-x64-glibc2_17-Release-1.28.2.zip
ort_url=https://github.com/csukuangfj/onnxruntime-libs/releases/download/v1.28.2/$ort
ort_sha=c4f8994d56191d9d2c92a961b39fe790459f2c5d155f912b239506ea31359534
mkdir -p "$cache"
[ -d "$cache/src/.git" ] || git clone --depth 1 --branch "$tag" https://github.com/k2-fsa/sherpa-onnx "$cache/src"
# onnxruntime is fetched once here, checked, and handed to cmake as a local file.
[ -f "$cache/$ort" ] || curl -fL "$ort_url" -o "$cache/$ort"
echo "$ort_sha  $cache/$ort" | sha256sum -c -
mkdir -p "$cache/build"
cp -n "$cache/$ort" "$cache/build/$ort"
cmake -S "$cache/src" -B "$cache/build" \
  -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=ON \
  -DSHERPA_ONNX_ENABLE_TTS=OFF -DSHERPA_ONNX_ENABLE_SPEAKER_DIARIZATION=OFF \
  -DSHERPA_ONNX_ENABLE_PORTAUDIO=OFF -DSHERPA_ONNX_ENABLE_WEBSOCKET=OFF \
  -DSHERPA_ONNX_ENABLE_BINARY=OFF -DSHERPA_ONNX_BUILD_C_API_EXAMPLES=OFF \
  -DSHERPA_ONNX_ENABLE_PYTHON=OFF -DSHERPA_ONNX_ENABLE_TESTS=OFF \
  -DSHERPA_ONNX_ENABLE_GPU=OFF -DCMAKE_INSTALL_PREFIX="$cache/install"
cmake --build "$cache/build" -j"$(nproc)" --target install
echo "export SHERPA_ONNX_LIB_DIR=$cache/install/lib"
