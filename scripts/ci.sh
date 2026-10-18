#!/usr/bin/env bash
# Local CI: formatting, lints, tests. Run from anywhere in the repository.
set -euo pipefail
cd "$(dirname "$0")/.."

# When the output is not a terminal (a coding agent reading it), clippy and the
# tests go through rtk, which keeps errors and failures and drops the rest; the
# full log is saved and its path printed. CI_RAW=1 keeps cargo's own output.
cargo=(cargo)
if [[ ! -t 1 && -z ${CI_RAW:-} ]] && command -v rtk >/dev/null; then
    cargo=(rtk cargo)
fi

cargo fmt --all -- --check
"${cargo[@]}" clippy --workspace --all-targets -- -D warnings
"${cargo[@]}" test --workspace
