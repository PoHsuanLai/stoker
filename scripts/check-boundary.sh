#!/usr/bin/env bash
# Crate boundaries, mechanically enforced (ARCHITECTURE.md section 1 is the table; this is its
# mechanical form).
#
# `cargo tree -i <dep>` exits 101 when the dependency is absent, which is precisely the state
# we want. Checking the exit status would therefore fail whenever the boundary holds, so we
# check for OUTPUT instead: any line naming the dependency is a leak.
set -uo pipefail
cd "$(dirname "$0")/.."

# RULES: what a crate reaches through ANY path (transitive, default features). The pure crates
# never reach a runtime, an HTTP client, a bus, a compositor, a database or an inference runtime;
# vision-prep reaches the image stack only through its `pixels` feature; the one io crate
# (model-http: the Transport) may reach an HTTP stack and a runtime but nothing below. The codecs
# (model-wire, model-openai-compat) and model-extract are pure over the Transport trait: they
# reach no HTTP stack and no runtime, so when `HyperTransport` lands in model-http it sits behind
# a feature that only a daemon turns on, and these rows keep checking default features.
# genai-names has no dependencies at all.
# Nothing here reaches porter: stoker is portable and has no porter dependency.
PORTER="porter-core porter-infer porter-client porter-provider"
EFFECTS="tokio hyper hyper-util rustls zbus zvariant reqwest ureq wayland-client wayland-backend reis atspi oo7 ort fastembed rusqlite notify cedar-policy rmcp"
# Audio devices belong to the voice daemon (docket's voiced), never to the speech crates; and no
# TTS stack with a GPL grapheme step is ever linked: Kokoro runs as its own process (voice.md 0.3).
AUDIO_DEVICES="pipewire libpulse-binding libpulse-simple-binding cpal"
GPL_TTS="espeak-rs espeak-ng espeak-ng-sys piper-rs"
IO_FORBIDDEN="zbus zvariant reqwest wayland-client wayland-backend reis atspi oo7 ort fastembed rusqlite notify cedar-policy rmcp"
RULES=(
  "cua-action: $EFFECTS $PORTER"
  "vision-prep: $EFFECTS $PORTER image fast_image_resize"
  "model-provider: $EFFECTS $PORTER"
  "cua-parse: $EFFECTS $PORTER"
  "cua-vendors: $EFFECTS $PORTER"
  "cua-session: $EFFECTS $PORTER"
  "model-replay: $EFFECTS $PORTER"
  "model-catalog: $EFFECTS $PORTER"
  "engine-supervisor: $EFFECTS $PORTER"
  "model-http: $IO_FORBIDDEN $PORTER"
  "model-wire: $EFFECTS $PORTER"
  "model-extract: $EFFECTS $PORTER"
  "genai-names: $EFFECTS $PORTER"
  "model-openai-compat: $EFFECTS $PORTER $AUDIO_DEVICES $GPL_TTS"
  "speech-provider: $EFFECTS $PORTER $AUDIO_DEVICES $GPL_TTS"
  "speech-vad: $EFFECTS $PORTER $AUDIO_DEVICES $GPL_TTS"
  "speech-host-client: $IO_FORBIDDEN $PORTER $AUDIO_DEVICES $GPL_TTS"
)
fail=0

for rule in "${RULES[@]}"; do
  crate="${rule%%:*}"
  read -r -a forbidden <<<"${rule#*:}"
  # A crate that cargo cannot find would make every check below pass vacuously.
  if ! cargo tree -p "$crate" --depth 0 >/dev/null 2>&1; then
    echo "ERROR: cargo tree cannot resolve $crate; the boundary was not checked"
    fail=1
    continue
  fi
  leaked=0
  for dep in "${forbidden[@]}"; do
    if cargo tree -p "$crate" -i "$dep" -e normal,build 2>/dev/null | grep -q .; then
      echo "LEAK: $crate depends on $dep"
      cargo tree -p "$crate" -i "$dep" -e normal,build 2>/dev/null | head -20
      leaked=1
      fail=1
    fi
  done
  if [ "$leaked" -eq 0 ]; then
    echo "boundary holds: $crate reaches none of ${forbidden[*]}"
  fi
done

# The allowed edges between our own crates: each crate's DIRECT normal and build path
# dependencies (all features), and nothing else. A dependency not listed is a leak; so is one
# the crate no longer has, so the table stays exact. Dev dependencies are outside it.
EDGES=(
  "cua-action:"
  "vision-prep: cua-action"
  "model-provider: cua-action genai-names vision-prep"
  "cua-parse: cua-action model-provider"
  "cua-vendors: cua-action cua-parse model-provider"
  "cua-session: cua-action cua-parse cua-vendors model-provider vision-prep"
  "model-replay: model-http model-provider speech-provider vision-prep"
  "model-catalog: cua-action model-provider speech-provider vision-prep"
  "engine-supervisor: model-catalog"
  "model-http:"
  "model-wire: model-http model-provider"
  "model-extract: model-provider"
  "genai-names:"
  "model-openai-compat: model-http model-provider model-wire speech-provider"
  "speech-provider: model-provider"
  "speech-vad: speech-provider"
  "speech-host-client: model-provider speech-provider"
)
for edge in "${EDGES[@]}"; do
  crate="${edge%%:*}"
  read -r -a allowed <<<"${edge#*:}"
  found=$(cargo tree -p "$crate" --depth 1 -e normal,build --prefix none --all-features 2>/dev/null \
    | grep '(/' | awk '{print $1}' | grep -vx "$crate" | sort -u | tr '\n' ' ')
  want=$(printf '%s\n' "${allowed[@]}" | grep . | sort -u | tr '\n' ' ')
  if [ "$found" != "$want" ]; then
    echo "EDGE: $crate depends on [${found% }], the table allows [${want% }]"
    fail=1
  else
    echo "edges hold: $crate depends on [${found% }]"
  fi
done

# The excluded crates (they need a runtime outside the pinned block, so they are not workspace
# members) are checked from their own manifests: their direct path dependencies are exactly the
# ones listed, and they reach no porter crate, no audio device crate and no GPL TTS stack. They
# MAY reach `ort` and `sherpa-onnx`, which is why they are excluded. A nested crate (the fuzz
# targets of cua-parse: nightly only, its own `[workspace]` table) is named by its path under
# `crates/`; it is not in the workspace's `exclude` list, which only names top-level crates.
EXCLUDED=(
  "speech-vad-silero: speech-provider speech-vad"
  "speech-host: speech-provider"
  "cua-parse/fuzz: cua-action cua-parse model-provider"
)
for entry in "${EXCLUDED[@]}"; do
  crate="${entry%%:*}"
  read -r -a allowed <<<"${entry#*:}"
  manifest="crates/$crate/Cargo.toml"
  # The first line of `cargo tree` is the crate itself, whatever its package is called.
  found=$(cargo tree --manifest-path "$manifest" --depth 1 -e normal,build --prefix none 2>/dev/null \
    | tail -n +2 | grep '(/' | awk '{print $1}' | sort -u | tr '\n' ' ')
  want=$(printf '%s\n' "${allowed[@]}" | grep . | sort -u | tr '\n' ' ')
  if [ "$found" != "$want" ]; then
    echo "EDGE: $crate (excluded) depends on [${found% }], the table allows [${want% }]"
    fail=1
  else
    echo "edges hold: $crate (excluded) depends on [${found% }]"
  fi
  leaked=0
  for dep in $PORTER $AUDIO_DEVICES $GPL_TTS; do
    if cargo tree --manifest-path "$manifest" -i "$dep" -e normal,build 2>/dev/null | grep -q .; then
      echo "LEAK: $crate (excluded) depends on $dep"
      leaked=1
      fail=1
    fi
  done
  if [ "$leaked" -eq 0 ]; then
    echo "boundary holds: $crate (excluded) reaches none of $PORTER $AUDIO_DEVICES $GPL_TTS"
  fi
done

# Every workspace member has a row above, so a new crate cannot slip in unchecked; every
# excluded crate has a row in EXCLUDED, and is listed in the workspace's `exclude`.
for member in $(sed -n '/^members = \[/,/^\]/s#^  "crates/\(.*\)",$#\1#p' Cargo.toml); do
  printf '%s\n' "${EDGES[@]}" | grep -q "^$member:" || { echo "ERROR: $member has no row in EDGES"; fail=1; }
  printf '%s\n' "${RULES[@]}" | grep -q "^$member:" || { echo "ERROR: $member has no row in RULES"; fail=1; }
done

for member in $(sed -n '/^exclude = \[/,/^\]/s#^  "crates/\(.*\)",$#\1#p' Cargo.toml); do
  printf '%s\n' "${EXCLUDED[@]}" | grep -q "^$member:" || { echo "ERROR: $member has no row in EXCLUDED"; fail=1; }
done

exit "$fail"
