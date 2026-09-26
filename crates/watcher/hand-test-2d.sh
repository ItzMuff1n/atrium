#!/usr/bin/env bash
# Phase 2d hands-on harness — the gate for the filesystem watcher.
#
# This is the thing Muffin runs. Its value is NOT that it runs the checks; it is
# that three of its checks are INDEPENDENT of what the program says about itself:
#
#   !! OUTSIDE TOUCHED  the outside directory is listed and its sentinel checksummed
#                       before the run and after EVERY line. A watcher that reached
#                       outside the root would move this.
#   !! HOST TOUCHED     /etc/passwd's checksum and mtime, same discipline.
#   !! ROOT GONE        the environment root still exists.
#   !! REAL PATH LEAKED no default-output line may contain the root's real path.
#                       Checked as bytes, against every line, unconditionally.
#
# A green run therefore means: nothing outside moved, nothing in the environment was
# destroyed, and the sandbox's real location was never printed. The program's own
# claims are checked against those, not taken on trust.
#
# Usage: bash hand-test-2d.sh [root]

set -u

ROOT="${1:-/tmp/atrium-2d-root}"
OUTSIDE="$(dirname "$ROOT")/atrium-2d-outside"
# A third location. The OUTSIDE directory is a sentinel and nothing else — a fixture
# built inside it makes the harness's own setup look like an escape. (Found by
# running it: the first version did exactly that.)
STAGE="$(dirname "$ROOT")/atrium-2d-stage"
HERE="$(cd "$(dirname "$0")" && pwd)"

# The binary lives in the WORKSPACE target dir, not in the crate's.
#
# `cargo build` in a cargo workspace writes to the workspace root's target/debug,
# whatever directory it is run from. The previous version used
# `./target/debug/atrium-watcher` after `cd "$HERE"`, so it resolved to
# `crates/watcher/target/debug/` — a path the build never writes. It only ever
# worked because a stale binary from an earlier session happened to sit there, and
# the `-newer` check then compared the wrong artefact against the sources.
# Observed 24 Sep 2026: crates/watcher/target/debug/atrium-watcher was 19:12:12,
# while crates/watcher/src/lib.rs was 19:34:51 — the script was testing a binary
# 22 minutes older than the code it claimed to test.
#
# Resolve to the workspace root explicitly and refuse to guess. (hand-test-1b.sh
# carries the same fix; it was the first script repaired this way.)
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="$REPO_ROOT/target/debug/atrium-watcher"

cd "$HERE" || { echo "cannot cd to $HERE"; exit 2; }

FAILS=0
OKS=0
LEAKS=0
GONE=0
TOUCHED=0

BEFORE_OUTSIDE=""
BEFORE_SENTINEL=""
BEFORE_PASSWD=""

ok()   { OKS=$((OKS+1));   printf '  [ok]   %s\n' "$1"; }
fail() { FAILS=$((FAILS+1)); printf '  [FAIL] %s\n' "$1"; }

# ---------------------------------------------------------------- independent checks

check_outside() {
  local where="$1"
  local now_sent now_list
  now_sent="$(cksum < "$OUTSIDE/sentinel.txt" 2>/dev/null || echo MISSING)"
  now_list="$(ls -A "$OUTSIDE" 2>/dev/null | sort | tr '\n' ' ')"
  if [ "$now_sent" != "$BEFORE_SENTINEL" ] || [ "$now_list" != "$BEFORE_OUTSIDE" ]; then
    printf '  !! OUTSIDE TOUCHED (%s)\n' "$where"
    printf '     before: %s | %s\n' "$BEFORE_OUTSIDE" "$BEFORE_SENTINEL"
    printf '     after : %s | %s\n' "$now_list" "$now_sent"
    TOUCHED=$((TOUCHED+1)); FAILS=$((FAILS+1))
  fi
  local now_passwd
  now_passwd="$(cksum < /etc/passwd 2>/dev/null || echo MISSING)"
  if [ "$now_passwd" != "$BEFORE_PASSWD" ]; then
    printf '  !! HOST TOUCHED (%s): /etc/passwd changed\n' "$where"
    TOUCHED=$((TOUCHED+1)); FAILS=$((FAILS+1))
  fi
  if [ ! -d "$ROOT" ]; then
    printf '  !! ROOT GONE (%s)\n' "$where"
    GONE=$((GONE+1)); FAILS=$((FAILS+1))
  fi
}

# Run the watcher over a window, capture output, then run the independent checks.
# $1 = label, $2 = milliseconds to watch for. Sets RUN_OUT and RUN_RC.
run_watch() {
  local label="$1" ms="$2"
  RUN_OUT="$("$BIN" watch --root "$ROOT" --duration-ms "$ms" --settle-ms 60 2>&1)"
  RUN_RC=$?
  check_outside "$label"
  # The disclosure check: no default-output line may contain the real root.
  if printf '%s\n' "$RUN_OUT" | grep -qF "$ROOT"; then
    printf '  !! REAL PATH LEAKED (%s)\n' "$label"
    printf '%s\n' "$RUN_OUT" | grep -F "$ROOT" | head -3 | sed 's/^/     /'
    LEAKS=$((LEAKS+1)); FAILS=$((FAILS+1))
  fi
}

# Does the output contain a line whose tag is $1 and which mentions $2?
has() { printf '%s\n' "$RUN_OUT" | grep -E "^$1" | grep -qF -- "$2"; }
count_tag() { printf '%s\n' "$RUN_OUT" | grep -cE "^$1" || true; }

# ---------------------------------------------------------------- setup

echo "=== atrium-watcher, Phase 2d hands-on harness ==="
echo "root    : $ROOT"
echo "outside : $OUTSIDE"
echo

# Rebuild if any source file is newer than the binary, and say so when it happens.
# The `-newer` test compares the WORKSPACE binary against the workspace sources;
# testing the crate-relative binary against crate sources was the defect above.
NEED_BUILD=0
if [ ! -x "$BIN" ]; then NEED_BUILD=1; fi
if [ -x "$BIN" ] && [ -n "$(find "$REPO_ROOT"/crates/watcher/src "$REPO_ROOT"/crates/watcher/tests "$REPO_ROOT"/crates/watcher/Cargo.toml -newer "$BIN" -print -quit 2>/dev/null)" ]; then
  NEED_BUILD=1
fi
if [ "$NEED_BUILD" = 1 ]; then
  echo "  building (a source file is newer than the binary, or the binary is absent)"
  # Capture the output so the EXIT STATUS IS CARGO'S, not tail's.
  #
  # This was `if ! ( cd … && cargo build 2>&1 | tail -3 )`. In a pipeline the status
  # is the LAST command's, so that tested `tail` (which succeeds even when cargo
  # fails) and the branch below was unreachable. A failed build therefore fell
  # through to a stale binary — the exact failure this script exists to prevent.
  # Found by review round 1 on this PR and confirmed by a failing test before the fix.
  BUILD_OUT="$( cd "$REPO_ROOT" && cargo build -p atrium-watcher 2>&1 )"
  BUILD_RC=$?
  if [ "$BUILD_RC" -ne 0 ]; then
    printf '%s\n' "$BUILD_OUT" | tail -3
    echo "  BUILD FAILED (cargo exit $BUILD_RC) — nothing below can be trusted"; exit 1
  fi
fi
[ -x "$BIN" ] || {
  echo "no binary at $BIN" >&2
  echo "  The workspace build writes there; if it is missing, the build failed." >&2
  exit 1
}
# The check the topic requires: FAIL if the binary is older than any file in the
# crate's `src/`.
#
# Rebuilding above is the normal path. This is the assertion for the case where the
# build reports success and the artefact still did not refresh: without it the pass
# would run a stale binary and report a verdict about superseded code. It refuses
# rather than warns, because "a stale binary was visible in the output" is not the
# same as "a stale binary was not used".
STALE_AFTER="$(find "$REPO_ROOT/crates/watcher/src" -newer "$BIN" -print -quit 2>/dev/null)"
if [ -n "$STALE_AFTER" ]; then
  echo "REFUSING: $BIN is older than ${STALE_AFTER#"$REPO_ROOT"/} after a successful build." >&2
  echo "  Testing it would report a verdict about code that is not on disk." >&2
  exit 2
fi
# Name the artefact under test. A hands-on pass is worth only what it ran.
echo "binary under test: $BIN"
echo "                   ($(stat -c '%y' "$BIN" | cut -c1-19), sha256 $(sha256sum "$BIN" | cut -c1-16))"

rm -rf "$ROOT" "$OUTSIDE" "$STAGE"
mkdir -p "$ROOT" "$OUTSIDE" "$STAGE"
printf 'sentinel\n' > "$OUTSIDE/sentinel.txt"
printf 'keep\n' > "$OUTSIDE/keep.txt"

# Fixtures, built by the tool itself so the shape is in one place.
"$BIN" fixtures --root "$ROOT" --outside "$OUTSIDE" > /dev/null || {
  echo "fixtures failed"; exit 1
}

BEFORE_OUTSIDE="$(ls -A "$OUTSIDE" | sort | tr '\n' ' ')"
BEFORE_SENTINEL="$(cksum < "$OUTSIDE/sentinel.txt")"
BEFORE_PASSWD="$(cksum < /etc/passwd)"
echo "before  : outside=[$BEFORE_OUTSIDE] sentinel=[$BEFORE_SENTINEL]"
echo "          /etc/passwd=[$BEFORE_PASSWD]"
echo

# ---------------------------------------------------------------- §A the plain case

echo "=== A. the plain case — the plan's own verification step ==="
# A background writer, so the watcher is genuinely watching while a shell command runs.
( sleep 0.5; echo hi > "$ROOT/home/documents/by-command.txt" ) &
WRITER=$!
run_watch "A" 1600
wait $WRITER 2>/dev/null
check_outside "A end"

if has CREATE "/home/documents/by-command.txt"; then
  ok "A.1 a shell command's new file is reported: CREATE /home/documents/by-command.txt"
else
  fail "A.1 the command's file was not reported as CREATE"
fi
if has MODIF "/home/documents/by-command.txt"; then
  ok "A.2 the command's write is reported at close: MODIF"
else
  fail "A.2 no MODIF for the command's write"
fi
if printf '%s\n' "$RUN_OUT" | grep -q "^# "; then
  ok "A.3 the run states what it cannot see"
else
  fail "A.3 no 'cannot see' statements in the output"
fi
echo

# ---------------------------------------------------------------- §B the reduction

echo "=== B. once per edit, not once per write ==="
rm -f "$ROOT/home/documents/notes.txt"
printf 'seed\n' > "$ROOT/home/documents/notes.txt"
( sleep 0.4; for i in 1 2 3; do printf 'edit %s\n' "$i" > "$ROOT/home/documents/notes.txt"; sleep 0.15; done ) &
WRITER=$!
run_watch "B" 1500
wait $WRITER 2>/dev/null
check_outside "B end"

N_MODIF_NOTES="$(printf '%s\n' "$RUN_OUT" | grep -E "^MODIF" | grep -cF "notes.txt" || true)"
if [ "$N_MODIF_NOTES" -ge 1 ]; then
  ok "B.1 three separate edits produced $N_MODIF_NOTES MODIF records (one per completed edit)"
else
  fail "B.1 the edits produced no MODIF at all"
fi
echo "        (note: three open/write/close cycles ARE three changes — attack-list §B.2)"
echo

# ---------------------------------------------------------------- §C names as bytes

echo "=== C. the name is right — including names that are not text ==="
( sleep 0.4; printf 'x' > "$ROOT/home/documents/with space.txt" ) &
W1=$!
run_watch "C1" 1400
wait $W1 2>/dev/null
check_outside "C1 end"
if has CREATE "/home/documents/with space.txt"; then
  ok "C.1 a name containing a space is reported exactly"
else
  fail "C.1 a name containing a space was not reported exactly"
fi

( sleep 0.4; printf 'x' > "$ROOT/home/documents/"$'bad-\xff\xfe-name.txt' ) &
W2=$!
run_watch "C2" 1400
wait $W2 2>/dev/null
check_outside "C2 end"
if printf '%s\n' "$RUN_OUT" | grep -q 'bad-\\xFF\\xFE-name.txt'; then
  ok "C.2 a non-UTF-8 name is reported as escaped bytes, not replacement characters"
else
  fail "C.2 a non-UTF-8 name was not reported as escaped bytes"
  printf '%s\n' "$RUN_OUT" | grep -i "bad" | head -3 | sed 's/^/        /'
fi
echo

# ---------------------------------------------------------------- §D the tree

echo "=== D. the tree — what is watched, and what is not ==="
( sleep 0.4; printf 'x' > "$ROOT/home/documents/sub/deep/deep-change.txt" ) &
W3=$!
run_watch "D1" 1400
wait $W3 2>/dev/null
check_outside "D1 end"
if has CREATE "sub/deep/deep-change.txt"; then
  ok "D.1 a file three levels down is reported"
else
  fail "D.1 a file three levels down was not reported"
fi

# A symlink out of the root must not be followed: writing OUTSIDE is invisible.
#
# The write happens INSIDE the watched window and is removed again before the
# outside listing is compared — otherwise the harness's own fixture shows up as
# "!! OUTSIDE TOUCHED" and the check reports a fault that is its own doing. (Found
# by running it: the first version did exactly that.)
( sleep 0.5; printf 'secret' > "$OUTSIDE/host-side.txt"; sleep 0.4; rm -f "$OUTSIDE/host-side.txt" ) &
W4=$!
run_watch "D2" 1800
wait $W4 2>/dev/null
rm -f "$OUTSIDE/host-side.txt"
check_outside "D2 end"
if printf '%s\n' "$RUN_OUT" | grep -qE "^(CREATE|MODIF|DELETE|MOVEFROM|MOVETO|ATTRIB)"; then
  fail "D.2 activity OUTSIDE the root was reported as a change (a symlink was followed)"
  printf '%s\n' "$RUN_OUT" | grep -E "^(CREATE|MODIF)" | head -3 | sed 's/^/        /'
else
  ok "D.2 a write outside the root produced no change record"
fi

# A POPULATED subtree moved in must be watched at every level — the blind list's
# finding. This is the line that would have caught it.
rm -rf "$STAGE/incoming"
mkdir -p "$STAGE/incoming/lvl2/lvl3"
printf 'x' > "$STAGE/incoming/top.txt"
printf 'x' > "$STAGE/incoming/lvl2/mid.txt"
printf 'x' > "$STAGE/incoming/lvl2/lvl3/deep.txt"
# Moved in WHILE the watcher is running — that is the case that was broken.
( sleep 0.4; mv "$STAGE/incoming" "$ROOT/home/incoming"; sleep 0.3; \
  printf 'x' > "$ROOT/home/incoming/top-new.txt"; \
  printf 'x' > "$ROOT/home/incoming/lvl2/mid-new.txt"; \
  printf 'x' > "$ROOT/home/incoming/lvl2/lvl3/deep-new.txt" ) &
W5=$!
run_watch "D3" 2200
wait $W5 2>/dev/null
check_outside "D3 end"
if has CREATE "incoming/top-new.txt" && has CREATE "incoming/lvl2/mid-new.txt" && has CREATE "incoming/lvl2/lvl3/deep-new.txt"; then
  ok "D.3 a populated subtree moved INTO the root is watched at every level"
else
  fail "D.3 a moved-in subtree is only watched at its top level (blind-list §1.3)"
  printf '%s\n' "$RUN_OUT" | grep "incoming" | head -5 | sed 's/^/        /'
fi
echo

# ---------------------------------------------------------------- §E moves

echo "=== E. moves — the cookie is the pairing ==="
( sleep 0.4; mv "$ROOT/home/documents/sub/deep/leaf.txt" "$ROOT/home/documents/leaf-moved.txt" ) &
W6=$!
run_watch "E" 1500
wait $W6 2>/dev/null
check_outside "E end"
FR="$(printf '%s\n' "$RUN_OUT" | grep -oE '^MOVEFROM.*cookie=[0-9]+' | grep -oE '[0-9]+$' | head -1)"
TO="$(printf '%s\n' "$RUN_OUT" | grep -oE '^MOVETO.*cookie=[0-9]+' | grep -oE '[0-9]+$' | head -1)"
if [ -n "$FR" ] && [ "$FR" = "$TO" ]; then
  ok "E.1 the two halves of a cross-directory move share cookie=$FR"
elif [ -n "$FR" ] || [ -n "$TO" ]; then
  fail "E.1 a move's halves did not pair (MOVEFROM cookie=$FR, MOVETO cookie=$TO)"
else
  fail "E.1 no move was reported at all"
fi
echo

# ---------------------------------------------------------------- §G refusals

echo "=== G. refusals — the watcher declines, with a reason ==="
OUT="$("$BIN" watch --root "/tmp/atrium-2d-not-here-$$" --duration-ms 50 2>&1)"; RC=$?
if [ "$RC" -eq 1 ] && printf '%s' "$OUT" | grep -qi "cannot watch"; then
  ok "G.1 a missing root is refused with a reason (exit 1)"
else
  fail "G.1 a missing root was not refused properly (exit $RC)"
fi
OUT="$("$BIN" watch --root "relative/path" --duration-ms 50 2>&1)"; RC=$?
if [ "$RC" -eq 2 ] && printf '%s' "$OUT" | grep -qi "absolute"; then
  ok "G.2 a relative root is a usage error (exit 2)"
else
  fail "G.2 a relative root was not a usage error (exit $RC)"
fi
check_outside "G end"
echo

# ---------------------------------------------------------------- §I the boundary

echo "=== I. what the watcher is, and is not ==="
if printf '%s\n' "$RUN_OUT" | grep -q "# cannot see"; then
  ok "I.1 every run states what it cannot see, where the result is read"
else
  fail "I.1 the run does not state its limits"
fi
if [ "$LEAKS" -eq 0 ]; then
  ok "I.2 no line of the default output contained the root's real path (checked every line)"
else
  fail "I.2 the real root path appeared in the change stream ($LEAKS time(s))"
fi
echo

# ---------------------------------------------------------------- §K the boring half

echo "=== K. the boring half ==="
OUT="$("$BIN" demo 2>&1)"; RC=$?
check_outside "K demo"
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -q "^CREATE"; then
  ok "K.1 demo runs its own scenario and reports what it saw (exit 0)"
else
  fail "K.1 demo did not run cleanly (exit $RC)"
fi
if printf '%s' "$OUT" | grep -q "ONE MODIF"; then
  ok "K.2 demo exercises the reduction explicitly"
else
  fail "K.2 demo does not exercise the reduction"
fi
echo

# ---------------------------------------------------------------- cleanup + tally

# TEMPORARY (throwaway PR): a deliberately failing line. Deleted before this
# branch is finished with. It fails through the script's own fail() helper, so
# the script still cleans up and still exits 1 -- the only thing it proves is
# that GitHub refuses to merge a PR whose hand-tests job is red.
if [ "${PROBE_NEVER_SET:-0}" = 1 ]; then ok "probe"; else fail "DELIBERATE PROBE FAILURE (throwaway branch)"; fi

echo "=== cleanup ==="
rm -rf "$ROOT" "$OUTSIDE" "$STAGE" 2>/dev/null
echo "  throwaway directories removed"
echo
echo "lines run: $OKS   failures: $FAILS"
echo "outside touches: $TOUCHED   root disappearances: $GONE   real-path leaks: $LEAKS"

if [ "$FAILS" -eq 0 ] && [ "$LEAKS" -eq 0 ] && [ "$TOUCHED" -eq 0 ] && [ "$GONE" -eq 0 ]; then
  echo "hand-test-2d: every line behaved as required"
  exit 0
else
  echo "hand-test-2d: $FAILS line(s) FAILED"
  exit 1
fi
