#!/usr/bin/env bash
# Atrium Phase 2a — hands-on harness (delegation-briefs/phase-2a-file-operations.md §9.7).
# Runs every line of attack-list-2a.md against atrium-fileops in a throwaway
# root with a throwaway OUTSIDE directory as the escape detector.
#
# Usage:  bash fileops/hand-test-2a.sh [root]
#         (default root: /tmp/atrium-2a-root)
#
# The outside dir is <root's parent>/atrium-2a-outside. Both are deleted at
# the end. Nothing else is touched. The host /etc/passwd checksum and mtime
# are tracked throughout and compared at the end.

set -u

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="${1:-/tmp/atrium-2a-root}"
OUTSIDE="$(dirname "$ROOT")/atrium-2a-outside"

# The binary lives in the WORKSPACE target dir, not the crate's.
#
# `cargo build` in a cargo workspace writes to the workspace root's target/debug,
# whatever directory it is run from. The previous version used `$HERE/target/debug`
# — crates/fileops/target/debug — which the build never writes. It only "worked"
# because a stale binary from an earlier session sat there, so the `[ ! -x ]` guard
# skipped building entirely and the pass ran against old code.
# Observed 24 Sep 2026: the stale crates/watcher binary was 22 minutes older than
# its own lib.rs while the script reported it as current. The stale per-crate
# target/ dirs have since been deleted, which is what makes this a hard failure
# rather than a silent one.
# Resolve to the workspace root explicitly. (Same fix as hand-test-1b.sh, which was
# repaired first, and hand-test-2d.sh.)
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="$REPO_ROOT/target/debug/atrium-fileops"

# Rebuild when absent, or when any crate source is newer than the binary.
NEED_BUILD=0
[ -x "$BIN" ] || NEED_BUILD=1
if [ -x "$BIN" ] && [ -n "$(find "$REPO_ROOT"/crates/fileops/src "$REPO_ROOT"/crates/fileops/tests "$REPO_ROOT"/crates/fileops/Cargo.toml -newer "$BIN" -print -quit 2>/dev/null)" ]; then
    NEED_BUILD=1
fi
if [ "$NEED_BUILD" = 1 ]; then
    echo "building (binary absent, or a crate source is newer than it)..." >&2
    (cd "$REPO_ROOT" && cargo build -p atrium-fileops >/dev/null 2>&1) || { echo "cargo build failed" >&2; exit 2; }
fi
[ -x "$BIN" ] || { echo "no binary at $BIN after building" >&2; exit 2; }
# The check the topic requires: FAIL if the binary is older than any file in the
# crate's `src/`. Rebuilding above is the normal path; this asserts the case where
# the build reports success and the artefact still did not refresh, so the pass
# cannot run a stale binary and report a verdict about superseded code.
STALE_AFTER="$(find "$REPO_ROOT/crates/fileops/src" -newer "$BIN" -print -quit 2>/dev/null)"
if [ -n "$STALE_AFTER" ]; then
    echo "REFUSING: $BIN is older than ${STALE_AFTER#"$REPO_ROOT"/} after a successful build." >&2
    echo "  Testing it would report a verdict about code that is not on disk." >&2
    exit 2
fi
# Name the artefact under test, so a stale one is visible in the output.
echo "binary under test: $BIN"
echo "                   ($(stat -c '%y' "$BIN" | cut -c1-19), sha256 $(sha256sum "$BIN" | cut -c1-16))"

rm -rf "$ROOT" "$OUTSIDE"
mkdir -p "$ROOT" "$OUTSIDE"

export ATRIUM_FIXTURE_OUTSIDE="$OUTSIDE"
"$BIN" run --root "$ROOT" fixtures >/dev/null || { echo "fixtures failed" >&2; exit 2; }
mkdir -p "$OUTSIDE/sub"
printf 'sentinel\n' > "$OUTSIDE/sentinel.txt"
printf 'keep\n' > "$OUTSIDE/sub/keep.txt"

# --- Record before the run (J.1/J.2) ---------------------------------------
fingerprint_dir() {
    # The outside directory's OWN contents only: entry names at every level,
    # per-entry metadata, and content checksums. Deliberately NOT `ls -laR`:
    # that prints each directory's `..` entry, and $OUTSIDE sits directly under
    # /tmp -- so the fingerprint included /tmp's own size and mtime, and any
    # unrelated process creating or removing a file in /tmp changed it. The
    # detector then reported OUTSIDE TOUCHED with nothing outside the root
    # having been touched. Observed on a shared CI runner: two runs of the same
    # workflow at the same commit gave `failures: 0` and `failures: 51`, and
    # `hand-tests` is a required check. Reproduced directly -- 4 quiet runs at
    # 0 failures, 4 runs with unrelated /tmp churn at 56, 10, 59 and 11. This
    # is a narrowing to what the detector was always meant to measure, not a
    # weakening: a real touch anywhere under $OUTSIDE still changes this.
    # `hand-test-2b.sh` rejected `ls -laR` for the same reason and is the
    # reference implementation; this mirrors it. Issue #83.
    ( printf '%s\n' '--- entries'
      (cd "$1" && find . -mindepth 1 -printf '%y %P\n' | sort)
      printf '%s\n' '--- stat'
      # Mode is in here deliberately. The old `ls -laR` fingerprint carried it
      # in its permission column, and a chmod on a file outside the root is a
      # real touch that must still be caught. Dropping it would have been a
      # silent weakening while replacing the listing.
      find "$1" -mindepth 1 -printf '%y %m %P %s %T@\n' | sort
      printf '%s\n' '--- cksum'
      cksum "$1/sentinel.txt" "$1/sub/keep.txt" 2>/dev/null )
}
OUT_BEFORE="$(fingerprint_dir "$OUTSIDE")"
SENTINEL_BEFORE="$(cksum "$OUTSIDE/sentinel.txt")"
KEEP_BEFORE="$(cksum "$OUTSIDE/sub/keep.txt")"
PASSWD_BEFORE="$(cksum /etc/passwd) $(stat -c '%Y' /etc/passwd)"

echo "environment root: $ROOT"
echo "outside dir:      $OUTSIDE"
echo "out sentinel before:  $SENTINEL_BEFORE"
echo "/etc/passwd before:   $PASSWD_BEFORE"
echo

FAILS=0
OKS=0

# check_outside <line>: the independent detector. Looks at the disk, never
# at anything the program said about itself.
check_outside() {
    local line="$1"
    if [ "$(fingerprint_dir "$OUTSIDE")" != "$OUT_BEFORE" ]; then
        echo "  !! OUTSIDE TOUCHED at $line"
        FAILS=$((FAILS+1))
    fi
    if [ "$(cksum /etc/passwd) $(stat -c '%Y' /etc/passwd)" != "$PASSWD_BEFORE" ]; then
        echo "  !! HOST /etc/passwd CHANGED at $line"
        FAILS=$((FAILS+1))
    fi
    if [ ! -d "$ROOT" ]; then
        echo "  !! ROOT GONE at $line"
        FAILS=$((FAILS+1))
    fi
}

# run <section> <expect: REFUSE|OK> <op-args...>
# One line per attack-list check.
run() {
    local section="$1" want="$2"; shift 2
    local out rc mark verdict
    out="$("$BIN" run --root "$ROOT" "$@" 2>&1)"; rc=$?
    verdict="${out%% *}"
    mark="ok  "
    if [ "$want" = "REFUSE" ]; then
        if [ "$verdict" != "REFUSE" ] || [ "$rc" -eq 0 ]; then mark="FAIL"; FAILS=$((FAILS+1)); fi
    else
        if [ "$verdict" != "OK" ]; then mark="FAIL"; FAILS=$((FAILS+1)); fi
    fi
    # H.2/H.3/H.4: NO host path may appear in the program's output, on a
    # success OR a refusal. The resolver names real paths in two of its
    # rejection reasons; the operations re-word those from the resolver's
    # structured fields so the reason still names the failing step while the
    # real location is dropped (src/lib.rs, FileOpError::Rejected). This check
    # is therefore unconditional: there is no exempt line, and a refusal that
    # discloses the root or the outside directory fails like any other.
    case "$out" in
        *"$OUTSIDE"*|*"$ROOT"*)
            mark="FAIL"; FAILS=$((FAILS+1)); echo "  !! HOST PATH DISCLOSED: $out" ;;
    esac
    printf '[%s] [%-6s] %-58s %s\n' "$mark" "$section" "$*" "$out"
    OKS=$((OKS+1))
    check_outside "$section $*"
    [ "$mark" = "FAIL" ] && return 1 || return 0
}

# must_exist_inside <line> <real-path-under-root>  (J.5 mirror check)
# must_exist_inside <label> <real-path-under-root>  (J.5 mirror check)
must_exist_inside() {
    if [ ! -e "$2" ] && [ ! -L "$2" ]; then
        echo "  !! MUST EXIST MISSING at $1: $2"
        FAILS=$((FAILS+1))
    fi
}
must_not_exist_anywhere() {
    # $1 real path; checked by looking, not by trusting the message (J.4)
    if [ -e "$1" ] || [ -L "$1" ]; then
        echo "  !! MUST NOT EXIST PRESENT: $1"
        FAILS=$((FAILS+1))
    fi
}

echo "=== A. traversal in the path argument (MUST REFUSE) ==="
run A.1 REFUSE write /home/../../escaped.txt --content x || true
must_not_exist_anywhere "$OUTSIDE/escaped.txt"
run A.2 REFUSE create-dir /../escaped-dir || true
run A.3 REFUSE delete /home/documents/../../escaped.txt --recursive || true
run A.4 REFUSE read /../../etc/passwd || true
run A.5 REFUSE list /home/../.. || true
run A.6 REFUSE move /home/documents/notes.txt /home/../../moved.txt || true
must_exist_inside A.6 "$ROOT/home/documents/notes.txt"
run A.7 REFUSE delete /.. --recursive || true
run A.8 OK create-dir /..../deep || true
must_exist_inside A.8 "$ROOT/..../deep"

echo
echo "=== B. absolute paths that look like host paths (MUST DO IT INSIDE) ==="
run B.1 OK create-dir /etc/passwd || true
must_exist_inside B.1 "$ROOT/etc/passwd"
run B.2 OK write /etc/passwd/x --content hi || true
must_exist_inside B.2 "$ROOT/etc/passwd/x"
B3_OUT="$("$BIN" run --root "$ROOT" write /etc/shadow --content hi 2>&1)"; B3_RC=$?
case "$B3_OUT" in
    *"Permission denied"*|*"os error 13"*)
        echo "[FAIL] [B.3   ] write /etc/shadow — host permission error: aimed at the HOST"
        FAILS=$((FAILS+1)) ;;
    *) printf '[ok  ] [B.3   ] %-58s %s\n' "write /etc/shadow" "$B3_OUT" ;;
esac
check_outside "B.3"
run B.4 OK delete /etc/passwd --recursive || true
must_not_exist_anywhere "$ROOT/etc/passwd"
run B.5 OK create-dir /root/secret || true
must_exist_inside B.5 "$ROOT/root/secret"
B6_OUT="$("$BIN" run --root "$ROOT" write /usr/bin/evil --content x 2>&1)"
case "$B6_OUT" in
    *"Permission denied"*|*"os error 13"*)
        echo "[FAIL] [B.6   ] write /usr/bin/evil — host permission error: aimed at the HOST"
        FAILS=$((FAILS+1)) ;;
    *) printf '[ok  ] [B.6   ] %-58s %s\n' "write /usr/bin/evil" "$B6_OUT" ;;
esac

echo
echo "=== C. symlink routes out (MUST REFUSE, every operation) ==="
run C.1 REFUSE write /outside-link/created.txt --content x || true
must_not_exist_anywhere "$OUTSIDE/created.txt"
run C.2 REFUSE create-dir /outside-link/newdir || true
must_not_exist_anywhere "$OUTSIDE/newdir"
run C.3 REFUSE read /outside-link/sentinel.txt || true
run C.4 REFUSE list /outside-link || true
run C.5 REFUSE delete /outside-link/sentinel.txt || true
must_exist_inside C.5 "$OUTSIDE/sentinel.txt"
run C.6 REFUSE move /home/documents/notes.txt /outside-link/moved.txt || true
must_exist_inside C.6 "$ROOT/home/documents/notes.txt"
run C.7 REFUSE write /chain-a/created.txt --content x || true
must_not_exist_anywhere "$OUTSIDE/created.txt"
run C.8 REFUSE create-dir /chain-a/newdir || true
run C.9 REFUSE write /rel-outside-link/created.txt --content x || true
run C.10 REFUSE list /rel-outside-link || true
run C.11 REFUSE write /dangling-out/newfile --content x || true
# C.12: the absent target is the PARENT here, so the ordinary parent rule refuses
# (not a symlink rule). C.13: creating the absent target THROUGH the link must work.
# C.13 runs LAST in this section, and on a DIFFERENT dangling link, deliberately:
# creating it makes <root>/home/documents/absent.txt real, and E.8 later expects
# that exact path to be absent. Two dangling-ins, two purposes, no interference.
run C.12 REFUSE write /dangling-in/newfile --content x || true
run C.13 OK write /dangling-in2 --content x || true
must_exist_inside C.13 "$ROOT/home/documents/absent2.txt"

echo
echo "=== D. move — both ends attacked ==="
run D.1 REFUSE move /outside-link/sentinel.txt /home/documents/copied.txt || true
must_not_exist_anywhere "$ROOT/home/documents/copied.txt"
must_exist_inside D.1 "$OUTSIDE/sentinel.txt"
run D.2 REFUSE move /home/documents/notes.txt /home/../../out.txt || true
run D.3 REFUSE move /home/documents/notes.txt /outside-link/dest.txt || true
run D.4 REFUSE move /home/../../x /home/documents/y || true
run D.5 REFUSE move / /home/movedroot || true
run D.6 REFUSE move /home/documents /outside-link/sub || true

echo
echo "=== E. delete — the most dangerous operation in the phase ==="
run E.1 OK delete /trap --recursive || true
must_not_exist_anywhere "$ROOT/trap"
if [ "$(cksum "$OUTSIDE/sentinel.txt")" != "$SENTINEL_BEFORE" ] || \
   [ "$(cksum "$OUTSIDE/sub/keep.txt")" != "$KEEP_BEFORE" ]; then
    echo "  !! E.1 OUTSIDE FILES DESTROYED BY RECURSIVE DELETE"
    FAILS=$((FAILS+1))
fi
run E.2 OK delete /loopdir --recursive || true
must_not_exist_anywhere "$ROOT/loopdir"
run E.3 REFUSE delete /home/documents || true
run E.5 OK delete /home/documents/subdir || true
must_not_exist_anywhere "$ROOT/home/documents/subdir"
run E.4 OK create-dir /home/documents/subdir || true
run E.4 OK delete /home/documents/subdir --recursive || true
must_not_exist_anywhere "$ROOT/home/documents/subdir"
run E.6 REFUSE delete / || true
run E.7 OK delete /home/documents/notes.txt || true
must_not_exist_anywhere "$ROOT/home/documents/notes.txt"
run E.8 REFUSE delete /home/documents/absent.txt || true
run E.9 REFUSE delete /outside-link || true
run E.10 REFUSE delete /../../../etc/passwd || true

echo
echo "=== F. create and write — the ordinary mistakes ==="
printf 'notes\n' > "$ROOT/home/documents/notes.txt"
run F.1 OK write /home/documents/notes.txt --content replaced || true
run F.2 REFUSE write /home/documents/no-such-dir/x --content y || true
must_not_exist_anywhere "$ROOT/home/documents/no-such-dir"
run F.3 OK create-dir /home/newdir/deep/deeper || true
must_exist_inside F.3 "$ROOT/home/newdir/deep/deeper"
run F.4 REFUSE write /home/documents --content x || true
run F.5 REFUSE create-dir /home/documents/notes.txt || true
run F.6 OK create-dir /home/documents || true
run F.7 REFUSE write /home/link-to-file/x --content y || true

echo
echo "=== G. reading and listing the wrong kind of thing ==="
run G.1 REFUSE read /home/documents || true
run G.2 REFUSE list /home/documents/notes.txt || true
run G.3 REFUSE read /home/documents/absent.txt || true
G4_B="$("$BIN" run --root "$ROOT" list /home/documents)"
run G.4 OK list /home/documents || true
G4_A="$("$BIN" run --root "$ROOT" list /home/documents)"
[ "$G4_A" = "$G4_B" ] || { echo "  !! G.4 listing order unstable"; FAILS=$((FAILS+1)); }
run G.5 OK read /inside-link/notes.txt || true

echo
echo "=== I. operations that must work — the boring half ==="
run I.1 OK create-dir /home/work || true
must_exist_inside I.1 "$ROOT/home/work"
run I.2 OK write /home/work/a.txt --content alpha || true
run I.3 OK write /home/work/b.txt --content beta || true
run I.4 OK read /home/work/a.txt || true
run I.5 OK list /home/work || true
run I.6 OK move /home/work/a.txt /home/work/c.txt || true
must_exist_inside I.6 "$ROOT/home/work/c.txt"
must_not_exist_anywhere "$ROOT/home/work/a.txt"
run I.7 OK delete /home/work/c.txt || true
run I.8 OK delete /home/work --recursive || true
must_exist_inside I.8 "$ROOT"
run I.9 OK list / || true
run I.10 OK read /home/documents/notes.txt || true

echo
echo "=== K. symlink-object limitation — recorded, not fixed ==="
"$BIN" run --root "$ROOT" delete /outside-link > /dev/null 2>&1 \
    && echo "[FAIL] [K.1   ] delete /outside-link accepted — should fail-closed" && FAILS=$((FAILS+1)) \
    || echo "[ok  ] [K.1   ] delete /outside-link refuses (fail-closed; link can never be removed)"

echo
OUT_AFTER="$(fingerprint_dir "$OUTSIDE")"
echo "out sentinel after:   $(cksum "$OUTSIDE/sentinel.txt" 2>/dev/null || echo MISSING)"
echo "/etc/passwd after:    $(cksum /etc/passwd) $(stat -c '%Y' /etc/passwd)"
if [ "$OUT_AFTER" != "$OUT_BEFORE" ]; then
    echo "!! OUTSIDE DIRECTORY CHANGED OVER THE RUN"
    FAILS=$((FAILS+1))
fi
if [ "$(cksum /etc/passwd) $(stat -c '%Y' /etc/passwd)" != "$PASSWD_BEFORE" ]; then
    echo "!! HOST /etc/passwd CHANGED OVER THE RUN"
    FAILS=$((FAILS+1))
fi

rm -rf "$ROOT" "$OUTSIDE"
echo
echo "lines run: $OKS   failures: $FAILS"
if [ "$FAILS" -gt 0 ]; then
    echo "hand-test-2a: $FAILS FAILED"
    exit 1
fi
echo "hand-test-2a: every line behaved as required"
exit 0
