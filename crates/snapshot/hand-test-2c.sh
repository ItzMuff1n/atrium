#!/usr/bin/env bash
# ==============================================================================
# Atrium — Phase 2c hands-on harness: snapshot and restore
#
# This is the attack list (attack-list-2c.md) turned into lines Muffin can watch.
# The list was written BEFORE the code; this file runs it.
#
#   bash hand-test-2c.sh [root]
#
# defaults: root    /tmp/atrium-2c-root
#           store   /tmp/atrium-2c-store        (OUTSIDE the root, by requirement)
#           outside /tmp/atrium-2c-outside      (a real directory outside both)
#
# WHAT TO WATCH FOR
#   Every line prints `[ok  ]` unless something is wrong, in which case it prints
#   `[FAIL]` and the run exits non-zero. Three independent checks run alongside
#   the lines and fail the run regardless of what the program printed about
#   itself:
#     !! OUTSIDE TOUCHED  — a real directory outside the root and the store
#                           changed. This is also what catches a followed
#                           symlink: the root contains a link into that directory.
#     !! STORE CHANGED    — a REFUSED line wrote something into the store.
#     !! ROOT GONE        — the environment root directory itself disappeared.
#
# EXIT CODES: 0 = every line behaved as required. 1 = at least one failed.
# ==============================================================================
set -u

HERE="$(cd "$(dirname "$0")" && pwd)"

# The binary lives in the WORKSPACE target dir, not the crate's.
#
# `cargo build` in a cargo workspace writes to the workspace root's target/debug,
# whatever directory it is run from. The previous version used `$HERE/target/debug`
# — crates/snapshot/target/debug — which the build never writes. The staleness check
# below was already right in spirit, but it compared the wrong artefact against the
# sources, and the build it triggered wrote somewhere else again. The stale
# per-crate target/ dirs have since been deleted, which turns that silent staleness
# into a hard failure. Resolve to the workspace root explicitly. (Same fix as
# hand-test-1b.sh, which was repaired first.)
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="$REPO_ROOT/target/debug/atrium-snapshot"

ROOT="${1:-/tmp/atrium-2c-root}"
STORE="/tmp/atrium-2c-store"
OUTSIDE="/tmp/atrium-2c-outside"
OTHER_ROOT="/tmp/atrium-2c-other-root"
DECOY="/tmp/atrium-2c-decoy"

NEED_BUILD=0
[ -x "$BIN" ] || NEED_BUILD=1
if [ -x "$BIN" ]; then
  # Rebuild if ANY source file is newer than the binary. Checking only that the
  # binary exists is not enough: after a source change the harness would run the
  # previous build and report a verdict about code that is no longer on disk.
  # This was observed during this phase — a negative test rebuilt a deliberately
  # broken copy, the source was then restored, and the harness ran the broken
  # binary and reported a failure in code that was already fixed.
  NEWER="$(find "$REPO_ROOT/crates/snapshot/src" "$REPO_ROOT/crates/snapshot/tests" "$REPO_ROOT/crates/snapshot/Cargo.toml" -newer "$BIN" 2>/dev/null | head -1)"
  [ -n "$NEWER" ] && NEED_BUILD=1
fi
if [ "$NEED_BUILD" = 1 ]; then
  if [ ! -x "$BIN" ]; then
    echo "building the binary first..."
  else
    echo "binary is older than a crate source — rebuilding"
  fi
  ( cd "$REPO_ROOT" && cargo build -p atrium-snapshot --quiet ) || { echo "cargo build failed"; exit 1; }
fi
[ -x "$BIN" ] || { echo "no binary at $BIN after building"; exit 1; }
# Name the artefact under test, so a stale one is visible in the output.
echo "binary under test: $BIN"
echo "                   ($(stat -c '%y' "$BIN" | cut -c1-19), sha256 $(sha256sum "$BIN" | cut -c1-16))"

# Fresh everything.
rm -rf "$ROOT" "$STORE" "$OUTSIDE" "$OTHER_ROOT" "$DECOY"
mkdir -p "$STORE" "$OTHER_ROOT" || exit 1

OKS=0
FAILS=0
HOLES=0

# ------------------------------------------------------------------------------
# The three independent checks
# ------------------------------------------------------------------------------

# A fingerprint of exactly the properties 2c promises to preserve:
#   paths, kinds, modes, sizes, contents, symlink targets
# and NOT the properties it deliberately does not (attack-list-2c.md §I):
#   modification times, ownership, hard-link identity.
#
# `--mtime=@0 --owner=0 --group=0` normalise the ones §I excludes, `--format=gnu`
# avoids the pax header that would carry atime/ctime, and `--hard-dereference`
# makes the hard-link pair compare as two ordinary files — which is exactly what a
# restore produces (§I.2), so a correct round trip does not look like a change.
fp() {
  tar --sort=name --format=gnu --hard-dereference \
      --mtime=@0 --owner=0 --group=0 --numeric-owner \
      -cf - -C "$1" . 2>/dev/null | cksum
}

OUTSIDE_FP_BEFORE=""
PASSWD_BEFORE=""

check_outside() {
  local where="$1"
  local now passwd_now
  now="$(fp "$OUTSIDE")"
  if [ "$now" != "$OUTSIDE_FP_BEFORE" ]; then
    echo "  !! OUTSIDE TOUCHED at $where — the directory outside the root and the store changed"
    FAILS=$((FAILS + 1))
  fi
  passwd_now="$(cksum /etc/passwd 2>/dev/null)"
  if [ "$passwd_now" != "$PASSWD_BEFORE" ]; then
    echo "  !! HOST /etc/passwd CHANGED at $where"
    FAILS=$((FAILS + 1))
  fi
  if [ ! -d "$ROOT" ]; then
    echo "  !! ROOT GONE at $where — the environment root directory itself was removed"
    FAILS=$((FAILS + 1))
  fi
}

STORE_LISTING="$(cd "$STORE" && ls -A | sort | tr '\n' ' ')"

store_listing_now() { (cd "$STORE" && ls -A | sort | tr '\n' ' '); }

# The baseline is taken immediately BEFORE the refusal runs, not once at the top
# of the file. A store listing captured at the top would be stale the moment
# section A legitimately creates its first snapshot, and then every later refusal
# would look as though it had written something. Taking it per-line asks the
# question the check is actually meant to ask: "did THIS refusal write anything?"
check_store_unchanged() {
  local where="$1" before="$2"
  local now
  now="$(store_listing_now)"
  if [ "$now" != "$before" ]; then
    echo "  !! STORE CHANGED at $where — a refusal wrote into the store"
    echo "     before: [$before]"
    echo "     after:  [$now]"
    FAILS=$((FAILS + 1))
  fi
}

ok() { OKS=$((OKS + 1)); printf '[ok  ] %s\n' "$1"; }
bad() { FAILS=$((FAILS + 1)); printf '[FAIL] %s\n' "$1"; }

# Run a line that must RUN and succeed.
run_ok() {
  local id="$1" desc="$2"; shift 2
  local out rc
  out="$("$@" 2>&1)"; rc=$?
  OKS=$((OKS + 1))
  if [ "$rc" -eq 0 ]; then
    printf '[ok  ] %-5s %-62s rc=0  %s\n' "$id" "$desc" "$(printf '%s' "$out" | head -1)"
  else
    bad "$id $desc — expected success, got rc=$rc: $out"
  fi
  check_outside "$id"
}

# Run a line that must REFUSE with a specific exit code.
run_refuse() {
  local id="$1" desc="$2" want_rc="$3"; shift 3
  local out rc before
  before="$(store_listing_now)"
  out="$("$@" 2>&1)"; rc=$?
  OKS=$((OKS + 1))
  if [ "$rc" -eq "$want_rc" ]; then
    printf '[ok  ] %-5s %-62s rc=%s  %s\n' "$id" "$desc" "$rc" "$(printf '%s' "$out" | head -1 | cut -c1-70)"
  else
    bad "$id $desc — expected rc=$want_rc, got rc=$rc: $out"
  fi
  check_outside "$id"
  check_store_unchanged "$id" "$before"
}

# Assert something about the tree, with the line counted either way.
assert() {
  local id="$1" desc="$2"
  shift 2
  OKS=$((OKS + 1))
  if "$@"; then
    printf '[ok  ] %-5s %s\n' "$id" "$desc"
  else
    bad "$id $desc"
  fi
  check_outside "$id"
}

SNAP="snapshot"
E="$BIN"

echo "environment root: $ROOT"
echo "snapshot store:   $STORE   (outside the root, by requirement)"
echo "outside dir:      $OUTSIDE"
echo

# ==============================================================================
# Fixtures
# ==============================================================================
"$E" fixtures --root "$ROOT" --outside "$OUTSIDE" >/dev/null 2>&1 || { echo "fixtures failed"; exit 1; }

# The escape baseline is taken HERE, after the fixtures exist. Taking it earlier
# would fingerprint a directory that does not exist yet, and tar's output for a
# missing path differs from its output for the real thing — so the very first
# check would report "OUTSIDE TOUCHED" for a change the harness itself made. That
# bug was observed on the first run of this file (117 false escapes).
OUTSIDE_FP_BEFORE="$(fp "$OUTSIDE")"
PASSWD_BEFORE="$(cksum /etc/passwd 2>/dev/null)"

FP_INITIAL="$(fp "$ROOT")"
echo "=== fixtures built ==="
echo "  files:   $(tar --sort=name -tf <(tar -cf - -C "$ROOT" . 2>/dev/null) 2>/dev/null | grep -v '/$' | wc -l) entries"
echo "  links:   link-docs (relative, inside)  link-escape (absolute, OUTSIDE)  link-broken (dangling)"
echo "  hard:    hard/one.txt and hard/two.txt share one inode"
echo

# ==============================================================================
# A. Capture — the whole root, and nothing else
# ==============================================================================
echo "=== A. capture — the whole root, and nothing else ==="

run_ok "A.1" "create a snapshot" "$E" $SNAP create --root "$ROOT" --store "$STORE" --label base

assert "A.2" "listed and visible" bash -c "cd '$STORE' && ls -A | grep -qx base && ls -A | grep -qx base.snapshot"

assert "A.3" "snapshot tree fingerprint == root tree fingerprint" \
  test "$(fp "$STORE/base")" = "$FP_INITIAL"

assert "A.4" "a deep file is present at the same depth" \
  test -f "$STORE/base/deep/a/b/c/d/e/leaf.txt"

assert "A.5" "empty directories survive" \
  bash -c "[ -d '$STORE/base/home/work' ] && [ -d '$STORE/base/home/documents/sub' ]"

assert "A.6" "mode 755 preserved in the snapshot" \
  bash -c "[ \"\$(stat -c %a '$STORE/base/weird/mode-755.sh')\" = 755 ]"

assert "A.7" "mode 444 preserved in the snapshot" \
  bash -c "[ \"\$(stat -c %a '$STORE/base/weird/mode-444.txt')\" = 444 ]"

assert "A.8" "zero-byte file present and empty" \
  bash -c "[ -f '$STORE/base/weird/empty-file.txt' ] && [ \$(stat -c %s '$STORE/base/weird/empty-file.txt') -eq 0 ]"

assert "A.9" "unicode name survives with the same bytes" \
  test -f "$STORE/base/weird/café.txt"

assert "A.10" "space in a name survives" \
  test -f "$STORE/base/weird/with space.txt"

assert "A.11" "dangling link survives as a dangling link" \
  bash -c "[ -L '$STORE/base/link-broken' ] && [ \"\$(readlink '$STORE/base/link-broken')\" = nothing-here.txt ]"

assert "A.12" "taking a snapshot changed nothing in the root" \
  test "$(fp "$ROOT")" = "$FP_INITIAL"
echo

# ==============================================================================
# B. Refusals — nothing is written at all
# ==============================================================================
echo "=== B. refusals — nothing written anywhere ==="

run_refuse "B.1" "root does not exist" 1 "$E" $SNAP create --root /tmp/atrium-2c-no-such-root --store "$STORE" --label x
run_refuse "B.2" "root is a plain file" 1 "$E" $SNAP create --root "$ROOT/home/documents/notes.txt" --store "$STORE" --label x
run_refuse "B.3" "root is /" 1 "$E" $SNAP create --root / --store "$STORE" --label x
ln -sfn "$ROOT" /tmp/atrium-2c-root-link
run_refuse "B.4" "root is a symlink" 1 "$E" $SNAP create --root /tmp/atrium-2c-root-link --store "$STORE" --label x
run_refuse "B.5" "store INSIDE the root" 1 "$E" $SNAP create --root "$ROOT" --store "$ROOT/snapshots" --label x
run_refuse "B.6" "store == root" 1 "$E" $SNAP create --root "$ROOT" --store "$ROOT" --label x
run_refuse "B.7" "store does not exist" 1 "$E" $SNAP create --root "$ROOT" --store /tmp/atrium-2c-no-such-store --label x
printf 'x' > /tmp/atrium-2c-store-as-file
run_refuse "B.8" "store is a plain file" 1 "$E" $SNAP create --root "$ROOT" --store /tmp/atrium-2c-store-as-file --label x
run_refuse "B.9" "duplicate label" 1 "$E" $SNAP create --root "$ROOT" --store "$STORE" --label base
run_refuse "B.10" "missing --root" 2 "$E" $SNAP create --store "$STORE" --label x
run_refuse "B.11" "missing --store" 2 "$E" $SNAP create --root "$ROOT" --label x
run_refuse "B.12" "missing --label" 2 "$E" $SNAP create --root "$ROOT" --store "$STORE"
run_refuse "B.13" "unknown flag" 2 "$E" $SNAP create --root "$ROOT" --store "$STORE" --label x --wat
echo

# ==============================================================================
# C. Restore — exactly back
# ==============================================================================
echo "=== C. restore — exactly back ==="

printf 'MODIFIED' > "$ROOT/home/documents/notes.txt"
run_ok "C.1" "restore after a file was modified" "$E" $SNAP restore --root "$ROOT" --store "$STORE" --id base
assert "C.2" "modified file came back with the original content" \
  bash -c "[ \"\$(cat '$ROOT/home/documents/notes.txt')\" = hi ]"

rm -f "$ROOT/weird/empty-file.txt"
run_ok "C.3" "restore after a file was deleted" "$E" $SNAP restore --root "$ROOT" --store "$STORE" --id base
assert "C.4" "deleted file is back" test -f "$ROOT/weird/empty-file.txt"

printf 'stranger' > "$ROOT/not-in-snapshot.txt"
run_ok "C.5" "restore with an extra file present" "$E" $SNAP restore --root "$ROOT" --store "$STORE" --id base
assert "C.6" "the extra file is GONE (a restore only adds nothing back if it removed it)" \
  test ! -e "$ROOT/not-in-snapshot.txt"

rm -rf "$ROOT/deep"
run_ok "C.7" "restore after a directory was deleted" "$E" $SNAP restore --root "$ROOT" --store "$STORE" --id base
assert "C.8" "the deleted directory came back with its contents" \
  test -f "$ROOT/deep/a/b/c/d/e/leaf.txt"

chmod 666 "$ROOT/weird/mode-444.txt"
run_ok "C.9" "restore after a mode was changed" "$E" $SNAP restore --root "$ROOT" --store "$STORE" --id base
assert "C.10" "the mode is 444 again" \
  bash -c "[ \"\$(stat -c %a '$ROOT/weird/mode-444.txt')\" = 444 ]"

rm -f "$ROOT/link-docs"; mkdir "$ROOT/link-docs"; printf 'x' > "$ROOT/link-docs/real.txt"
run_ok "C.11" "restore after a symlink was replaced by a directory" "$E" $SNAP restore --root "$ROOT" --store "$STORE" --id base
assert "C.12" "it is a symlink again, pointing at home/documents" \
  bash -c "[ -L '$ROOT/link-docs' ] && [ \"\$(readlink '$ROOT/link-docs')\" = home/documents ]"
assert "C.13" "and reading through it still works" \
  bash -c "[ \"\$(cat '$ROOT/link-docs/notes.txt')\" = hi ]"

INODE_BEFORE="$(stat -c %i "$ROOT")"
run_ok "C.14" "restore is idempotent (a second restore)" "$E" $SNAP restore --root "$ROOT" --store "$STORE" --id base
assert "C.15" "the root directory itself survived (same inode)" \
  bash -c "[ \"\$(stat -c %i '$ROOT')\" = '$INODE_BEFORE' ]"

run_refuse "C.16" "restore an unknown id" 1 "$E" $SNAP restore --root "$ROOT" --store "$STORE" --id no-such-snapshot
assert "C.17" "the root is untouched by that refusal" \
  test "$(fp "$ROOT")" = "$FP_INITIAL"

# A different root entirely.
printf 'important' > "$OTHER_ROOT/important.txt"
run_refuse "C.18" "restore into a DIFFERENT root" 1 "$E" $SNAP restore --root "$OTHER_ROOT" --store "$STORE" --id base
assert "C.19" "the other root's own data is untouched" \
  test -f "$OTHER_ROOT/important.txt"
echo

# ==============================================================================
# D. The plan's own verification step, verbatim
# ==============================================================================
echo "=== D. the plan's own step: snapshot, delete EVERYTHING, restore ==="

FP_BEFORE_D="$(fp "$ROOT")"
run_ok "D.1" "take a snapshot" "$E" $SNAP create --root "$ROOT" --store "$STORE" --label plan-step

# Delete everything in the environment, hidden files included.
shopt -s dotglob
rm -rf "$ROOT"/*
shopt -u dotglob
run_ok "D.2" "delete everything in the environment" true
assert "D.3" "the environment is now empty" bash -c "[ -z \"\$(ls -A '$ROOT')\" ]"
assert "D.4" "the snapshot still exists (it lives outside, so deleting the environment did not touch it)" \
  bash -c "[ -d '$STORE/plan-step' ] && [ -f '$STORE/plan-step.snapshot' ]"

run_ok "D.5" "restore" "$E" $SNAP restore --root "$ROOT" --store "$STORE" --id plan-step
assert "D.6" "the environment matches what it was before the deletion, exactly" \
  test "$(fp "$ROOT")" = "$FP_BEFORE_D"
assert "D.7" "the snapshot is still usable a second time" \
  bash -c "'$E' $SNAP restore --root '$ROOT' --store '$STORE' --id plan-step >/dev/null 2>&1"
echo

# ==============================================================================
# E. Symlinks are copied, never followed
# ==============================================================================
echo "=== E. symlinks copied, never followed ==="

assert "E.1" "the escape link's target string is preserved verbatim" \
  bash -c "[ \"\$(readlink '$STORE/base/link-escape')\" = '$OUTSIDE' ]"

assert "E.2" "the outside directory's contents are NOT in the snapshot" \
  bash -c "! test -f '$STORE/base/sentinel.txt' && ! test -e '$STORE/base/keep.txt'"

assert "E.3" "nor are they anywhere else under the store" \
  bash -c "! find '$STORE' -name 'sentinel.txt' -print -quit 2>/dev/null | grep -q ."

assert "E.4" "nothing is recorded under the link's own name" \
  bash -c "! tar -tf <(tar -cf - -C '$STORE/base' . 2>/dev/null) 2>/dev/null | grep -q 'link-escape/'"

assert "E.5" "a link to a directory stays a link in the store" \
  test -L "$STORE/base/link-docs"

assert "E.6" "and a restore keeps it a link" \
  test -L "$ROOT/link-docs"

assert "E.7" "the dangling link is still dangling after both directions" \
  bash -c "[ -L '$ROOT/link-broken' ] && [ ! -e '$ROOT/link-broken' ] && [ \"\$(readlink '$ROOT/link-broken')\" = nothing-here.txt ]"
echo

# ==============================================================================
# F. A snapshot that is incomplete or altered is never restorable
# ==============================================================================
echo "=== F. incomplete or altered snapshots are never restorable ==="

# F.1 an orphan directory with no completed record.
mkdir -p "$STORE/orphan/home"
printf 'half a snapshot' > "$STORE/orphan/home/x.txt"
assert "F.1" "a directory with no record is not LISTED as a snapshot" \
  bash -c "! '$E' $SNAP list --store '$STORE' 2>/dev/null | grep -q orphan"
run_refuse "F.2" "and cannot be restored" 1 "$E" $SNAP restore --root "$ROOT" --store "$STORE" --id orphan
assert "F.3" "the orphan directory is left on disk, not tidied away" \
  test -d "$STORE/orphan"

# F.4 an altered snapshot: change a file inside the store.
cp "$STORE/base.snapshot" /tmp/atrium-2c-record-backup
printf 'TAMPERED-IN-THE-STORE' > "$STORE/base/home/documents/notes.txt"
FP_WORKING="$(fp "$ROOT")"
run_refuse "F.4" "an ALTERED snapshot is refused on restore" 1 "$E" $SNAP restore --root "$ROOT" --store "$STORE" --id base
assert "F.5" "the environment was NOT touched by that refusal" \
  test "$(fp "$ROOT")" = "$FP_WORKING"
assert "F.6" "the refusal names the file that changed" \
  bash -c "'$E' $SNAP restore --root '$ROOT' --store '$STORE' --id base 2>&1 | grep -q 'notes.txt'"
assert "F.7" "the damaged snapshot is left visible for investigation" \
  test -d "$STORE/base"

# F.8 a snapshot with a file deleted from it.
rm -f "$STORE/base/deep/a/b/c/d/e/leaf.txt"
run_refuse "F.8" "a snapshot missing a recorded file is refused" 1 "$E" $SNAP restore --root "$ROOT" --store "$STORE" --id base
assert "F.9" "the environment is still untouched" \
  test "$(fp "$ROOT")" = "$FP_WORKING"

# F.10 a truncated record.
cp /tmp/atrium-2c-record-backup "$STORE/base.snapshot"
printf '' > "$STORE/base/deep/a/b/c/d/e/leaf.txt"   # restore the file the walk needs
"$E" $SNAP create --root "$ROOT" --store "$STORE" --label second >/dev/null 2>&1
python3 - "$STORE/second.snapshot" <<'PY'
import sys
p = sys.argv[1]
text = open(p).read()
cut = text.rfind("END ")
open(p, "w").write(text[:cut])
PY
assert "F.10" "a truncated record is not LISTED as a snapshot" \
  bash -c "! '$E' $SNAP list --store '$STORE' 2>/dev/null | grep -qw second"
run_refuse "F.11" "and cannot be restored" 1 "$E" $SNAP restore --root "$ROOT" --store "$STORE" --id second
echo

# ==============================================================================
# G. The store, the IDs and the labels — no path escapes
# ==============================================================================
echo "=== G. names used as paths — nothing escapes the store ==="

STORE_LISTING="$(cd "$STORE" && ls -A | sort | tr '\n' ' ')"
run_refuse "G.1" "label ../../outside-evil" 1 "$E" $SNAP create --root "$ROOT" --store "$STORE" --label ../../outside-evil
run_refuse "G.2" "label /etc/evil" 1 "$E" $SNAP create --root "$ROOT" --store "$STORE" --label /etc/evil
run_refuse "G.3" "label ." 1 "$E" $SNAP create --root "$ROOT" --store "$STORE" --label .
run_refuse "G.4" "label .." 1 "$E" $SNAP create --root "$ROOT" --store "$STORE" --label ..
run_refuse "G.5" "label with a slash" 1 "$E" $SNAP create --root "$ROOT" --store "$STORE" --label a/b
run_refuse "G.6" "id ../../outside-evil on restore" 1 "$E" $SNAP restore --root "$ROOT" --store "$STORE" --id ../../outside-evil
run_refuse "G.7" "id with a leading slash on restore" 1 "$E" $SNAP restore --root "$ROOT" --store "$STORE" --id /etc/evil
run_refuse "G.8" "a 300-byte label" 1 "$E" $SNAP create --root "$ROOT" --store "$STORE" --label "$(printf 'a%.0s' $(seq 300))"

assert "G.9" "nothing was created at the outside path the label aimed at" \
  bash -c "! test -e /tmp/outside-evil && ! test -e /etc/evil"
assert "G.10" "the store is unchanged by every refusal above" \
  bash -c "[ \"\$(cd '$STORE' && ls -A | sort | tr '\n' ' ')\" = '$STORE_LISTING' ]"
assert "G.11" "each refusal names the specific problem" \
  bash -c "'$E' $SNAP create --root '$ROOT' --store '$STORE' --label ../x 2>&1 | grep -qi 'path separator'"
echo

# ==============================================================================
# H. What this tool is, and what it is not — printed, not gate lines
# ==============================================================================
echo "=== H. stated plainly (not built — another phase's) ==="
echo "  H.1 this tool is NOT reachable by the agent. It is the app's own undo"
echo "      machinery, driven by the app and by the user. That is why it takes real"
echo "      host paths and is not routed through the path resolver: the resolver"
echo "      turns VIRTUAL paths into real ones, and there is no virtual path here."
echo "  H.2 a snapshot does NOT protect the store. A shell command under 2b can"
echo "      reach anything the user can reach, the store included. That is 2e."
echo "  H.3 snapshots are not automatic yet. This build is the mechanism;"
echo "      'before every task' is the caller's job, and the caller is Phase 5."
echo "  H.4 this is not an integrity guarantee against a hostile writer. The record"
echo "      detects accidental alteration; it is not cryptographic."
echo

# ==============================================================================
# I. What is deliberately not preserved — printed, not gate lines
# ==============================================================================
echo "=== I. deliberately not preserved (these are decisions, not oversights) ==="
echo "  I.1 modification times are not restored (std cannot set a timestamp)."
echo "      Contents, names, modes and links are; the clock is not."
echo "  I.2 hard links come back as separate files with identical contents."
echo "  I.3 extended attributes, ACLs and sparse structure are not preserved."
echo "  I.4 ownership is not restored."
echo "  I.5 directory modification times are not preserved."
echo
echo "  --- and here is I.2 happening, so it is visible rather than merely claimed:"
echo "      before the snapshot:  $(ls -i "$STORE/base/hard/one.txt" 2>/dev/null | awk '{print $1}') $(ls -i "$STORE/base/hard/two.txt" 2>/dev/null | awk '{print $1}')"
echo "      after the restore:   $(ls -i "$ROOT/hard/one.txt" 2>/dev/null | awk '{print $1}') $(ls -i "$ROOT/hard/two.txt" 2>/dev/null | awk '{print $1}')"
echo "      (the numbers were shared before and are not now)"
echo

# ==============================================================================
# J. The boring half
# ==============================================================================
echo "=== J. the boring half — ordinary use ==="

rm -rf "$ROOT" "$STORE"; mkdir -p "$STORE" "$ROOT"
printf 'one file' > "$ROOT/only.txt"
run_ok "J.1" "snapshot a root with a single file" "$E" $SNAP create --root "$ROOT" --store "$STORE" --label simple
rm "$ROOT/only.txt"
run_ok "J.2" "restore it" "$E" $SNAP restore --root "$ROOT" --store "$STORE" --id simple
assert "J.3" "the file is back" test -f "$ROOT/only.txt"

run_ok "J.4" "list shows snapshots" "$E" $SNAP list --store "$STORE"
assert "J.5" "list shows the snapshot by name" \
  bash -c "'$E' $SNAP list --store '$STORE' 2>/dev/null | grep -q simple"

# Two labels, independent.
printf 'second version' > "$ROOT/only.txt"
run_ok "J.6" "a second snapshot with a different label" "$E" $SNAP create --root "$ROOT" --store "$STORE" --label other
run_ok "J.7" "restore the FIRST one" "$E" $SNAP restore --root "$ROOT" --store "$STORE" --id simple
assert "J.8" "the older snapshot restored its own state" \
  bash -c "[ \"\$(cat '$ROOT/only.txt')\" = 'one file' ]"
assert "J.9" "both snapshots still listed" \
  bash -c "[ \$('$E' $SNAP list --store '$STORE' 2>/dev/null | tail -n +2 | wc -l) -eq 2 ]"

# An empty root.
rm -rf "$ROOT"; mkdir -p "$ROOT"
run_ok "J.10" "snapshot an EMPTY root" "$E" $SNAP create --root "$ROOT" --store "$STORE" --label empty
run_ok "J.11" "restore an empty root" "$E" $SNAP restore --root "$ROOT" --store "$STORE" --id empty
assert "J.12" "still empty, no error" bash -c "[ -z \"\$(ls -A '$ROOT')\" ]"

# Empty store.
rm -rf "$STORE"; mkdir -p "$STORE"
assert "J.13" "listing an EMPTY store is not an error" \
  bash -c "'$E' $SNAP list --store '$STORE' >/dev/null 2>&1"

# A newline in a filename.
mkdir -p "$ROOT/d"
printf 'newline name' > "$ROOT/d/line
break.txt"
run_ok "J.14" "snapshot a root with a newline in a filename" "$E" $SNAP create --root "$ROOT" --store "$STORE" --label newline
rm -rf "$ROOT/d"
run_ok "J.15" "restore it" "$E" $SNAP restore --root "$ROOT" --store "$STORE" --id newline
assert "J.16" "the newline name came back" \
  bash -c "[ -f '$ROOT/d/line
break.txt' ]"

assert "J.17" "a refusal that is understood returns 2 for usage, 1 for refusal" \
  bash -c "'$E' $SNAP create --root '$ROOT' --store '$STORE' >/dev/null 2>&1; [ \$? -eq 2 ]"
echo

# ==============================================================================
# L. Lines the blind list added
# ==============================================================================
echo "=== L. lines the blind list added ==="

# --- F.12: a symlink planted at the snapshot's own name in the store
rm -rf "$STORE"; mkdir -p "$STORE"
rm -rf "$ROOT"; mkdir -p "$ROOT"
printf 'legit\n' > "$ROOT/legit.txt"
ln -s "$OUTSIDE" "$STORE/planted"
run_refuse "F.12" "a symlink already at the snapshot's name is refused" 1 \
  "$E" $SNAP create --root "$ROOT" --store "$STORE" --label planted
assert "F.12b" "and nothing was written through it into the outside directory" \
  bash -c "! test -e '$OUTSIDE/planted' && ! test -e '$OUTSIDE/legit.txt'"

# --- F.13: the snapshot tree replaced by a symlink to a DECOY directory
rm -rf "$STORE" "$ROOT" "$DECOY"; mkdir -p "$STORE" "$ROOT" "$DECOY"
printf 'DECOY SECRET\n' > "$DECOY/secret.txt"
printf 'legit\n' > "$ROOT/legit.txt"
"$E" $SNAP create --root "$ROOT" --store "$STORE" --label decoy >/dev/null 2>&1
rm -rf "$STORE/decoy"; ln -s "$DECOY" "$STORE/decoy"     # genuine record, swapped tree
printf 'MY WORK\n' > "$ROOT/legit.txt"
run_refuse "F.13" "a snapshot tree swapped for a symlink to a decoy is refused" 1 \
  "$E" $SNAP restore --root "$ROOT" --store "$STORE" --id decoy
assert "F.13b" "the decoy's contents were NOT copied into the environment" \
  bash -c "! test -e '$ROOT/secret.txt'"
assert "F.13c" "the environment is untouched (the refusal ran before any deletion)" \
  bash -c "[ \"\$(cat '$ROOT/legit.txt')\" = 'MY WORK' ]"

# --- F.14: an EXTRA file inside the snapshot tree
rm -rf "$STORE" "$ROOT"; mkdir -p "$STORE" "$ROOT"
printf 'one\n' > "$ROOT/one.txt"
"$E" $SNAP create --root "$ROOT" --store "$STORE" --label extra >/dev/null 2>&1
printf 'smuggled\n' > "$STORE/extra/extra.txt"
run_refuse "F.14" "an extra file in the snapshot tree is refused" 1 \
  "$E" $SNAP restore --root "$ROOT" --store "$STORE" --id extra
assert "F.14b" "the extra file was not restored into the environment" \
  bash -c "! test -e '$ROOT/extra.txt'"

# --- L.1: a FIFO must refuse promptly, never hang
rm -rf "$STORE" "$ROOT"; mkdir -p "$STORE" "$ROOT"
printf 'normal\n' > "$ROOT/normal.txt"
mkfifo "$ROOT/a-fifo"
echo "  (running create under a 10s timeout: a blocking open on the fifo would hang)"
timeout 10 "$E" $SNAP create --root "$ROOT" --store "$STORE" --label fifo > /tmp/l1.log 2>&1
L1RC=$?
OKS=$((OKS + 1))
if [ "$L1RC" -eq 124 ]; then
  bad "L.1 create HUNG on a FIFO (timed out after 10s)"
elif [ "$L1RC" -eq 1 ]; then
  printf '[ok  ] %-5s %-62s rc=1  %s\n' "L.1" "a FIFO refuses promptly, no hang" "$(head -1 /tmp/l1.log | cut -c1-60)"
else
  bad "L.1 unexpected rc=$L1RC on a FIFO"
fi
assert "L.1b" "the refusal names the fifo and what it is" \
  bash -c "grep -q 'named pipe' /tmp/l1.log"

# --- L.2: snapshot -> restore -> snapshot: the two records must match
rm -rf "$STORE" "$ROOT" "$OUTSIDE"; mkdir -p "$STORE"
"$E" fixtures --root "$ROOT" --outside "$OUTSIDE" >/dev/null 2>&1
"$E" $SNAP create --root "$ROOT" --store "$STORE" --label pass1 >/dev/null 2>&1
"$E" $SNAP restore --root "$ROOT" --store "$STORE" --id pass1 >/dev/null 2>&1
"$E" $SNAP create --root "$ROOT" --store "$STORE" --label pass2 >/dev/null 2>&1
assert "L.2" "a snapshot of a restored environment is byte-identical to the first snapshot" \
  bash -c "diff <(grep -v '^ROOT' '$STORE/pass1.snapshot') <(grep -v '^ROOT' '$STORE/pass2.snapshot') >/dev/null"

# --- L.3: locale independence
rm -rf "$STORE" "$ROOT"; mkdir -p "$STORE" "$ROOT"
printf 'a\n' > "$ROOT/a.txt"
printf 'u\n' > "$ROOT/café.txt"
printf 'z\n' > "$ROOT/ÆØÅ.txt"
LC_ALL=C "$E" $SNAP create --root "$ROOT" --store "$STORE" --label loc >/dev/null 2>&1
rm -f "$ROOT/a.txt" "$ROOT/café.txt" "$ROOT/ÆØÅ.txt"
LC_ALL=en_US.UTF-8 "$E" $SNAP restore --root "$ROOT" --store "$STORE" --id loc >/dev/null 2>&1
assert "L.3" "created under LC_ALL=C, restored under UTF-8: all three names are back" \
  bash -c "[ -f '$ROOT/a.txt' ] && [ -f '$ROOT/café.txt' ] && [ -f '$ROOT/ÆØÅ.txt' ]"

# --- L.4: THE DEFECT THE BLIND LIST FOUND — a read-only directory
rm -rf "$STORE" "$ROOT"; mkdir -p "$STORE" "$ROOT/rodir"
printf 'x\n' > "$ROOT/rodir/f.txt"
printf 'top\n' > "$ROOT/top.txt"
"$E" $SNAP create --root "$ROOT" --store "$STORE" --label ro >/dev/null 2>&1
chmod 555 "$ROOT/rodir"
printf 'MODIFIED\n' > "$ROOT/top.txt"
"$E" $SNAP restore --root "$ROOT" --store "$STORE" --id ro > /tmp/l4.log 2>&1
L4RC=$?
OKS=$((OKS + 1))
if [ "$L4RC" -eq 0 ]; then
  printf '[ok  ] %-5s %-62s rc=0\n' "L.4" "restore through a read-only directory succeeds"
else
  bad "L.4 restore failed on a read-only directory (rc=$L4RC): $(head -1 /tmp/l4.log)"
fi
assert "L.4b" "the modified file was genuinely restored, not lost to a failed delete" \
  bash -c "[ \"\$(cat '$ROOT/top.txt')\" = 'top' ]"
assert "L.4c" "the file inside the read-only directory is back" \
  bash -c "[ \"\$(cat '$ROOT/rodir/f.txt')\" = 'x' ]"
chmod -R 755 "$ROOT" 2>/dev/null
echo

# ==============================================================================
# Tally
# ==============================================================================
check_outside "end of run"

echo "=== cleanup ==="
rm -rf "$ROOT" "$STORE" "$OUTSIDE" "$OTHER_ROOT" /tmp/atrium-2c-root-link \
       /tmp/atrium-2c-store-as-file /tmp/atrium-2c-record-backup
echo "  throwaway directories removed"
echo

echo "lines run: $OKS   failures: $FAILS   holes demonstrated (expected, 2e's): $HOLES"
if [ "$FAILS" -eq 0 ]; then
  echo "hand-test-2c: every line behaved as required"
  exit 0
else
  echo "hand-test-2c: $FAILS FAILED"
  exit 1
fi
