#!/usr/bin/env bash
# Refresh canon/data/names/GATEWAYS.tsv from the BIEL language API, and
# shallow-clone every source it names into target/canon-harvest/ for
# `harvest-names` to read. Run by hand when the names are being re-harvested;
# nothing in the build or the tests runs it.
#
# A gateway language's sources are its `Primary` text resources shown on BIEL.
# When an org repo (WA-Catalog, WycliffeAssociates) holds one, the source is
# one of them, the first of ulb, reg, avd, udb, blv, nav: one translation's own
# names, not two translations argued over. Otherwise every per-book Writer
# repo is a source, and the review sees where they disagree. A language with no Primary text gets a row with no sources
# and falls back to English.
set -euo pipefail
cd "$(dirname "$0")/../.."

api=https://api.bibleineverylanguage.org/v1/graphql
host=https://content.bibletranslationtools.org
out=canon/data/names/GATEWAYS.tsv
cache=target/canon-harvest

query='{ content(where: {
    language: { wa_language_metadata: { is_gateway: { _eq: true } } }
    wa_content_metadata: { show_on_biel: { _eq: true } } }) {
  resource_type
  language { ietf_code english_name national_name }
  git_repo { username repo_name }
  wa_content_metadata { status } } }'

json=$(jq -n --arg q "$query" '{query: $q}' |
  curl -fsS -X POST "$api" -H 'content-type: application/json' --data @-)

{
  echo "# The gateway languages whose book names canon harvests. Written by"
  echo "# canon/gen/fetch-gateways.sh from the BIEL language API; do not edit."
  echo "# locale	english_name	national_name	sources (owner/repo, comma-separated)"
  jq -r '
    def ranks: ["ulb", "reg", "avd", "udb", "blv", "nav"];
    def text: . as $t | ranks | index($t) != null;
    def org: . == "WA-Catalog" or . == "WycliffeAssociates";
    .data.content
    | group_by(.language.ietf_code)[]
    | . as $rows
    | ([$rows[]
        | select(.wa_content_metadata.status == "Primary" and .git_repo != null
                 and (.resource_type | text))
        | .git_repo + {rank: (.resource_type as $t | ranks | index($t))}]) as $repos
    | ([$repos[] | select(.username | org)]) as $orgs
    | (if ($orgs | length) > 0 then [$orgs | sort_by(.rank) | .[0]] else $repos end)
    | map("\(.username)/\(.repo_name)") | unique | join(",") as $sources
    | [$rows[0].language.ietf_code, $rows[0].language.english_name,
       $rows[0].language.national_name, $sources]
    | @tsv' <<<"$json"
} >"$out"

mkdir -p "$cache"
grep -v '^#' "$out" | cut -f4 | tr ',' '\n' | grep -v '^$' | sort -u |
  while read -r repo; do
    if [[ -d $cache/$repo ]]; then continue; fi
    echo "cloning $repo"
    git clone -q --depth 1 "$host/$repo" "$cache/$repo" ||
      echo "  failed: $repo" >&2
  done
echo "$(grep -vc '^#' "$out") languages in $out; sources in $cache"
