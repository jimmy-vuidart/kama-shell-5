#!/usr/bin/env bash
# The code graph of the repository (codegraph, pinned here): where a subject
# lives, who calls what, what a change touches. Quicker than reading the
# crates one file after the other.
#
#   scripts/codegraph.sh explore "how a setting is reloaded while running"
#   scripts/codegraph.sh callers apply_arrangement
#   scripts/codegraph.sh impact Ratio
#   scripts/codegraph.sh help
#   scripts/codegraph.sh serve --mcp        what .mcp.json starts
#
# The index is in .codegraph/, never committed: built on first use, brought up
# to date before each question. Nothing leaves the machine.
set -euo pipefail
cd "$(dirname "$0")/.."

export CODEGRAPH_TELEMETRY=0
codegraph=(npx -y @colbymchenry/codegraph@1.6.2)

[ -f .codegraph/codegraph.db ] || "${codegraph[@]}" init --yes . >&2

case "${1:-help}" in
    # The server watches the files itself; these do not read the index.
    serve | init | uninit | index | sync | unlock | help | version | -*) ;;
    *) "${codegraph[@]}" sync . >/dev/null ;;
esac

exec "${codegraph[@]}" "$@"
