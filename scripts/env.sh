#!/usr/bin/env bash
# Source this file to use Play Anything's project-local toolchain and media backends.
PLAY_ANYTHING_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export PLAY_ANYTHING_ROOT
export PLAY_ANYTHING_TOOLS="${PLAY_ANYTHING_TOOLS:-$PLAY_ANYTHING_ROOT/.tools}"

if [[ -d "$PLAY_ANYTHING_TOOLS/cargo-home" ]]; then
    export CARGO_HOME="${CARGO_HOME:-$PLAY_ANYTHING_TOOLS/cargo-home}"
fi
if [[ -d "$PLAY_ANYTHING_TOOLS/rustup" ]]; then
    export RUSTUP_HOME="${RUSTUP_HOME:-$PLAY_ANYTHING_TOOLS/rustup}"
fi

export PATH="$PLAY_ANYTHING_TOOLS/bin:$PLAY_ANYTHING_TOOLS/rust/bin:$PLAY_ANYTHING_TOOLS/cargo-home/bin:$PATH"
