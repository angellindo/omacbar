#!/usr/bin/env bash
# Keep every agent reading the same instructions (AGENTS.md "Mirrors").
# Source side: CLAUDE.md, .claude/agents/, .claude/skills/. Mirror side: AGENTS.md, .agents/.
# Usage: scripts/sync-agents.sh          copy source → mirror
#        scripts/sync-agents.sh --check  exit 1 if the mirror differs (used by scripts/check.sh)
set -euo pipefail
cd "$(dirname "$0")/.."

pairs=("CLAUDE.md:AGENTS.md" ".claude/agents:.agents/agents" ".claude/skills:.agents/skills")

if [[ "${1:-}" == "--check" ]]; then
  drift=0
  for p in "${pairs[@]}"; do
    src=${p%%:*} dst=${p##*:}
    if [[ -L "$dst" ]] || ! diff -rq "$src" "$dst" >/dev/null 2>&1; then
      echo "drift: $dst differs from $src" >&2; drift=1
    fi
  done
  (( drift == 0 )) || { echo "run scripts/sync-agents.sh and commit both sides" >&2; exit 1; }
  echo "agent mirrors in sync"
  exit 0
fi

mkdir -p .agents
for p in "${pairs[@]}"; do
  src=${p%%:*} dst=${p##*:}
  rm -rf "$dst"
  cp -r "$src" "$dst"
  echo "synced $src → $dst"
done
