#!/usr/bin/env bash
# Atrium Phase 1 — hands-on attack pass.
#
# Runs every line of attack-list.md sections A-I against the resolver, in a
# throwaway environment root built by the resolver's own `fixtures` mode.
#
# Usage:  bash hand-test.sh [root]
#         (default root: /tmp/atrium-handtest)
#
# Read-only with respect to the project: it only creates files under the
# throwaway root and deletes that root at the end.

set -u

ROOT="${1:-/tmp/atrium-handtest}"
BIN="./target/debug/atrium-resolver"

if [ ! -x "$BIN" ]; then
    echo "building..." >&2
    cargo build >/dev/null 2>&1 || { echo "cargo build failed" >&2; exit 2; }
fi

rm -rf "$ROOT"
mkdir -p "$ROOT"
"$BIN" fixtures --root "$ROOT" >/dev/null || { echo "fixtures failed" >&2; exit 2; }

echo "environment root: $ROOT"
echo

FAILS=0
ACCEPTS=0
REJECTS=0

# run <section> <expected: R|A|I> <path>
run() {
    local section="$1" want="$2" path="$3"
    local out rc verdict detail
    out="$("$BIN" resolve --root "$ROOT" "$path" 2>&1)"
    rc=$?
    verdict="${out%% *}"
    detail="${out#* }"

    local mark="ok  "
    if [ "$want" = "R" ] && [ "$verdict" != "REJECT" ]; then mark="FAIL"; FAILS=$((FAILS+1)); fi
    if [ "$want" = "A" ] && [ "$verdict" != "ACCEPT" ]; then mark="FAIL"; FAILS=$((FAILS+1)); fi

    # Independent containment check: any ACCEPT must sit under the root.
    if [ "$verdict" = "ACCEPT" ]; then
        ACCEPTS=$((ACCEPTS+1))
        case "$detail" in
            "$ROOT"*) : ;;
            *) mark="FAIL"; FAILS=$((FAILS+1)); echo "  !! ESCAPE FROM ROOT: $detail" ;;
        esac
    else
        REJECTS=$((REJECTS+1))
    fi
    if [ "$rc" != "0" ] && [ "$rc" != "1" ]; then
        mark="FAIL"; FAILS=$((FAILS+1))
    fi

    printf '[%s] [%-1s %-6s] %-46s %s\n' "$mark" "$section" "$verdict" "$path" "$detail"
}

echo "=== A. plain traversal (expect REJECT) ==="
run A R '/..'
run A R '/../..'
run A R '/../../..'
run A R '/../../../../../../../../../../..'
run A R '/home/../..'
run A R '/home/documents/../../..'
run A R '/home/documents/../../../etc/passwd'
run A R '/./../.'
run A R '/a/./../../..'
run A R '/a/b/c/../../../..'
run A R '/a/b/../b/../b/../../..'

echo
echo "=== B. absolute host paths (expect REJECT) ==="
run B R '/etc/passwd'
run B R '/etc/shadow'
run B R '/home/muffin'
run B R '/home/muffin/.ssh/id_rsa'
run B R '/root'
run B R '/proc/self/environ'
run B R '/proc/self/cwd'
run B R '/dev/null'
run B R '/sys/class'
run B R '/var/log/syslog'

echo
echo "=== C. relative paths (expect REJECT) ==="
run C R 'home/documents'
run C R './home/documents'
run C R '../home'
run C R '..'
run C R '.'
run C R 'documents/../../..'

echo
echo "=== D. symlinks (expect REJECT) ==="
run D R '/link-to-etc'
run D R '/link-to-home'
run D R '/link-to-root'
run D R '/link-to-parent'
run D R '/chain-a'
run D R '/chain-deep'
run D R '/loop-a'
run D R '/dir-link/file.txt'
run D R '/good-dir/inner-link'
run D R '/link-to-etc/../home/documents'

echo
echo "=== E. separator / normalisation (expect REJECT) ==="
run E R '\'
run E R '\..\..'
run E R '/home\..\..'
run E R '/home/documents\..\..'
run E R '/home//../..'
echo "  -- the five lines that were listed here as REJECT until 12 Sep 2026."
echo "     They are lexically identical to section-I accepts, so they have been"
echo "     MOVED to section I in attack-list.md. Expect ACCEPT:"
run I A '//'
run I A '///home///documents'
run I A '/home/documents//'
run I A '/home/./documents/.'
run I A '/home/documents/..'

echo
echo "=== F. unicode / lookalikes (expect REJECT) ==="
run F R '/home⁄documents'
run F R '/home∕documents'
run F R '/home／documents'
run F R '/home/‮documents'
run F R '/home/doc‮uments'
run F R '/﻿'
run F R '/home/﻿documents'
# The two café lines below look identical and are different byte sequences:
# one composed (U+00E9), one decomposed (e + U+0301). Built with printf so
# the bytes are exactly what the list describes.
COMPOSED="$(printf '/caf\xc3\xa9/documents')"
DECOMPOSED="$(printf '/cafe\xcc\x81/documents')"
run F R "$COMPOSED"
run F R "$DECOMPOSED"
echo "  -- composed  bytes: $(printf '%s' "$COMPOSED" | od -An -tx1 | tr -d '\n')"
echo "  -- decomposed bytes: $(printf '%s' "$DECOMPOSED" | od -An -tx1 | tr -d '\n')"
echo "  -- NUL-byte lines CANNOT be passed as command-line arguments at all"
echo "     (argv is NUL-terminated). They are covered only by cargo test:"
echo "     section_f_unicode_and_lookalikes and section_h_nasty_combinations."

echo
echo "=== G. length and shape (expect REJECT, except the / contradiction) ==="
LONG="/$(printf 'a%.0s' $(seq 1 300))"
DEEP="/a"
for _ in $(seq 1 199); do DEEP="$DEEP/a"; done
C255="/$(printf 'b%.0s' $(seq 1 255))"
C256="/$(printf 'c%.0s' $(seq 1 256))"
echo "  -- (LONG is one component of ${#LONG} chars incl. slash; DEEP is $(( $(tr -cd '/' <<<"$DEEP" | wc -c) )) components)"
run G R "$LONG"
run G R "$DEEP"
run G R "$C255"
run G R "$C256"
run G R ''
run G R ' '
run G R '/   '
run G R '/home/documents.'
run G R '/home/documents...'
run G R '/home/documents '
run G R '/home/ documents'
echo "  -- '/' was listed in section G as REJECT until 12 Sep 2026, contradicting"
echo "     section I. It is an ACCEPT; the G line has been removed. Expect ACCEPT:"
run I A '/'

echo
echo "=== H. nasty combinations (expect REJECT) ==="
run H R '/link-to-etc/../../..'
run H R '/home/../link-to-root/home/muffin'
run H R '//../..'
run H R '/home/documents/../../../'
run H R '/./link-to-parent/..'
echo "  -- '/home/NUL/../..' not passable via argv; covered by cargo test."

echo
echo "=== I. must be ACCEPTED ==="
run I A '/'
run I A '/home'
run I A '/home/documents'
run I A '/home/documents/notes.txt'
run I A '/home/documents/'
run I A '/home/./documents'
run I A '/home/subdir/../documents'
run I A '/home/a/b/../../documents'
run I A '/café/notes.txt'
run I A '/home/my documents/file.txt'
run I A '/home/documents/file.name.with.dots.txt'
run I A '/home/documents/-leading-dash.txt'
run I A '/home/.hidden'
run I A '/home/.hidden/inner.txt'
run I A '/внутри/файл.txt'
run I A '/日本語/ファイル.txt'
run I A '/link-to-inside'
run I A '/link-to-inside/file.txt'

echo
echo "accepts: $ACCEPTS   rejects: $REJECTS"
if [ "$FAILS" -gt 0 ]; then
    echo "hand-test: $FAILS line(s) FAILED"
    exit 1
fi
echo "hand-test: every line behaved as required"
exit 0
