#!/usr/bin/env bash
# Rebuilds the input trees of research 0012 (TB-51) from public revisions plus the files next to
# this script. It builds nothing and runs no test; the cargo commands per tree are in 0012 § Reproduce.
#
# Usage: reconstruct.sh <app-git> <rune-pin-git> <rune-head-git> <out-dir>
#   <app-git>        any clone of github.com/angellindo/omacbar holding APP below
#   <rune-pin-git>   any clone of github.com/anomalyco/rune holding P below
#   <rune-head-git>  any clone of github.com/anomalyco/rune holding H below (may be the same clone)
#   <out-dir>        new or empty directory
# Output: <out-dir>/proto/{rune-pin,app-A1,app-A2,app-B1,app-B2} and <out-dir>/src/rune-head, laid
# out so the relative paths in app-cargo-patch.toml / app-head-cargo.patch resolve, and one
# "tree <name> <git tree id>" line per tree (compare with 0012 § Reproduce).
set -euo pipefail

APP=905dad2b6364e0d913a3c60befa4cfe7395dfb72
P=f73f72cdc563534e4f7de9f5a79bed007ac42384
H=22ebd7e7f5ceb189e1829f066b5ba0fe31ed5598

[[ $# -eq 4 ]] || { sed -n '5,10p' "$0"; exit 2; }
FIX=$(cd "$(dirname "$0")" && pwd)
APP_GIT=$1 PIN_GIT=$2 HEAD_GIT=$3
mkdir -p "$4" && OUT=$(cd "$4" && pwd)
[[ -z $(ls -A "$OUT") ]] || { echo "out-dir not empty: $OUT" >&2; exit 2; }

(cd "$FIX" && sha256sum --quiet -c SHA256SUMS)

snap() { # <git> <sha> <dest>: the tree of that exact commit, nothing else
  [[ $(git -C "$1" rev-parse --verify "$2^{commit}") == "$2" ]]
  mkdir -p "$3" && git -C "$1" archive --format=tar "$2" | tar -x -C "$3"
}
apply() { (cd "$1" && patch -p1 -s --no-backup-if-mismatch < "$FIX/$2"); }
tree_id() { # <dir>: git tree id of every file under it (ignored ones too), without touching it
  local g; g=$(mktemp -d)
  git init -q --bare "$g"
  GIT_DIR=$g GIT_WORK_TREE=$1 GIT_INDEX_FILE=$g/index git add -A -f
  GIT_DIR=$g GIT_INDEX_FILE=$g/index git write-tree
  rm -rf "$g"
}

# Rune: H as fetched; P plus the opt-in patch (§3 of 0012 and its Rune unit test).
snap "$HEAD_GIT" "$H" "$OUT/src/rune-head"
snap "$PIN_GIT" "$P" "$OUT/proto/rune-pin"
apply "$OUT/proto/rune-pin" rune-cancel-drag-on-leave.patch

# A1 (control) and A2: APP on the patched pin, T and the public-API checks added.
A1=$OUT/proto/app-A1
snap "$APP_GIT" "$APP" "$A1"
cat "$FIX/app-cargo-patch.toml" >> "$A1/Cargo.toml"
cp "$FIX/window_leave_in_capture.rs" "$FIX/rune_cancel_drag_on_leave.rs" "$A1/crates/tb-daemon/tests/"
cp -a "$A1" "$OUT/proto/app-A2"
apply "$OUT/proto/app-A2" sim-window-opt-in.patch

# B1 (control) and B2: APP on H (Rune and its 8 vendored crates as paths, H's lock), T added.
B1=$OUT/proto/app-B1
snap "$APP_GIT" "$APP" "$B1"
apply "$B1" app-head-cargo.patch
apply "$B1" app-head-Cargo.lock.patch
cp "$FIX/window_leave_in_capture.rs" "$B1/crates/tb-daemon/tests/"
cp -a "$B1" "$OUT/proto/app-B2"
apply "$OUT/proto/app-B2" sim-window-head-hover-tracks-bounds.patch

for t in src/rune-head proto/rune-pin proto/app-A1 proto/app-A2 proto/app-B1 proto/app-B2; do
  echo "tree ${t##*/} $(tree_id "$OUT/$t")"
done
