#!/usr/bin/env bash
# Crate layers, mechanically enforced (ARCHITECTURE.md section 1 is the table; this is its
# mechanical form).
#
#   scripts/check-boundary.sh               run every check
#   scripts/check-boundary.sh --long-files  also report source files over 500 lines (no failure yet)
#   scripts/check-boundary.sh --self-test   seed a std::fs call in a P crate and assert it fails
#
# Every crate is one tier:
#   P  pure: no std::fs/env/process/net, no clock reads, no sleeps, no tokio, hyper, reqwest, zbus
#      or rustix, and no UI words.
#   C  client/IO: may do IO, but has no UI words (Label, Help, Sheet, SettingRow).
#   A  app/daemon: wiring only.
# A crate depends on its own tier or a lower one (P < C < A), never up.
#
# `cargo tree -i <dep>` exits 101 when the dependency is absent, which is precisely the state
# we want. Checking the exit status would therefore fail whenever the boundary holds, so we
# check for OUTPUT instead: any line naming the dependency is a leak.
set -uo pipefail
cd "$(dirname "$0")/.."

# LAYERS: crate -> tier. Every workspace member and every excluded crate has exactly one row.
LAYERS=(
  "cua-action: P"
  "vision-prep: P"
  "cua-parse: P"
  "cua-vendors: P"
  "cua-session: P"
  "genai-names: P"
  "model-provider: P"
  "model-extract: P"
  "model-catalog: P"
  "engine-supervisor: P"
  "model-wire: P"
  "model-openai-compat: P"
  "model-replay: P"
  "speech-provider: P"
  "speech-vad: P"
  # The Transport trait and the wire types are pure; the reqwest client and TLS sit behind the
  # `client` and `tls` features in the same crate, so it is a P crate with KNOWN rows until the
  # client moves to its own crate (T1.2).
  "model-http: P"
  "speech-host-client: C"
  "speech-host: C"
  # Excluded from the workspace (they need a runtime outside the pinned block).
  "speech-vad-silero: C"
  "speech-host-sherpa: A"
  # The fuzz targets of cua-parse (nightly only, their own `[workspace]` table).
  "cua-parse/fuzz: A"
)

# KNOWN: a violation that exists today, with the plan lane (~/rs-wt/agent-layers/PLAN.md) that
# removes it. "crate: ban: lane". A row whose violation is gone fails, so the list only shrinks.
# model-http: the feature-gated reqwest client (connect.rs, request.rs, transport.rs).
KNOWN=(
  "model-http: reqwest: T1.2"
  "model-http: tokio: T1.2"
)

# BANS: "name|tiers|extended regex" over the non-test source of a crate of those tiers. A line
# that starts with `//` is skipped, and so is everything from the first `#[cfg(test)]` on.
BANS=(
  'fs|P|std::(fs|\{[^}]*\bfs\b)'
  'env|P|std::(env|\{[^}]*\benv\b)'
  'process|P|std::(process|\{[^}]*\bprocess\b)'
  'net|P|std::(net|os::unix::net|\{[^}]*\bnet\b)'
  'clock|P|SystemTime::now|Instant::now'
  'sleep|P|thread::sleep'
  'tokio|P|\btokio\b'
  'hyper|P|\bhyper\b'
  'reqwest|P|\breqwest\b'
  'zbus|P|\bzbus\b'
  'rustix|P|\brustix\b'
  'ui-words|PC|\b(Label|Help|Sheet|SettingRow)\b|ds-settings'
)

# RULES: what a crate reaches through ANY path (transitive, default features). The P crates
# never reach a runtime, an HTTP client, a bus, a compositor, a database or an inference runtime;
# vision-prep reaches the image stack only through its `pixels` feature; the one C crate with an
# HTTP stack (model-http) may reach a runtime but nothing below. The codecs (model-wire,
# model-openai-compat) and model-extract are pure over the Transport trait, so when
# `HttpClient`'s transport is used it sits behind a feature that only a daemon turns on, and these rows
# keep checking default features. Nothing here reaches porter: stoker is portable.
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
  "speech-host: $EFFECTS $PORTER $AUDIO_DEVICES $GPL_TTS"
)
# The excluded crates may reach `ort` and `sherpa-onnx`, which is why they are excluded; they
# reach no porter crate, no audio device crate and no GPL TTS stack.
EXCLUDED_FORBIDDEN="$PORTER $AUDIO_DEVICES $GPL_TTS"

tier_of() {
  local row
  for row in "${LAYERS[@]}"; do
    [ "${row%%:*}" = "$1" ] && { echo "${row#*: }"; return; }
  done
}
rank() { case "$1" in P) echo 0 ;; C) echo 1 ;; A) echo 2 ;; *) echo 9 ;; esac; }

# Prints "file:line: text" for each non-test source line of crate $2 under root $1 that matches
# regex $3.
scan_source() {
  local root="$1" crate="$2" regex="$3" file
  [ -d "$root/crates/$crate/src" ] || return 0
  while IFS= read -r file; do
    sed '/^[[:space:]]*#\[cfg(test)\]/q' "$file" | grep -nE "$regex" \
      | grep -vE '^[0-9]+:[[:space:]]*//' | sed "s#^#$file:#"
  done < <(find "$root/crates/$crate/src" -name '*.rs' ! -name tests.rs ! -path '*/tests/*' | sort)
}

# Runs every source ban on crate $2 under root $1; prints "ban: hit" lines.
source_violations() {
  local root="$1" crate="$2" tier spec name tiers regex
  tier=$(tier_of "$crate")
  for spec in "${BANS[@]}"; do
    name="${spec%%|*}"; spec="${spec#*|}"
    tiers="${spec%%|*}"; regex="${spec#*|}"
    case "$tiers" in *"$tier"*) ;; *) continue ;; esac
    scan_source "$root" "$crate" "$regex" | sed "s#^#$name: #"
  done
}

known_lane() { # crate ban -> lane, empty when not listed
  local row
  for row in "${KNOWN[@]}"; do
    case "$row" in "$1: $2: "*) echo "${row##*: }"; return ;; esac
  done
}

if [ "${1:-}" = "--self-test" ]; then
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT
  mkdir -p "$tmp/crates/model-provider/src" "$tmp/crates/speech-host-client/src"
  printf 'pub fn f() { let _ = std::fs::read("x"); }\n' >"$tmp/crates/model-provider/src/lib.rs"
  printf 'pub fn f() { let _ = std::fs::read("x"); }\n' >"$tmp/crates/speech-host-client/src/lib.rs"
  p_hits=$(source_violations "$tmp" model-provider)
  c_hits=$(source_violations "$tmp" speech-host-client)
  if ! grep -q '^fs: ' <<<"$p_hits"; then
    echo "SELF-TEST FAILED: a std::fs call in a P crate was not caught"
    exit 1
  fi
  if [ -n "$c_hits" ]; then
    echo "SELF-TEST FAILED: a std::fs call in a C crate was reported"
    exit 1
  fi
  echo "self-test ok: std::fs in a P crate fails, in a C crate passes"
  exit 0
fi

fail=0
long_files=0
[ "${1:-}" = "--long-files" ] && long_files=1

members=$(sed -n '/^members = \[/,/^\]/s#^  "crates/\(.*\)",$#\1#p' Cargo.toml)
excluded=$(sed -n '/^exclude = \[/,/^\]/s#^  "crates/\(.*\)",$#\1#p' Cargo.toml)

# Every crate has a row in LAYERS, and every row names a crate, so a new crate cannot slip in
# unchecked. The fuzz targets are nested, so they are named here by their path under `crates/`.
layer_rows=$(printf '%s\n' "${LAYERS[@]}" | grep -v '^#')
for crate in $members $excluded cua-parse/fuzz; do
  grep -q "^$crate: [PCA]\$" <<<"$layer_rows" || { echo "ERROR: $crate has no row in LAYERS"; fail=1; }
done
for row in "${LAYERS[@]}"; do
  crate="${row%%:*}"
  [ -f "crates/$crate/Cargo.toml" ] || { echo "ERROR: LAYERS names $crate, which has no crates/$crate/Cargo.toml"; fail=1; }
done

# Crate dependency graph, from the manifests alone: a crate's DIRECT normal and build path
# dependencies. An upstream that cannot be resolved today (sherpa-onnx-sys asks for a `zip` 2
# that crates.io has yanked whole) cannot hide an edge.
direct_path_deps() {
  if grep -qx "$1" <<<"$members"; then
    cargo tree -p "$1" --depth 1 -e normal,build --prefix none --all-features 2>/dev/null \
      | grep '(/' | awk '{print $1}' | grep -vx "$1" | sort -u
  else
    cargo metadata --manifest-path "crates/$1/Cargo.toml" --format-version 1 --no-deps 2>/dev/null \
      | python3 -c 'import json,sys
for d in json.load(sys.stdin)["packages"][0]["dependencies"]:
    if d.get("path") and d["kind"] in (None, "build"): print(d["name"])' | sort -u
  fi
}

# Dependencies point down only.
for row in "${LAYERS[@]}"; do
  crate="${row%%:*}"
  tier=$(tier_of "$crate")
  deps=$(direct_path_deps "$crate")
  bad=0
  for dep in $deps; do
    dep_tier=$(tier_of "$dep")
    if [ -z "$dep_tier" ]; then
      echo "LAYER: $crate depends on $dep, which has no row in LAYERS"; bad=1; fail=1
    elif [ "$(rank "$dep_tier")" -gt "$(rank "$tier")" ]; then
      echo "LAYER: $crate ($tier) depends on $dep ($dep_tier), which is above it"; bad=1; fail=1
    fi
  done
  [ "$bad" -eq 0 ] && echo "layers hold: $crate ($tier) depends only on its own tier or below"
done

# Source bans, with the KNOWN list.
for row in "${LAYERS[@]}"; do
  crate="${row%%:*}"
  hits=$(source_violations . "$crate")
  seen=""
  while IFS= read -r hit; do
    [ -n "$hit" ] || continue
    ban="${hit%%: *}"
    lane=$(known_lane "$crate" "$ban")
    if [ -n "$lane" ]; then
      echo "KNOWN: $crate ($ban) is removed by lane $lane: ${hit#*: }"
      seen="$seen $ban"
    else
      echo "BAN: $crate ($(tier_of "$crate")): $hit"
      fail=1
    fi
  done <<<"$hits"
  for k in "${KNOWN[@]}"; do
    [ "${k%%:*}" = "$crate" ] || continue
    k_ban=$(cut -d: -f2 <<<"$k" | tr -d ' ')
    grep -qw "$k_ban" <<<"$seen" || { echo "STALE: KNOWN names $k_ban in $crate, which no longer breaks it; delete the row"; fail=1; }
  done
  [ -z "$hits" ] && echo "bans hold: $crate ($(tier_of "$crate")) source is clean"
done

# Source files over 500 lines: reported on request, not a failure yet.
if [ "$long_files" -eq 1 ]; then
  find crates -name '*.rs' -path '*/src/*' -not -path '*/target/*' -print0 \
    | xargs -0 wc -l | awk '$1 > 500 && $2 != "total" {print "LONG: " $2 " has " $1 " lines"}'
fi

for row in "${RULES[@]}"; do
  crate="${row%%:*}"
  read -r -a forbidden <<<"${row#*:}"
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

# The excluded crates are checked from their own manifests.
for crate in $excluded cua-parse/fuzz; do
  manifest="crates/$crate/Cargo.toml"
  leaked=0
  # The whole tree when it resolves. When it does not, say so and check what the manifest names
  # directly: the path dependencies are workspace members whose own trees the rows above check,
  # so only the registry dependencies named here are left to look at.
  if cargo tree --manifest-path "$manifest" --depth 0 >/dev/null 2>&1; then
    reaches() { cargo tree --manifest-path "$manifest" -i "$1" -e normal,build 2>/dev/null | grep -q .; }
  else
    echo "NOTE: $crate (excluded) does not resolve today; its leak check covers its direct dependencies only"
    direct=$(cargo metadata --manifest-path "$manifest" --format-version 1 --no-deps 2>/dev/null \
      | python3 -c 'import json,sys
for d in json.load(sys.stdin)["packages"][0]["dependencies"]:
    if d["kind"] in (None, "build"): print(d["name"])')
    reaches() { printf '%s\n' "$direct" | grep -qx "$1"; }
  fi
  for dep in $EXCLUDED_FORBIDDEN; do
    if reaches "$dep"; then
      echo "LEAK: $crate (excluded) depends on $dep"
      leaked=1
      fail=1
    fi
  done
  if [ "$leaked" -eq 0 ]; then
    echo "boundary holds: $crate (excluded) reaches none of $EXCLUDED_FORBIDDEN"
  fi
done

exit "$fail"
