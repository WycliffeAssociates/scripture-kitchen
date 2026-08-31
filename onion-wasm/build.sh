#!/usr/bin/env sh
# The ONE way to build the committed packages. Run this, never the raw
# wasm-pack lines: the RUSTFLAGS below are part of the output.
#
#     ./onion-wasm/build.sh
#
# Panic locations from dependency crates and from std embed the absolute path
# of the tree that built them — a registry checkout under $CARGO_HOME, and the
# rustup sysroot, whose name carries the host triple. Both differ on every
# machine, so the same source produced a different .wasm on macOS and on a
# Linux runner, and the committed binary shipped the builder's home directory
# to consumers. Remapping the two roots makes the build host-independent, which
# is what lets CI gate the BINARY and not merely its interface.
set -eu
cd "$(dirname "$0")"

sysroot=$(rustc --print sysroot)
cargo_home=${CARGO_HOME:-$HOME/.cargo}
RUSTFLAGS="--remap-path-prefix=$cargo_home=/cargo --remap-path-prefix=$sysroot=/rust${RUSTFLAGS:+ $RUSTFLAGS}"
export RUSTFLAGS

wasm-pack build --target web     --release --weak-refs --out-dir pkg-web
wasm-pack build --target bundler --release --weak-refs --out-dir pkg-bundler

# wasm-pack writes a `*` .gitignore into each out-dir; these are committed.
rm -f pkg-web/.gitignore pkg-bundler/.gitignore
