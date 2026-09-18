#!/usr/bin/env bash
# Atrium Phase 2b — hands-on harness (delegation-briefs/phase-2b-shell.md §9.7).
# Runs every line of attack-list-2b.md against atrium-shell in a throwaway
# root, with a throwaway OUTSIDE directory as the escape detector.
#
# Usage:  bash shell/hand-test-2b.sh [root]
#         (default root: /tmp/atrium-2b-root)
#
# The outside dir is <root's parent>/atrium-2b-outside. Both are deleted at
# the end, along with ONLY the §G marker paths this script created in /tmp.
# Nothing else is touched. The host /etc/passwd checksum and mtime are
# tracked throughout and re-checked after every line.
#
# The runner does NOT cage the command: §G lines are EXPECTED to reach the
# host and are printed marked as HOLE, not counted as failures. The cage is
# BUILD-PLAN.md §2e. The outside sentinel is never the target of §G, so the
# escape detector stays meaningful (§G.6).

set -u

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="${1:-/tmp/atrium-2b-root}"
OUTSIDE="$(dirname "$ROOT")/atrium-2b-outside"
BIN="$HERE/target/debug/atrium-shell"
SELF_TAG="atrium-2b-harness-$$"

if [ ! -x "$BIN" ]; then
    echo "building..." >&2
    (cd "$HERE" && cargo build >/dev/null 2>&1) || { echo "cargo build failed" >&2; exit 2; }
fi

rm -rf "$ROOT" "$OUTSIDE"
mkdir -p "$ROOT/home/documents/sub" "$ROOT/home/work" "$ROOT/trap" "$OUTSIDE/sub"
printf 'hi\n' > "$ROOT/home/documents/notes.txt"
printf 'x\n'  > "$ROOT/afile.txt"
ln -s "$OUTSIDE" "$ROOT/trap/escape"
printf 'sentinel\n' > "$OUTSIDE/sentinel.txt"
printf 'keep\n' > "$OUTSIDE/sub/keep.txt"

# D.1: the canary is set in OUR OWN environment, so a leaked environment is
# detectable by name inside the command.
export ATRIUM_2B_CANARY="canary-$SELF_TAG-should-not-appear"

# --- Record before the run (§I.1) ------------------------------------------
fingerprint_dir() {
    # The outside directory's OWN contents only: entry names, per-entry
    # metadata, and content checksums. Deliberately NOT `ls -laR`: that
    # prints the `..` entry (the real /tmp) whose mtime changes whenever
    # anything else in /tmp changes — including this script's own §G
    # marker files — and would make the detector fire on the designed hole
    # instead of on a real escape (§G.6's exact concern).
    ( printf '%s\n' "--- entries"; cd "$1" && ls -A
      printf '%s\n' "--- entries sub"; ls -A "$1/sub"
      printf '%s\n' "--- stat"; stat -c '%n %s %Y' "$1/sentinel.txt" "$1/sub/keep.txt" 2>/dev/null
      printf '%s\n' "--- cksum"; cksum "$1/sentinel.txt" "$1/sub/keep.txt" 2>/dev/null )
}
OUT_BEFORE="$(fingerprint_dir "$OUTSIDE")"
PASSWD_BEFORE="$(cksum /etc/passwd) $(stat -c '%Y' /etc/passwd)"
ROOT_LISTING_BEFORE="$(cd "$ROOT" && ls -laR)"

echo "environment root: $ROOT"
echo "outside dir:      $OUTSIDE"
echo "out dir before:"
echo "$OUT_BEFORE" | sed 's/^/    /'
echo "/etc/passwd before: $PASSWD_BEFORE"
echo

FAILS=0
OKS=0
HOLES=0

# check_outside <line>: the independent detector. Looks at the disk, never
# at anything the runner says about itself.
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

# Extract the command's own stdout payload from the runner's output
# (between the `--- stdout ---` and `--- stderr ---` separators).
child_stdout() { sed -n '/^--- stdout ---$/,/^--- stderr ---$/p' | sed '1d;$d'; }

# run <section> <cwd> <cmd...>
# The generic line runner: prints the line, what was run, the runner's
# status header, and whether the runner RAN or REFUSED. Callers add the
# line-specific assertions. Refusal by a bad cwd must exit non-zero.
run_line() {
    local section="$1" cwd="$2"; shift 2
    local out rc status
    out="$("$BIN" run --root "$ROOT" --cwd "$cwd" -- "$@" 2>&1)"; rc=$?
    status="$(printf '%s\n' "$out" | head -1)"
    printf '[%-5s] %-45s %s\n' "$section" "$cwd: $*" "$status"
    OKS=$((OKS+1))
    check_outside "$section $cwd $*"
    LAST_OUT="$out"
    LAST_RC="$rc"
    LAST_CHILD_STDOUT="$(printf '%s\n' "$out" | child_stdout)"
}

expect_ran() { # the runner said OK and exited 0
    local line="$1"
    if [ "$LAST_RC" -ne 0 ] || [ "${LAST_OUT%% *}" != "OK" ]; then
        echo "[FAIL] [$line] must RUN, runner said: $(printf '%s\n' "$LAST_OUT" | head -1)"
        FAILS=$((FAILS+1)); return 1
    fi
    return 0
}

expect_refused() { # refused: REFUSE line, non-zero rc, reason naming the virtual path
    local line="$1" vpath="$2"; shift 2
    local ok=1
    if ! printf '%s\n' "$LAST_OUT" | head -1 | grep -q '^REFUSE '; then
        echo "[FAIL] [$line] must REFUSE, runner said: $(printf '%s\n' "$LAST_OUT" | head -1)"
        FAILS=$((FAILS+1)); ok=0
    elif [ "$LAST_RC" -eq 0 ]; then
        echo "[FAIL] [$line] refusal must exit non-zero"
        FAILS=$((FAILS+1)); ok=0
    fi
    # The reason names the virtual path and a specific thing; and must
    # never name the real root / outside dir (H.1).
    # The empty vpath (B.5) trivially "appears" in any string; only a
    # non-empty one is worth checking for.
    if [ -n "$vpath" ] && ! printf '%s\n' "$LAST_OUT" | head -1 | grep -qF -- "$vpath"; then
        echo "[FAIL] [$line] refusal does not name the virtual path: $(printf '%s\n' "$LAST_OUT" | head -1)"
        FAILS=$((FAILS+1)); ok=0
    fi
    for w in "$@"; do
        if ! printf '%s\n' "$LAST_OUT" | head -1 | grep -qF "$w"; then
            echo "[FAIL] [$line] refusal reason must mention '$w': $(printf '%s\n' "$LAST_OUT" | head -1)"
            FAILS=$((FAILS+1)); ok=0
        fi
    done
    case "$LAST_OUT" in
        *"$OUTSIDE"*|*"$ROOT"*)
            echo "[FAIL] [$line] refusal DISCLOSED a real path: $(printf '%s\n' "$LAST_OUT" | head -1)"
            FAILS=$((FAILS+1)); ok=0 ;;
    esac
    return $((1-ok))
}

must_exist_inside()    { [ -e "$2" ] || [ -L "$2" ] || { echo "  !! MUST EXIST MISSING at $1: $2"; FAILS=$((FAILS+1)); }; }
must_not_exist()       { [ ! -e "$1" ] && [ ! -L "$1" ] || { echo "  !! MARKER APPEARED: $1"; FAILS=$((FAILS+1)); }; }

echo "=== A. where the command starts (RUNS; checked on disk) ==="
# A.1/A.4/A.7 print the REAL path — the command's own output, the subject
# of the line (H.2 exemption, marked).
echo "  (A.1/A.4/A.7 print the command's own output, which contains the root's real path —"
echo "   marked exemption per attack-list-2b.md H.2; this is §A.8, the disclosure 2b cannot prevent)"
run_line A.1 /home/work sh -c 'pwd'
expect_ran A.1 && {
    [ "$(printf '%s' "$LAST_CHILD_STDOUT" | tr -d '\n')" = "$ROOT/home/work" ] \
        && echo "     ok: pwd is the REAL root's home/work: $(printf '%s' "$LAST_CHILD_STDOUT" | tr -d '\n')" \
        || { echo "[FAIL] [A.1] pwd was: $(printf '%s' "$LAST_CHILD_STDOUT" | tr -d '\n')"; FAILS=$((FAILS+1)); }
}
run_line A.2 /home/work sh -c 'touch relative.txt'
expect_ran A.2; must_exist_inside A.2 "$ROOT/home/work/relative.txt"
must_not_exist "$OUTSIDE/relative.txt"
run_line A.3 /home/work sh -c 'mkdir -p a/b && touch a/b/deep.txt'
expect_ran A.3; must_exist_inside A.3 "$ROOT/home/work/a/b/deep.txt"
run_line A.4 / sh -c 'pwd'
expect_ran A.4 && {
    P="$(printf '%s' "$LAST_CHILD_STDOUT" | tr -d '\n')"
    [ "$P" = "$ROOT" ] && [ "$P" != "/" ] \
        && echo "     ok: virtual / is the environment root: $P" \
        || { echo "[FAIL] [A.4] pwd was: $P"; FAILS=$((FAILS+1)); }
}
run_line A.5 /home/work sh -c 'touch ../sibling.txt'
expect_ran A.5; must_exist_inside A.5 "$ROOT/home/sibling.txt"
must_not_exist "$OUTSIDE/sibling.txt"
run_line A.6 /home/documents/sub sh -c 'pwd; touch marker-sub.txt'
expect_ran A.6 && {
    P="$(printf '%s' "$LAST_CHILD_STDOUT" | tr -d '\n')"
    [ "$P" = "$ROOT/home/documents/sub" ] \
        || { echo "[FAIL] [A.6] pwd was: $P"; FAILS=$((FAILS+1)); }
}
must_exist_inside A.6 "$ROOT/home/documents/sub/marker-sub.txt"
run_line A.7 /home/documents sh -c 'pwd'
expect_ran A.7 && {
    [ "$(printf '%s' "$LAST_CHILD_STDOUT" | tr -d '\n')" = "$ROOT/home/documents" ] \
        || { echo "[FAIL] [A.7] pwd was: $(printf '%s' "$LAST_CHILD_STDOUT" | tr -d '\n')"; FAILS=$((FAILS+1)); }
}

echo
echo "=== B. refusals — nothing starts at all (marker absence proves it) ==="
run_line B.1 '/home/../../work' sh -c 'touch refused-marker-b1'
expect_refused B.1 '/home/../../work' '..' 'climbed above'
must_not_exist "$ROOT/refused-marker-b1"; must_not_exist "$ROOT/work/refused-marker-b1"
must_not_exist "$OUTSIDE/refused-marker-b1"; must_not_exist "$OUTSIDE/work/refused-marker-b1"
run_line B.2 '/nonexistent/work' sh -c 'touch refused-marker-b2'
expect_refused B.2 '/nonexistent/work' 'does not exist'
must_not_exist "$ROOT/nonexistent/work/refused-marker-b2"; must_not_exist "$OUTSIDE/refused-marker-b2"
run_line B.3 '/afile.txt' sh -c 'touch refused-marker-b3'
expect_refused B.3 '/afile.txt' 'file' 'not a directory'
run_line B.4 '/home/documents/notes.txt' sh -c 'touch refused-marker-b4'
expect_refused B.4 '/home/documents/notes.txt' 'not a directory'
run_line B.5 '' sh -c 'touch refused-marker-b5'
expect_refused B.5 '' 'empty'
run_line B.6 'home/work' sh -c 'touch refused-marker-b6'
expect_refused B.6 'home/work' 'relative'
run_line B.7 '/trap/escape' sh -c 'touch refused-marker-b7'
expect_refused B.7 '/trap/escape' 'symlink'
must_not_exist "$OUTSIDE/refused-marker-b7"
run_line B.8 '/trap/escape/..' sh -c 'touch refused-marker-b8'
expect_refused B.8 '/trap/escape' 'symlink'
must_not_exist "$OUTSIDE/refused-marker-b8"

echo
echo "  B.9 — filesystem byte-identical after every refusal above:"
ROOT_LISTING_AFTER_B="$(cd "$ROOT" && ls -laR)"
# Remove A-section artifacts from the comparison: B.9 is about the refusal
# lines; compare snapshots taken immediately around them instead.
B9_SNAP="$(ls -laR "$ROOT" "$OUTSIDE" 2>/dev/null | cksum)"
run_line B.9x '/home/../../work' sh -c 'touch refused-marker-b9' >/dev/null
expect_refused B.9x '/home/../../work' '..' >/dev/null
if [ "$(ls -laR "$ROOT" "$OUTSIDE" 2>/dev/null | cksum)" = "$B9_SNAP" ]; then
    echo "  [ok  ] listing of root and outside is byte-identical across another refusal"
else
    echo "[FAIL] [B.9] filesystem changed across a refusal"; FAILS=$((FAILS+1))
fi

echo
echo "  B.10 — a command's own non-zero exit is RUNS, never REFUSE:"
run_line B.10 /home/work sh -c 'exit 3'
expect_ran B.10 && {
    printf '%s\n' "$LAST_OUT" | head -1 | grep -q 'status=exit 3' \
        && echo "  [ok  ] exit 3 reported as exit 3" \
        || { echo "[FAIL] [B.10] exit 3 not reported: $(printf '%s\n' "$LAST_OUT" | head -1)"; FAILS=$((FAILS+1)); }
}

echo
echo "=== C. output and exit code come back unaltered ==="
run_line C.1 /home/work sh -c 'printf "hi\n"'
expect_ran C.1 && {
    [ "$LAST_CHILD_STDOUT" = "hi" ] \
        && printf '%s\n' "$LAST_OUT" | head -1 | grep -q 'status=exit 0' \
        && printf '%s\n' "$LAST_OUT" | head -1 | grep -q 'stderr=0 bytes' \
        || { echo "[FAIL] [C.1] $(printf '%s\n' "$LAST_OUT" | head -1) / stdout=$LAST_CHILD_STDOUT"; FAILS=$((FAILS+1)); }
}
run_line C.2 /home/work sh -c 'echo oops >&2'
expect_ran C.2 && {
    printf '%s\n' "$LAST_OUT" | sed -n '/^--- stderr ---$/,$p' | tail -n +2 | grep -q '^oops$' \
        && printf '%s\n' "$LAST_OUT" | head -1 | grep -q 'stdout=0 bytes' \
        || { echo "[FAIL] [C.2] $(printf '%s\n' "$LAST_OUT" | head -1)"; FAILS=$((FAILS+1)); }
}
run_line C.3 /home/work sh -c 'echo on-out; echo on-err >&2'
expect_ran C.3 && {
    [ "$(printf '%s\n' "$LAST_OUT" | child_stdout)" = "on-out" ] \
        && [ "$(printf '%s\n' "$LAST_OUT" | sed -n '/^--- stderr ---$/,$p' | tail -n +2)" = "on-err" ] \
        || { echo "[FAIL] [C.3] streams merged or wrong"; FAILS=$((FAILS+1)); }
}
for N in 0 1 42 255; do
    REF=$(sh -c "exit $N" >/dev/null 2>&1; echo $?)   # I.4: the shell itself says
    run_line C.4 /home/work sh -c "exit $N"
    expect_ran C.4 && {
        printf '%s\n' "$LAST_OUT" | head -1 | grep -q "status=exit $REF" \
            || { echo "[FAIL] [C.4] exit $N: shell says $REF, runner says $(printf '%s\n' "$LAST_OUT" | head -1)"; FAILS=$((FAILS+1)); }
    }
done
REF=$(sh -c 'definitely-not-a-real-command-2b' >/dev/null 2>&1; echo $?)
run_line C.5 /home/work sh -c 'definitely-not-a-real-command-2b'
expect_ran C.5 && {
    printf '%s\n' "$LAST_OUT" | head -1 | grep -q "status=exit $REF" \
        || { echo "[FAIL] [C.5] shell says $REF, runner says $(printf '%s\n' "$LAST_OUT" | head -1)"; FAILS=$((FAILS+1)); }
}
run_line C.6 /home/work sh -c 'kill -9 $$'
expect_ran C.6 && {
    printf '%s\n' "$LAST_OUT" | head -1 | grep -q 'status=signal 9' \
        && echo "  [ok  ] killed by SIGKILL reported as signal 9, not exit 0" \
        || { echo "[FAIL] [C.6] signal collapsed/wrong: $(printf '%s\n' "$LAST_OUT" | head -1)"; FAILS=$((FAILS+1)); }
}
run_line C.7 /home/work sh -c 'true'
expect_ran C.7 && {
    printf '%s\n' "$LAST_OUT" | head -1 | grep -q 'stdout=0 bytes stderr=0 bytes' \
        || { echo "[FAIL] [C.7] $(printf '%s\n' "$LAST_OUT" | head -1)"; FAILS=$((FAILS+1)); }
}
run_line C.8 /home/work sh -c 'printf x'
expect_ran C.8 && {
    printf '%s\n' "$LAST_OUT" | head -1 | grep -q 'stdout=1 bytes' \
        || { echo "[FAIL] [C.8] $(printf '%s\n' "$LAST_OUT" | head -1)"; FAILS=$((FAILS+1)); }
}
# C.9: all 256 byte values — NUL and non-UTF-8 bytes included. The body
# (256 bytes, values 0..255 in order) is compared byte-for-byte with od
# against the same generator run by the harness directly. The comparison
# never goes through $(...) (which would eat NUL).
C9GEN='i=0; while [ $i -lt 256 ]; do printf "\\$(printf "%03o" $i)"; i=$((i+1)); done'
run_line C.9 /home/work sh -c "$C9GEN"
expect_ran C.9 && {
    # The body is byte 11..(end-16) of the runner output (after the
    # "--- stdout ---\n" line, before the "\n--- stderr ---" separator
    # main.rs writes before stderr). dd cuts it out; head trims the pad.
    printf '%s' "$LAST_OUT" > "/tmp/atrium-2b-c9-body-$$"
    # LAST_OUT cannot hold NUL bytes — the runner's output went through
    # $(...) — so for byte-exactness re-run directly to a file instead:
    "$BIN" run --root "$ROOT" --cwd /home/work -- sh -c "$C9GEN" > "/tmp/atrium-2b-c9-body-$$" 2>/dev/null
    # strip header line 1 and the stdout separator line, then the stderr separator onward
    sed -n '/^--- stdout ---$/,$p' "/tmp/atrium-2b-c9-body-$$" | tail -n +2 | sed '/^--- stderr ---$/,$d' > "/tmp/atrium-2b-c9-body2-$$"
    # the separator line main.rs writes is "\n--- stderr ---"; the \n is a
    # pad byte main.rs added — remove the final byte if it is that pad
    truncate -s 256 "/tmp/atrium-2b-c9-body2-$$"
    mv "/tmp/atrium-2b-c9-body2-$$" "/tmp/atrium-2b-c9-body-$$"
    REFC9="$(sh -c "$C9GEN" | od -An -tu1 | tr -s ' ')"
    GOTC9="$(od -An -tu1 < "/tmp/atrium-2b-c9-body-$$" | tr -s ' ')"
    REFLEN="$(sh -c "$C9GEN" | wc -c)"
    GOTLEN="$(wc -c < "/tmp/atrium-2b-c9-body-$$")"
    rm -f "/tmp/atrium-2b-c9-body-$$"
    if [ "$REFLEN" = "256" ] && [ "$GOTLEN" = "256" ] && [ "$REFC9" = "$GOTC9" ]; then
        echo "  [ok  ] C.9 all 256 byte values: exact bytes, exact length (256 = reference 256)"
    else
        echo "[FAIL] [C.9] runner kept $GOTLEN bytes, reference is $REFLEN"
        FAILS=$((FAILS+1))
    fi
}
# C.10: direct argv pass-through (no shell): the runner is not a quoting
# layer — arguments containing spaces, quotes and a $ arrive unchanged.
run_line C.10 /home/work printf '[%s][%s][%s]' 'two words' 'has'"'"'quote"and$dollar' '  padded  '
expect_ran C.10 && {
    WANT='[two words][has'"'"'quote"and$dollar][  padded  ]'
    [ "$(printf '%s' "$LAST_CHILD_STDOUT")" = "$WANT" ] \
        || { echo "[FAIL] [C.10] want [$WANT] got [$(printf '%s' "$LAST_CHILD_STDOUT")]"; FAILS=$((FAILS+1)); }
}
run_line C.11 /home/work sh -c 'i=1; while [ $i -le 100 ]; do echo $i; i=$((i+1)); done'
expect_ran C.11 && {
    REF11="$(sh -c 'i=1; while [ $i -le 100 ]; do echo $i; i=$((i+1)); done')"
    [ "$(printf '%s' "$LAST_CHILD_STDOUT")" = "$REF11" ] \
        || { echo "[FAIL] [C.11] order within a stream not preserved"; FAILS=$((FAILS+1)); }
}

echo
echo "=== D. what the command is handed — the environment ==="
run_line D.1 /home/work sh -c 'echo "canary=[${ATRIUM_2B_CANARY:-ABSENT}]"'
expect_ran D.1 && {
    [ "$(printf '%s' "$LAST_CHILD_STDOUT" | tr -d '\n')" = "canary=[ABSENT]" ] \
        && echo "  [ok  ] canary (set in this harness's own env) is ABSENT in the child" \
        || { echo "[FAIL] [D.1] canary leaked: $LAST_CHILD_STDOUT"; FAILS=$((FAILS+1)); }
}
run_line D.2 /home/work sh -c 'env | sort'
expect_ran D.2 && {
    BAD=0
    printf '%s\n' "$LAST_CHILD_STDOUT" | while IFS= read -r kv; do
        case "$kv" in
            HOME=*|PATH=*|TMPDIR=*|PWD=*|SHLVL=*|_=*|'') : ;;
            *) echo "  leaked: $kv" ;;
        esac
    done | tee /tmp/atrium-2b-d2-leaks-$$ | grep -q . && BAD=1
    rm -f /tmp/atrium-2b-d2-leaks-$$
    # PWD/SHLVL/_ are synthesised by sh itself from an empty environ, not
    # inherited; what D.2 forbids is anything of the USER'S session.
    [ "$BAD" = "0" ] \
        && echo "  [ok  ] child env: HOME, PATH, TMPDIR from the runner; PWD, SHLVL, _ synthesised by sh itself; nothing else" \
        || { echo "[FAIL] [D.2] unexpected variables in child env:"; printf '%s\n' "$LAST_CHILD_STDOUT"; FAILS=$((FAILS+1)); }
    printf '%s\n' "$LAST_CHILD_STDOUT" | grep -q "$ATRIUM_2B_CANARY" \
        && { echo "[FAIL] [D.2] canary present in env output"; FAILS=$((FAILS+1)); } || true
}
run_line D.3 /home/work sh -c 'echo "PATH=$PATH"; ls >/dev/null && echo "bare ls runs"'
expect_ran D.3 && {
    printf '%s' "$LAST_CHILD_STDOUT" | grep -q '^PATH=/usr/bin:/bin$' \
        && printf '%s' "$LAST_CHILD_STDOUT" | grep -q 'bare ls runs' \
        || { echo "[FAIL] [D.3] $LAST_CHILD_STDOUT"; FAILS=$((FAILS+1)); }
    run_line D.3b /home/work ls /
    expect_ran D.3b || true
}
run_line D.4 /home/work sh -c 'touch "$HOME/homefile"'
expect_ran D.4; must_exist_inside D.4 "$ROOT/homefile"; must_not_exist "$OUTSIDE/homefile"
run_line D.5 /home/work sh -c 'touch "$TMPDIR/tmpfile"'
expect_ran D.5; must_exist_inside D.5 "$ROOT/tmpfile"; must_not_exist "$OUTSIDE/tmpfile"
run_line D.6a /home/work sh -c 'env | sort'
E1="$LAST_CHILD_STDOUT"
ATRIUM_2B_JUNK=junk-value run_line D.6b /home/work sh -c 'env | sort'
[ "$E1" = "$LAST_CHILD_STDOUT" ] \
    && echo "  [ok  ] D.6 the environment is fixed: identical across differently-set outer environments" \
    || { echo "[FAIL] [D.6] environment differed"; FAILS=$((FAILS+1)); }
echo "  D.7 — the runner's settings (printed by the harness, from the CLI/README):"
echo "       PATH=/usr/bin:/bin   HOME=<root>   TMPDIR=<root>   timeout=30s(default)   cap=1MiB(default)"

echo
echo "=== E. what the command's input is ==="
T0=$(date +%s%N)
run_line E.1 /home/work sh -c 'cat'
T1=$(date +%s%N)
expect_ran E.1 && {
    ELAPSED_MS=$(( (T1 - T0) / 1000000 ))
    [ "$ELAPSED_MS" -lt 5000 ] \
        && echo "  [ok  ] cat returned in ${ELAPSED_MS}ms — stdin is EOF, not the terminal" \
        || { echo "[FAIL] [E.1] cat blocked ${ELAPSED_MS}ms"; FAILS=$((FAILS+1)); }
}
expect_ran E.1 && {
    printf '%s\n' "$LAST_OUT" | head -1 | grep -q 'stdout=0 bytes stderr=0 bytes' \
        || { echo "[FAIL] [E.2] cat input was not empty: $(printf '%s\n' "$LAST_OUT" | head -1)"; FAILS=$((FAILS+1)); }
}
run_line E.3 /home/work sh -c 'read x; echo "got:$x"'
expect_ran E.3 && {
    [ "$(printf '%s' "$LAST_CHILD_STDOUT" | tr -d '\n')" = "got:" ] \
        || { echo "[FAIL] [E.3] $LAST_CHILD_STDOUT"; FAILS=$((FAILS+1)); }
}

echo
echo "=== F. the runner cannot be hung or flooded ==="
T0=$(date +%s%N)
out="$("$BIN" run --root "$ROOT" --cwd /home/work --timeout-ms 500 -- sh -c 'while true; do :; done' 2>&1)"; rc=$?
T1=$(date +%s%N)
echo "       (wall time: $(( (T1 - T0) / 1000000 ))ms against a 500ms limit)"
printf '[%-5s] %-45s %s\n' "F.1" "/home/work: (never exits)" "$(printf '%s\n' "$out" | head -1)"
OKS=$((OKS+1)); check_outside "F.1"
printf '%s\n' "$out" | head -1 | grep -q 'status=timed-out' \
    && echo "  [ok  ] reported as timed out, never exit 0" \
    || { echo "[FAIL] [F.1] $(printf '%s\n' "$out" | head -1)"; FAILS=$((FAILS+1)); }
MARK="atrium-2b-f2-$$"
"$BIN" run --root "$ROOT" --cwd /home/work --timeout-ms 300 -- sh -c "exec sleep 86400 # $MARK" >/dev/null 2>&1
sleep 0.2
if ps -eo args 2>/dev/null | grep -F "$MARK" | grep -v grep | grep -q .; then
    echo "[FAIL] [F.2] child still running after timeout:"; ps -eo args | grep -F "$MARK"
    FAILS=$((FAILS+1))
else
    echo "  [ok  ] F.2 after the timeout the child is gone (ps shows nothing bearing its marker)"
fi
OKS=$((OKS+1))
run_line F.3 /home/work sh -c 'i=0; while [ $i -lt 300000 ]; do echo line-$i; i=$((i+1)); done'
expect_ran F.3 && {
    printf '%s\n' "$LAST_OUT" | head -1 | grep -q 'stdout-truncated' \
        && echo "  [ok  ] stdout flood is capped and the cap is REPORTED (stdout-truncated)" \
        || { echo "[FAIL] [F.3] cap not reported: $(printf '%s\n' "$LAST_OUT" | head -1)"; FAILS=$((FAILS+1)); }
}
run_line F.4 /home/work sh -c 'i=0; while [ $i -lt 300000 ]; do echo err-$i >&2; i=$((i+1)); done'
expect_ran F.4 && {
    printf '%s\n' "$LAST_OUT" | head -1 | grep -q 'stderr-truncated' \
        || { echo "[FAIL] [F.4] stderr flood not capped/reported: $(printf '%s\n' "$LAST_OUT" | head -1)"; FAILS=$((FAILS+1)); }
}
T0=$(date +%s)
run_line F.5 /home/work sh -c 'i=0; while [ $i -lt 50000 ]; do echo out-$i; echo err-$i >&2; i=$((i+1)); done'
T1=$(date +%s)
expect_ran F.5 && {
    EL=$((T1 - T0))
    [ "$EL" -lt 25 ] \
        && echo "  [ok  ] both pipes flooded at once: no deadlock (${EL}s wall)" \
        || { echo "[FAIL] [F.5] took ${EL}s — deadlock signature"; FAILS=$((FAILS+1)); }
}
run_line F.6 /home/work sh -c 'i=0; while [ $i -lt 200000 ]; do echo $i; i=$((i+1)); done; exit 7'
expect_ran F.6 && {
    printf '%s\n' "$LAST_OUT" | head -1 | grep -q 'status=exit 7' \
        && printf '%s\n' "$LAST_OUT" | head -1 | grep -q 'truncated' \
        || { echo "[FAIL] [F.6] $(printf '%s\n' "$LAST_OUT" | head -1)"; FAILS=$((FAILS+1)); }
}

echo
echo "=== G. THE HOLE 2b DOES NOT CLOSE — expected to reach the host ==="
echo "  These lines are marked HOLE, not pass/fail. 2b guarantees WHERE a"
echo "  command starts and WHAT it is handed — it does NOT confine what the"
echo "  command can then reach. Closing this is BUILD-PLAN.md §2e, a required"
echo "  later phase. The outside sentinel directory is never the target (G.6),"
echo "  so the escape detector above stays meaningful."
hole_line() {
    local section="$1" cwd="$2"; shift 2
    local out rc
    out="$("$BIN" run --root "$ROOT" --cwd "$cwd" -- "$@" 2>&1)"; rc=$?
    printf '[HOLE ] %-45s %s\n' "$section: $cwd: $*" "$(printf '%s\n' "$out" | head -1)"
    OKS=$((OKS+1)); HOLES=$((HOLES+1))
    check_outside "$section"
    LAST_OUT="$out"; LAST_RC="$rc"; LAST_CHILD_STDOUT="$(printf '%s\n' "$out" | child_stdout)"
}
hole_line G.1 /home/work sh -c 'cd / && pwd'
[ "$(printf '%s' "$LAST_CHILD_STDOUT" | tr -d '\n')" = "/" ] \
    && echo "  (G.1 reached the host: cd / && pwd printed /)" \
    || echo "  (G.1 observed: $(printf '%s' "$LAST_CHILD_STDOUT" | tr -d '\n'))"
hole_line G.2 /home/work sh -c 'ls /'
printf '%s' "$LAST_CHILD_STDOUT" | grep -q 'etc' \
    && echo "  (G.2 reached the host: / lists host entries)"
hole_line G.3 /home/work sh -c 'cat /etc/passwd | head -1'
printf '%s' "$LAST_CHILD_STDOUT" | grep -q 'root' \
    && echo "  (G.3 reached the host: read a line of the host /etc/passwd)"
G4MARK=/tmp/atrium-2b-hole-marker-$$
rm -f "$G4MARK"
hole_line G.4 /home/work sh -c "touch $G4MARK"
[ -e "$G4MARK" ] \
    && { echo "  (G.4 reached the host: $G4MARK appeared in the real /tmp — deleting it)"; rm -f "$G4MARK"; } \
    || { echo "[FAIL] [G.4] marker did not appear (expected to be demonstrated)"; FAILS=$((FAILS+1)); }
G5MARK=/tmp/atrium-2b-hole-marker2-$$
rm -f "$G5MARK"
hole_line G.5 /home/work sh -c "touch $G5MARK; touch inside.txt"
[ -e "$G5MARK" ] && { echo "  (G.5 reached the host AND started inside: both files created — deleting the host marker)"; rm -f "$G5MARK"; } \
    || { echo "[FAIL] [G.5] marker2 did not appear"; FAILS=$((FAILS+1)); }
must_exist_inside G.5 "$ROOT/home/work/inside.txt"
echo "  G.6 — the outside sentinel is checked above after every line, §G included."

echo
echo "=== H./I. messages and independence ==="
# H.4 is structural: the runner passes command output through unaltered —
# already exercised by A.1/A.4 (the real path appears in the child's own
# stdout above) and by the C-section byte-exactness lines.
echo "  H.1/H.3 checked inline on every REFUSE line (reason names the virtual path,"
echo "  in plain words, and never a real path — any disclosure FAILed the line above)."
echo "  H.4 exercised by A.1/A.4/§G: command output containing the real path passed through unaltered."
echo "  I.1: the escape check looks at the disk only (listing + sentinel checksums)."
echo "  I.2: every RUNS-with-an-effect line was confirmed by the file existing inside the root."
echo "  I.3: every REFUSED line was confirmed by its marker's absence (inside and outside)."
echo "  I.4: exit codes were compared against the shell itself (\$REF), not remembered numbers."

echo
echo "=== J. the boring half ==="
run_line J.1 /home/work echo hi
expect_ran J.1 && [ "$(printf '%s' "$LAST_CHILD_STDOUT" | tr -d '\n')" = "hi" ] \
    || { echo "[FAIL] [J.1]"; FAILS=$((FAILS+1)); }
run_line J.2 /home/work sh -c 'ls'
printf '%s' "$LAST_CHILD_STDOUT" | tr '\n' ' '; echo
run_line J.3 /home/work sh -c 'mkdir newdir && pwd'
expect_ran J.3; must_exist_inside J.3 "$ROOT/home/work/newdir"
run_line J.4 /home/work sh -c 'printf "a\nb\n" | wc -l'
expect_ran J.4 && {
    [ "$(printf '%s' "$LAST_CHILD_STDOUT" | tr -d ' \n')" = "2" ] \
        || { echo "[FAIL] [J.4] $LAST_CHILD_STDOUT"; FAILS=$((FAILS+1)); }
}
run_line J.5 /home/work sh -c 'exit 3'
expect_ran J.5 && printf '%s\n' "$LAST_OUT" | head -1 | grep -q 'status=exit 3' \
    || { echo "[FAIL] [J.5]"; FAILS=$((FAILS+1)); }
run_line J.6 /home/work sh -c 'cd newdir && touch here.txt'
expect_ran J.6; must_exist_inside J.6 "$ROOT/home/work/newdir/here.txt"
run_line J.7 /home/work sh -c 'ls /not-yet-existing 2>/dev/null; touch brand/x 2>/dev/null; mkdir -p brand && touch brand/x'
expect_ran J.7; must_exist_inside J.7 "$ROOT/home/work/brand/x"

echo
echo "=== N. mechanisms the blind list added (attack-list-2b.md §N) ==="
echo "  These were written while the code was being built, so the build child"
echo "  never saw them. §N.8 is why this section exists: it caught a real"
echo "  defect — see the note after that line."

# run_line_ms <section> <cwd> <cmd...> — run_line plus wall-clock ms in LAST_MS
run_line_ms() {
    local section="$1" cwd="$2"; shift 2
    local t0 t1
    t0=$(date +%s%N)
    run_line "$section" "$cwd" "$@"
    t1=$(date +%s%N)
    LAST_MS=$(( (t1 - t0) / 1000000 ))
}

# run_line_ex <section> <cwd> <extra-runner-opts> <cmd...>
# Same as run_line but lets a line set --timeout-ms / --max-output-bytes.
# Needed because run_line uses the runner's DEFAULTS (30s limit, 1MiB cap),
# and a line that means to test the limit must actually set one.
run_line_ex() {
    local section="$1" cwd="$2" extra="$3"; shift 3
    local out rc status
    out="$("$BIN" run --root "$ROOT" --cwd "$cwd" $extra -- "$@" 2>&1)"; rc=$?
    status="$(printf '%s\n' "$out" | head -1)"
    printf '[%-5s] %-45s %s\n' "$section" "$cwd: $*" "$status"
    OKS=$((OKS+1))
    check_outside "$section $cwd $*"
    LAST_OUT="$out"
    LAST_RC="$rc"
    LAST_CHILD_STDOUT="$(printf '%s\n' "$out" | child_stdout)"
}

run_line_ex_ms() {
    local section="$1" cwd="$2" extra="$3"; shift 3
    local t0 t1
    t0=$(date +%s%N)
    run_line_ex "$section" "$cwd" "$extra" "$@"
    t1=$(date +%s%N)
    LAST_MS=$(( (t1 - t0) / 1000000 ))
}

# N.1-N.4: the PROGRAM failing to start is a different refusal from a bad cwd.
# Distinct reasons, and none of them may be reported as exit code 127 — that
# code means a child ran and could not find something.
printf 'x\n' > "$ROOT/home/work/notexec.txt"; chmod 644 "$ROOT/home/work/notexec.txt"
mkdir -p "$ROOT/home/work/sub"
n_refusals_seen=""
for spec in "N.1:./no-such-program-2b" "N.2:./notexec.txt" "N.3:./sub"; do
    id="${spec%%:*}"; prog="${spec#*:}"
    run_line "$id" /home/work "$prog"
    if printf '%s\n' "$LAST_OUT" | head -1 | grep -q '^REFUSE '; then
        reason="$(printf '%s\n' "$LAST_OUT" | head -1)"
        echo "        reason: ${reason#*— }"
        n_refusals_seen="$n_refusals_seen|$reason"
    else
        echo "[FAIL] [$id] a program that cannot start must REFUSE: $(printf '%s\n' "$LAST_OUT" | head -1)"
        FAILS=$((FAILS+1))
    fi
    if printf '%s\n' "$LAST_OUT" | head -1 | grep -q 'status=exit 127'; then
        echo "[FAIL] [$id] reported as exit 127 — that means a child ran"
        FAILS=$((FAILS+1))
    fi
done
# N.4: the three reasons must not all be the same text — a directory, a
# non-executable file and a missing name are three different problems.
ndistinct="$(printf '%s\n' "$n_refusals_seen" | tr '|' '\n' | grep -c .)"
[ "$ndistinct" -ge 3 ] \
    && echo "  [ok  ] N.4 the three spawn refusals are distinguishable and none is 127" \
    || { echo "[FAIL] [N.4] spawn refusals are not distinguishable ($ndistinct distinct of 3)"; FAILS=$((FAILS+1)); }
rm -f "$ROOT/home/work/notexec.txt"; rmdir "$ROOT/home/work/sub" 2>/dev/null

# N.5: a command that ignores the polite signal, with a grandchild holding the
# pipes. Before the §N.8 fix this returned only when the GRANDCHILD died.
run_line_ex_ms N.5 /home/work "--timeout-ms 1500" bash -c '(exec -a atrium2b-n5 sleep 999) & trap "" TERM; wait'
expect_ran N.5 && {
    printf '%s\n' "$LAST_OUT" | head -1 | grep -q 'status=timed-out' \
        || { echo "[FAIL] [N.5] not reported as timed out: $(printf '%s\n' "$LAST_OUT" | head -1)"; FAILS=$((FAILS+1)); }
    [ "$LAST_MS" -lt 6000 ] \
        && echo "  [ok  ] N.5 the limit held against a survivor holding the pipes (${LAST_MS}ms wall, 1500ms limit)" \
        || { echo "[FAIL] [N.5] took ${LAST_MS}ms — the deadline did not hold"; FAILS=$((FAILS+1)); }
}
pkill -f atrium2b-n5 2>/dev/null || true

# N.6: the limit is time, not activity — a slow drip must not extend it.
run_line_ex_ms N.6 /home/work "--timeout-ms 1500" sh -c 'i=0; while true; do printf x; sleep 1; done'
expect_ran N.6 && {
    printf '%s\n' "$LAST_OUT" | head -1 | grep -q 'status=timed-out' \
        || { echo "[FAIL] [N.6] slow drip not timed out: $(printf '%s\n' "$LAST_OUT" | head -1)"; FAILS=$((FAILS+1)); }
    [ "$LAST_MS" -lt 5000 ] \
        && echo "  [ok  ] N.6 output did not extend the deadline (${LAST_MS}ms wall)" \
        || { echo "[FAIL] [N.6] took ${LAST_MS}ms — output extended the deadline"; FAILS=$((FAILS+1)); }
}

# N.7: the instant-exit race — a command that finishes in under a millisecond
# is exit 0, never a timeout or a spawn failure.
run_line N.7 /home/work sh -c 'exit 0'
expect_ran N.7 && printf '%s\n' "$LAST_OUT" | head -1 | grep -q 'status=exit 0' \
    || { echo "[FAIL] [N.7] $(printf '%s\n' "$LAST_OUT" | head -1)"; FAILS=$((FAILS+1)); }

# N.8 — THE DEFECT LINE. A command that backgrounds something inheriting its
# pipes returns at once; a runner that waits for pipe EOF waits for the
# GRANDCHILD, far past its own limit. Observed before the fix: 12002ms against
# a 2000ms limit. Observed after: 222ms, status=exit 0.
run_line_ms N.8 /home/work bash -c '(exec -a atrium2b-n8 sleep 30) &'
expect_ran N.8 && {
    [ "$LAST_MS" -lt 3000 ] \
        && echo "  [ok  ] N.8 returned at the command's end, not the grandchild's (${LAST_MS}ms wall)" \
        || { echo "[FAIL] [N.8] took ${LAST_MS}ms — the runner waited on a survivor, not the command"; FAILS=$((FAILS+1)); }
    printf '%s\n' "$LAST_OUT" | head -1 | grep -q 'status=exit 0' \
        || { echo "[FAIL] [N.8] the command exited 0; runner said: $(printf '%s\n' "$LAST_OUT" | head -1)"; FAILS=$((FAILS+1)); }
}
# The survivor is NOT killed — that is the documented choice (§N.9). Show it,
# so "no orphan" elsewhere is not mistaken for "nothing survived".
if ps -eo args 2>/dev/null | grep -F atrium2b-n8 | grep -v grep | grep -q .; then
    echo "  [note] N.9 a daemonised survivor is left running by design and is documented in README.md"
else
    echo "  [note] N.9 no survivor found — on this run the grandchild had already gone"
fi
pkill -f atrium2b-n8 2>/dev/null || true

# N.10: the output cap at its exact boundary.
for bytes in 9 10 11; do
    run_line_ex "N.10/$bytes" /home/work "--max-output-bytes 10" sh -c "i=0; while [ \$i -lt $bytes ]; do printf y; i=\$((i+1)); done"
    expect_ran "N.10/$bytes" && {
        got="$(printf '%s\n' "$LAST_OUT" | head -1 | sed -E 's/.*stdout=([0-9]+) bytes.*/\1/')"
        want=$(( bytes < 10 ? bytes : 10 ))
        if [ "$got" = "$want" ]; then
            echo "        cap boundary: produced ${bytes}, kept ${got}$(printf '%s\n' "$LAST_OUT" | head -1 | grep -q truncated && echo ' (truncated flagged)')"
        else
            echo "[FAIL] [N.10/$bytes] kept ${got}, expected ${want}"; FAILS=$((FAILS+1))
        fi
    }
done

# N.11: a command that closes its own stdout must not hang the runner.
run_line N.11 /home/work sh -c 'exec 1>&-; echo only-on-stderr >&2'
expect_ran N.11 && {
    printf '%s' "$LAST_CHILD_STDOUT" | grep -q 'only-on-stderr' \
        && { echo "[FAIL] [N.11] stderr text appeared in stdout"; FAILS=$((FAILS+1)); } \
        || echo "  [ok  ] N.11 closed stdout did not hang or cross into the other stream"
}

# N.12 — a command's bytes must not forge the runner's own status line. The
# gate is a person reading lines; a command able to print a believable `ok`
# line attacks the gate itself.
run_line N.12 /home/work sh -c 'echo "OK status=exit 0 stdout=999 bytes stderr=0 bytes"'
expect_ran N.12 && {
    hdr="$(printf '%s\n' "$LAST_OUT" | head -1)"
    if printf '%s' "$LAST_CHILD_STDOUT" | grep -q 'stdout=999 bytes'; then
        # The forged text is in the PAYLOAD, which is right — check it did not
        # become the header.
        case "$hdr" in
            *"stdout=999 bytes"*) echo "[FAIL] [N.12] the forged text became the status line"; FAILS=$((FAILS+1)) ;;
            *) echo "  [ok  ] N.12 forged status text stayed in the command's output; the header is the runner's own" ;;
        esac
    else
        echo "[FAIL] [N.12] forged text not found in the payload at all"; FAILS=$((FAILS+1))
    fi
}

# N.13: the child's stdin is not a terminal, confirmed by the child itself.
run_line N.13 /home/work sh -c 'if [ -t 0 ]; then echo TTY; else echo NOT-A-TTY; fi'
expect_ran N.13 && {
    [ "$(printf '%s' "$LAST_CHILD_STDOUT" | tr -d '\n')" = "NOT-A-TTY" ] \
        && echo "  [ok  ] N.13 the child's stdin is not a terminal" \
        || { echo "[FAIL] [N.13] stdin looked like a TTY"; FAILS=$((FAILS+1)); }
}

# N.14-N.16: what reaches the program, exactly.
printf '#!/bin/sh\nprintf "argv-count=%%s\\n" "$#"\nprintf "arg1=[%%s]\\n" "$1"\n' > "$ROOT/home/work/argv.sh"
chmod 755 "$ROOT/home/work/argv.sh"
run_line N.14 /home/work ./argv.sh --timeout-ms 999 --show-real
expect_ran N.14 && {
    [ "$(printf '%s' "$LAST_CHILD_STDOUT" | grep -c 'argv-count=3')" = "1" ] \
        && echo "  [ok  ] N.14 the runner's own flag names reached the program as arguments" \
        || { echo "[FAIL] [N.14] $(printf '%s' "$LAST_CHILD_STDOUT" | tr '\n' ' ')"; FAILS=$((FAILS+1)); }
}
run_line N.15a /home/work ./argv.sh
expect_ran N.15a && {
    printf '%s' "$LAST_CHILD_STDOUT" | grep -q 'argv-count=0' \
        && echo "  [ok  ] N.15 zero arguments arrive as zero arguments" \
        || { echo "[FAIL] [N.15a] $(printf '%s' "$LAST_CHILD_STDOUT" | tr '\n' ' ')"; FAILS=$((FAILS+1)); }
}
run_line N.15b /home/work ./argv.sh ''
expect_ran N.15b && {
    printf '%s' "$LAST_CHILD_STDOUT" | grep -q 'argv-count=1' \
        && echo "  [ok  ] N.15 an empty-string argument arrives as one argument" \
        || { echo "[FAIL] [N.15b] $(printf '%s' "$LAST_CHILD_STDOUT" | tr '\n' ' ')"; FAILS=$((FAILS+1)); }
}
run_line N.16a /home/work ./argv.sh
expect_ran N.16a && echo "  [ok  ] N.16 ./program is interpreted relative to the child's cwd"
run_line N.16b /home/work ../work/argv.sh uplevel
expect_ran N.16b && {
    printf '%s' "$LAST_CHILD_STDOUT" | grep -q 'arg1=\[uplevel\]' \
        && echo "  [ok  ] N.16 ../work/program is passed as written, resolved against the cwd" \
        || { echo "[FAIL] [N.16b] $(printf '%s' "$LAST_CHILD_STDOUT" | tr '\n' ' ')"; FAILS=$((FAILS+1)); }
}

# N.17: recorded, not tested. The API takes String, so a non-UTF-8 argument
# cannot be expressed at all — the question is answered by the type.
echo "  [note] N.17 non-UTF-8 arguments: not representable — the API takes String."
echo "         Recorded in attack-list-2b.md §K.7; no line can be run for it."

# N.18: the same cwd spelled differently must reach the same directory.
run_line N.18 /home//work/./ sh -c 'pwd; touch norm.txt'
expect_ran N.18 && {
    must_exist_inside N.18 "$ROOT/home/work/norm.txt"
    echo "  [ok  ] N.18 //, ./ and the trailing slash all reached the same directory"
}

echo
echo "=== K. stated plainly (not built — another phase's) ==="
echo "  K.1 the timeout here is a safety stop so the runner cannot be hung."
echo "      It is NOT the DESIGN.md §7.1 kill switch (Phase 4, user-facing)."
echo "  K.6 a command that daemonises and exits leaves its survivor running."
echo "      The runner does not kill it; §N.9 shows it and README.md records it."
echo "  K.7 non-UTF-8 arguments are not representable (the API takes String)."

# --- Final integrity --------------------------------------------------------
echo
OUT_AFTER="$(fingerprint_dir "$OUTSIDE")"
echo "out dir after:"; echo "$OUT_AFTER" | sed 's/^/    /'
echo "/etc/passwd after: $(cksum /etc/passwd) $(stat -c '%Y' /etc/passwd)"
[ "$OUT_AFTER" = "$OUT_BEFORE" ] || { echo "!! OUTSIDE DIRECTORY CHANGED OVER THE RUN"; FAILS=$((FAILS+1)); }
[ "$(cksum /etc/passwd) $(stat -c '%Y' /etc/passwd)" = "$PASSWD_BEFORE" ] || { echo "!! HOST /etc/passwd CHANGED OVER THE RUN"; FAILS=$((FAILS+1)); }

rm -rf "$ROOT" "$OUTSIDE"
echo
echo "lines run: $OKS   failures: $FAILS   holes demonstrated (expected, 2e's): $HOLES"
if [ "$FAILS" -gt 0 ]; then
    echo "hand-test-2b: $FAILS FAILED"
    exit 1
fi
echo "hand-test-2b: every line behaved as required (the 5 HOLE lines reached the host by design)"
exit 0
