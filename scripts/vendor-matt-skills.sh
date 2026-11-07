#!/usr/bin/env bash
# Re-vendor mattpocock/skills (engineering + productivity) into .claude/skills. Review the diff before committing.
set -euo pipefail
cd "$(dirname "$0")/.."
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
git clone --depth 1 -q https://github.com/mattpocock/skills.git "$tmp/skills"
rev=$(git -C "$tmp/skills" rev-parse HEAD)
for d in "$tmp"/skills/skills/engineering/*/ "$tmp"/skills/skills/productivity/*/; do
  n=$(basename "$d"); rm -rf ".claude/skills/$n"; cp -r "$d" ".claude/skills/$n"
done
cp "$tmp/skills/LICENSE" .claude/skills/_vendor/mattpocock-skills.LICENSE
names=$(ls -d "$tmp"/skills/skills/{engineering,productivity}/*/ | xargs -n1 basename | sort | tr '\n' ' ')
sed -i "s/^Vendored: .*/Vendored: $names/" .claude/skills/_vendor/README.md
sed -i "s/at commit \`[0-9a-f]*\`/at commit \`$rev\`/" .claude/skills/_vendor/README.md
scripts/sync-agents.sh
echo "vendored mattpocock/skills @ $rev"
