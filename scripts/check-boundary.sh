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
# vision-prep reaches the image stack only through its `pixels` feature; the two io crates
# (model-http, model-openai-compat) may reach an HTTP stack and a runtime but nothing below.
# Nothing here reaches porter: stoker is portable and has no porter dependency.
PORTER="porter-core porter-infer porter-client porter-provider"
EFFECTS="tokio hyper hyper-util rustls zbus zvariant reqwest ureq wayland-client wayland-backend reis atspi oo7 ort fastembed rusqlite notify cedar-policy rmcp"
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
  "model-openai-compat: $IO_FORBIDDEN $PORTER"
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
  "model-provider: cua-action vision-prep"
  "cua-parse: cua-action model-provider"
  "cua-vendors: cua-action cua-parse model-provider"
  "cua-session: cua-action cua-parse cua-vendors model-provider vision-prep"
  "model-replay: model-provider vision-prep"
  "model-catalog: model-provider"
  "engine-supervisor: model-catalog"
  "model-http:"
  "model-openai-compat: model-http model-provider"
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

# Every workspace member has a row above, so a new crate cannot slip in unchecked.
for member in $(sed -n 's#^  "crates/\(.*\)",$#\1#p' Cargo.toml); do
  printf '%s\n' "${EDGES[@]}" | grep -q "^$member:" || { echo "ERROR: $member has no row in EDGES"; fail=1; }
  printf '%s\n' "${RULES[@]}" | grep -q "^$member:" || { echo "ERROR: $member has no row in RULES"; fail=1; }
done

exit "$fail"
