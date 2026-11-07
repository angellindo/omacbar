#!/usr/bin/env bash
# R9: Rust source files stay under 800 non-test lines (warn above 500).
# Counts lines before the first `#[cfg(test)]`; files named tests.rs / *_tests.rs and tests/ dirs are skipped.
set -euo pipefail
cd "$(dirname "$0")/.."
status=0
while IFS= read -r f; do
  n=$(awk '/^#\[cfg\(test\)\]/{exit} {c++} END{print c+0}' "$f")
  if (( n > 800 )); then echo "ERROR $f: $n lines (cap 800)"; status=1
  elif (( n > 500 )); then echo "warn  $f: $n lines (aim for < 500)"; fi
done < <(find crates -name '*.rs' -not -path '*/tests/*' -not -name 'tests.rs' -not -name '*_tests.rs')
exit $status
