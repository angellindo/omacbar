#!/usr/bin/env bash
# All mechanical gates. Agents run this before moving a ticket to In Review; it must exit 0.
# Usage: scripts/check.sh [--quick]   (--quick skips the GPU functional tests)
set -euo pipefail
cd "$(dirname "$0")/.."

step() { printf '\n\033[1m== %s\033[0m\n' "$*"; }

step "agent mirrors (CLAUDE.md = AGENTS.md, .claude = .agents)"
scripts/sync-agents.sh --check

step "script tests (python unittest, no cargo)"
python3 -B -m unittest discover -s scripts/tests -p 'test_*.py'

step "fmt"
cargo fmt --all -- --check

step "clippy (warnings are errors)"
cargo clippy --workspace --all-targets --locked -- -D warnings

step "tests"
if [[ "${1:-}" == "--quick" ]]; then
  cargo test --workspace --locked --lib --bins
else
  cargo test --workspace --locked
fi

step "file size (R9: 500 soft / 800 hard non-test lines)"
scripts/file-size.sh

printf '\n\033[32mall gates passed\033[0m\n'
