#!/usr/bin/env sh
# Builds one crate's committed wasm packages on x86_64 Linux, whatever this
# machine is: the same rustc, wasm-pack and wasm-opt CI pins, inside a
# container. LLVM lays out a few tied functions and data items in an order
# that depends on the BUILD HOST's architecture — an arm64 build differs from
# an x86_64 build by a few hundred bytes with identical content — so the
# committed .wasm, which CI gates byte for byte on x86_64, is built there too.
#
#     ./wasm-build.sh galley        # what galley/build.sh runs for you off-platform
#     ./wasm-build.sh onion-wasm
#
# Two named volumes keep the toolchain install and the registry between runs;
# the first run compiles wasm-pack and takes minutes, later runs take one.
set -eu
crate=${1:?crate directory: galley or onion-wasm}
root=$(cd "$(dirname "$0")" && pwd)
docker run --rm --platform linux/amd64 \
  -v "$root":/src \
  -v scripture-kitchen-cargo:/usr/local/cargo/registry \
  -v scripture-kitchen-cargo-bin:/usr/local/cargo/bin \
  -v scripture-kitchen-target:/tmp/target \
  -e CARGO_TARGET_DIR=/tmp/target \
  -w "/src/$crate" \
  rust:1.97.1 sh -c '
    set -eu
    rustup target add wasm32-unknown-unknown >/dev/null 2>&1
    [ -x /opt/binaryen-version_131/bin/wasm-opt ] || \
      curl -sSfL https://github.com/WebAssembly/binaryen/releases/download/version_131/binaryen-version_131-x86_64-linux.tar.gz | tar xz -C /opt
    export PATH=/opt/binaryen-version_131/bin:$PATH
    [ "$(wasm-pack --version 2>/dev/null)" = "wasm-pack 0.14.0" ] || cargo install wasm-pack --version 0.14.0 --locked -q
    WASM_BUILD_NATIVE=1 ./build.sh
  '
