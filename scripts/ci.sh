#!/usr/bin/env bash
# Local CI: formatting, lints, tests. Run from anywhere in the repository.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
