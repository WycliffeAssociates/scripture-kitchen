#!/usr/bin/env sh
# The ONE way to build the committed `usfm-galley` packages. Run this, never
# the raw wasm-pack lines: the RUSTFLAGS below are part of the output.
#
#     ./galley/build.sh
#
# Panic locations from dependency crates and from std embed the absolute path
# of the tree that built them — a registry checkout under $CARGO_HOME, and the
# rustup sysroot, whose name carries the host triple. Both differ on every
# machine. Remapping the two roots makes the build host-independent, which is
# what lets CI gate the BINARY and not merely its interface.
#
# `--features wasm` is a cargo argument, so it rides after the `--`.
set -eu
cd "$(dirname "$0")"

# The committed .wasm is an x86_64-Linux build (see ../wasm-build.sh: LLVM's
# layout of tied items depends on the build host's architecture). Off that
# platform this script hands itself to the container and stops.
if [ -z "${WASM_BUILD_NATIVE:-}" ] && [ "$(uname -s)-$(uname -m)" != "Linux-x86_64" ]; then
  exec ../wasm-build.sh "$(basename "$(pwd)")"
fi

sysroot=$(rustc --print sysroot)
cargo_home=${CARGO_HOME:-$HOME/.cargo}
# Generic std code instantiated in this crate carries std's source paths. With
# the rust-src component installed rustc resolves them to the local copy under
# the sysroot; without it they stay in std's own virtual form, /rustc/<commit>.
# CI has no rust-src, so the local copy is mapped onto that form and both hosts
# spell every std path the same way.
commit=$(rustc -vV | sed -n 's/^commit-hash: //p')
RUSTFLAGS="--remap-path-prefix=$cargo_home=/cargo --remap-path-prefix=$sysroot=/rust --remap-path-prefix=$sysroot/lib/rustlib/src/rust=/rustc/$commit${RUSTFLAGS:+ $RUSTFLAGS}"
export RUSTFLAGS

# wasm-opt is an input to the bytes too: CI pins binaryen version_131
# (.github/actions/wasm-tools), so this build refuses any other version rather
# than commit a .wasm the byte gate will reject.
want=131
have=$(wasm-opt --version 2>/dev/null | sed -n 's/.*version \([0-9][0-9]*\).*/\1/p')
if [ "$have" != "$want" ]; then
  echo "wasm-opt version_$want required (CI pins it); found ${have:-none}." >&2
  echo "Download binaryen-version_$want for this host from github.com/WebAssembly/binaryen/releases and put its bin/ first on PATH." >&2
  exit 1
fi

wasm-pack build --target web     --release --weak-refs --out-dir pkg-web     -- --features wasm
wasm-pack build --target bundler --release --weak-refs --out-dir pkg-bundler -- --features wasm

# wasm-pack writes a `*` .gitignore into each out-dir; these are committed.
rm -f pkg-web/.gitignore pkg-bundler/.gitignore
