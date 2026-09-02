#!/usr/bin/env bash
# Hydrate the dev corpora listed in manifest.tsv, verifying sha256.
#
# Two tiers. The corpora manifest.tsv pins are COMMITTED: they are the test
# tier, and CI has no R2 credentials, so a clone must be able to run sous's
# tests without this script. The 1500-bible calibration fleet is NOT committed
# — it lands in calibration-corpora/ (gitignored) and is fetched.
#
# Anything already present and verifying is skipped, so this is a no-op on a
# machine that has the corpora on disk.
#
# Credentials: see ../.env.example.
#
# Fetch paths, in order of preference (creds via op run --env-file ../.env):
#   R2_ACCESS_KEY_ID set -> S3 data plane (R2_SECRET_ACCESS_KEY, an
#                           R2_* endpoint var, R2_BUCKET; no rate limits)
#   SSC_BUCKET set       -> wrangler via the Cloudflare management API
#                           (CLOUDFLARE_API_TOKEN/ACCOUNT_ID; rate-limited,
#                           fine for a one-off object)
#   SSC_CORPORA_URL set  -> plain curl from a public base URL, if one exists
set -euo pipefail
cd "$(dirname "$0")"

endpoint() { echo "${R2_ENDPOINT_URL:-${R2_S3_API:-${CLOUDFLARE_R2_S3_API:-}}}"; }

fetch() { # $1 = object name (gzipped in the bucket)
  if [[ -n ${R2_ACCESS_KEY_ID:-} ]]; then
    AWS_ACCESS_KEY_ID=$R2_ACCESS_KEY_ID \
    AWS_SECRET_ACCESS_KEY=$R2_SECRET_ACCESS_KEY \
    AWS_ENDPOINT_URL=$(endpoint) AWS_REGION=auto \
      aws s3 cp "s3://${R2_BUCKET:?}/corpora/$1.gz" - | gunzip
  elif [[ -n ${SSC_BUCKET:-} ]]; then
    npx -y wrangler r2 object get "$SSC_BUCKET/corpora/$1.gz" --pipe --remote | gunzip
  elif [[ -n ${SSC_CORPORA_URL:-} ]]; then
    curl -fsSL "$SSC_CORPORA_URL/$1.gz" | gunzip
  else
    echo "set R2_* creds (S3, preferred), SSC_BUCKET (wrangler), or SSC_CORPORA_URL (public)" >&2
    return 2
  fi
}

ok=0 fetched=0
while IFS=$'\t' read -r name bytes sha; do
  [[ $name == \#* || -z $name ]] && continue
  if [[ -e $name ]] && echo "$sha  $name" | shasum -a 256 -c --status 2>/dev/null; then
    ok=$((ok + 1)); continue
  fi
  echo "fetching $name ($bytes bytes)"
  fetch "$name" > "$name.part"
  echo "$sha  $name.part" | shasum -a 256 -c --status ||
    { echo "sha256 mismatch for $name" >&2; rm -f "$name.part"; exit 1; }
  mv "$name.part" "$name"
  fetched=$((fetched + 1))
done < manifest.tsv
echo "verified: $ok already present, $fetched fetched"
