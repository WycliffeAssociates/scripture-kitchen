#!/usr/bin/env sh
# .claude/skills/verify-kitchen/doctor.sh — read-only: is galley/pkg-node worth driving?
set -eu
root=$(cd "$(dirname "$0")/../../.." && pwd)
pkg="$root/galley/pkg-node/usfm_galley_bg.wasm"
[ -f "$pkg" ] || { echo "doctor: no $pkg; run the Launch step"; exit 1; }
stale=$(find "$root/galley/src" "$root/onion/src" "$root/sous-chef/core/src" "$root/mise/src" \
  -name '*.rs' -newer "$pkg" | head -5)
if [ -n "$stale" ]; then
  echo "doctor: pkg-node is older than:"; echo "$stale"; echo "run the Launch step"; exit 1
fi
node -e "const g=require('$root/galley/pkg-node/usfm_galley.js'); new g.Galley(); console.log('doctor: ok, node ' + process.version)"
