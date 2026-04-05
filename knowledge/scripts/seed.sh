#!/usr/bin/env bash
# Seed the k-stack knowledge graph from knowledge/graph/seed.jsonl.
#
# Requirements:
#   - k-stack built: (cd k-stack && cargo build --release)
#   - ket CLI on PATH, or edit KET below to point at k-stack binary wrapper
#   - jq
#
# Usage:
#   KET_HOME=.ket ./knowledge/scripts/seed.sh
#
# This script is the *authoritative* way to re-materialize the knowledge graph.
# If histories diverge, re-run this against the current tree and compare CIDs
# against a peer's output. Divergent CIDs pinpoint divergent content.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
MANIFEST="$ROOT/knowledge/graph/seed.jsonl"
export KET_HOME="${KET_HOME:-$ROOT/.ket}"

KET="${KET:-ket}"
if ! command -v "$KET" >/dev/null 2>&1; then
  echo "error: '$KET' not on PATH. Install ket-cli or set KET=<path-to-binary>." >&2
  echo "       k-stack exposes MCP tools; for shell seeding install https://github.com/nickjoven/ket" >&2
  exit 1
fi

declare -A CID_BY_ID

resolve_parents() {
  local parents_json="$1"
  local cids=()
  while IFS= read -r pid; do
    [ -z "$pid" ] && continue
    if [ -z "${CID_BY_ID[$pid]:-}" ]; then
      echo "error: parent '$pid' not yet stored (check ordering in seed.jsonl)" >&2
      exit 1
    fi
    cids+=("${CID_BY_ID[$pid]}")
  done < <(echo "$parents_json" | jq -r '.[]?')
  printf '%s\n' "${cids[@]}"
}

while IFS= read -r line; do
  [ -z "$line" ] && continue
  kind=$(echo "$line" | jq -r '.kind')
  id=$(echo "$line" | jq -r '.id // .ref')
  parents_json=$(echo "$line" | jq -c '.parents // []')

  case "$kind" in
    schema|decision)
      ref=$(echo "$line" | jq -r '.ref')
      content=$(cat "$ROOT/knowledge/$ref")
      ;;
    invariant)
      content=$(echo "$line" | jq -c '{id,statement,penalty}')
      ;;
    *)
      content="$line"
      ;;
  esac

  parent_args=()
  while IFS= read -r cid; do
    [ -n "$cid" ] && parent_args+=(--parent "$cid")
  done < <(resolve_parents "$parents_json")

  node_cid=$("$KET" store --kind "$kind" --agent "seed.sh" \
              "${parent_args[@]}" --stdin <<<"$content" | jq -r '.node_cid')

  CID_BY_ID[$id]="$node_cid"
  printf '%-10s  %-34s  %s\n' "$kind" "$id" "$node_cid"
done < "$MANIFEST"

echo
echo "Knowledge graph seeded. KET_HOME=$KET_HOME"
echo "Snapshot CIDs and commit them alongside seed.jsonl for divergence detection."
