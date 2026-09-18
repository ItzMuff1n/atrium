#!/usr/bin/env bash
# Atrium Phase 1b — hands-on attack pass (paths that do not exist yet).
#
# Runs every line of attack-list-1b.md sections A–M against the resolver,
# plus section N, the textual-mode escape found on 18 Sep 2026, and section Q,
# the out-and-back chain and ancestor-landing ruling of 18 Sep 2026 (neither is
# in attack-list-1b.md — that file is Phase 1b's frozen record and is not
# edited); both are described where they are defined, below,
# in a throwaway environment root built by the resolver's own `fixtures`
# mode (which now also creates the dangling-symlink and §M.3 fixtures).
# Same shape as hand-test.sh.
#
# Usage:  bash hand-test-1b.sh [root]
#         (default root: /tmp/atrium-handtest-1b)
#
# Read-only with respect to the project: it creates files under the throwaway
# root and, for section Q, one directory beside it (the environment the root
# sits in), and deletes both at the end.

set -u

ROOT="${1:-/tmp/atrium-handtest-1b}"

# Find the resolver binary in the WORKSPACE target directory, not "here".
#
# `cargo build` in a cargo workspace puts binaries in the workspace root's
# target/debug, whatever directory it is run from. The previous version used
# `./target/debug/atrium-resolver` relative to the current directory, so run
# from `crates/resolver/` it looked in `crates/resolver/target/debug/` — which
# is NOT where the build writes. That path only ever worked because a stale
# binary from an earlier session happened to be sitting there, and the
# `[ ! -x ]` guard then skipped rebuilding. On 18 Sep 2026 that stale binary
# still accepted the very escape section N tests for, so running the gate from
# the crate directory reported the escape as fixed when it was not.
#
# Resolve to the workspace root explicitly and refuse to guess.
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="$REPO_ROOT/target/debug/atrium-resolver"

if [ ! -x "$BIN" ]; then
    echo "building (no binary at $BIN)..." >&2
    ( cd "$REPO_ROOT" && cargo build >/dev/null 2>&1 ) \
        || { echo "cargo build failed" >&2; exit 2; }
fi

if [ ! -x "$BIN" ]; then
    echo "hand-test-1b: no resolver binary at $BIN after building." >&2
    echo "  The workspace build writes there; if it is missing, the build failed." >&2
    exit 2
fi
# Report which binary is under test. A hands-on pass is only worth as much as
# the artefact it ran, and this line makes a stale one visible in the output.
echo "binary under test: $BIN"
echo "                   ($(stat -c '%y' "$BIN" | cut -c1-19), sha256 $(sha256sum "$BIN" | cut -c1-16))"

rm -rf "$ROOT"
mkdir -p "$ROOT"
"$BIN" fixtures --root "$ROOT" >/dev/null || { echo "fixtures failed" >&2; exit 2; }

# attack-list-1b.md §J: create the 255-byte name as a fixture inside the
# throwaway root, so the section's accept is a real accept on an existing
# name, not a lucky one.
C255_NAME="$(printf 'b%.0s' $(seq 1 255))"
touch "$ROOT/$C255_NAME"

# §M.6 needs a second name exactly at the limit, so the `..` cancels the first.
A255_NAME="$(printf 'a%.0s' $(seq 1 255))"
# §M.7: 20 component steps, and 20 vs 21 parent steps to bracket the counter.
A20="$(printf '/a%.0s' $(seq 1 20))"
U20="$(printf '/..%.0s' $(seq 1 20))"
U21="$(printf '/..%.0s' $(seq 1 21))"

# §M.3 relies on link-rel-out, created by the resolver's own `fixtures` mode.
# Fail loudly if it is absent rather than creating it here: without a relative
# outside-pointing link, §M.3's paths are ordinary absent paths and would ACCEPT,
# so quietly substituting a fixture would hide the very regression it tests for.
if [ ! -L "$ROOT/link-rel-out" ]; then
    echo "hand-test-1b: link-rel-out fixture missing — the binary is older than" >&2
    echo "this script. Run cargo build and try again." >&2
    exit 2
fi

# The root as the filesystem sees it, for the containment check above.
CANON_ROOT="$(realpath -m -- "$ROOT" 2>/dev/null || echo "$ROOT")"

# Section Q fixtures. A link whose target is OUTSIDE the root is followed
# through the environment that holds this test's root: the root's parent, a
# sibling directory outside it, and a relay out there that points back in.
#
# Built here rather than in the resolver's `fixtures` mode because they are
# shaped for THIS root's position on the host (the parent of the root), whereas
# the shared fixtures are host paths chosen to be wrong everywhere.
#
#   <root>/q-out      -> <parent>/q-outside          (a hop out)
#   <root>/q-relay    -> <root>                      (a relay link outside, back in)
#   <root>/q-back     -> <parent>/q-outside/q-relay   (out, then back in)
#   <root>/q-mid      -> <root>/q-mid-2               (inside)
#   <root>/q-mid-2    -> <parent>/q-outside/q-relay   (then out and back)
#   <root>/q-a        -> <parent>                     (lands on an ANCESTOR of the root)
#
# Section Q is not in attack-list-1b.md: that file is Phase 1b's frozen record
# and is not edited. The ruling it tests is in docs/DECISIONS.md (ruling 5,
# 18 Sep 2026).
OUTSIDE_DIR="$(dirname "$CANON_ROOT")/q-outside"
rm -rf "$OUTSIDE_DIR"
mkdir -p "$OUTSIDE_DIR"
ln -s "$CANON_ROOT" "$OUTSIDE_DIR/q-relay"
ln -s "$OUTSIDE_DIR" "$ROOT/q-out"
ln -s "$OUTSIDE_DIR/q-relay" "$ROOT/q-back"
ln -s "$ROOT/q-mid-2" "$ROOT/q-mid"
ln -s "$OUTSIDE_DIR/q-relay" "$ROOT/q-mid-2"
ln -s "$(dirname "$CANON_ROOT")" "$ROOT/q-a"


echo "environment root: $ROOT"
echo

FAILS=0
ACCEPTS=0
REJECTS=0

# Expected totals, so a silently added, removed or reclassified case is caught
# rather than read as a pass. A gate line that stops being run is the failure
# this whole file exists to prevent.
# (53 and 74 read from an actual run of this script on 18 Sep 2026, after
# section Q was added; 113 lines before it, 127 after.)
EXPECTED_ACCEPTS=53
EXPECTED_REJECTS=74

# run <section> <expected: R|A> <path>
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
    #
    # This compares REAL paths, not spelled ones. A string prefix test says a
    # path is contained when it BEGINS with the root's spelling, which is what
    # let the 18 Sep 2026 textual-mode escape through unnoticed: the resolver
    # returned `<root>/NAME`, whose spelling is under the root and whose real
    # location is outside it. `realpath` follows the symlinks, so the check
    # cannot be fooled by a name that merely looks contained.
    if [ "$verdict" = "ACCEPT" ]; then
        ACCEPTS=$((ACCEPTS+1))
        local real
        real="$(realpath -m -- "$detail" 2>/dev/null || echo "$detail")"
        case "$real" in
            "$CANON_ROOT"|"$CANON_ROOT"/*) : ;;
            *) mark="FAIL"; FAILS=$((FAILS+1))
               echo "  !! ESCAPE FROM ROOT: $detail (really $real)" ;;
        esac
    else
        REJECTS=$((REJECTS+1))
        # For 1b reject sections the reason must name the ESCAPE (a `..`
        # step or a symlink), not merely absence (attack-list-1b.md, "Why
        # the reason matters").
        case "$detail" in
            *"does not exist inside the environment root"*)
                mark="FAIL"; FAILS=$((FAILS+1)); echo "  !! REJECTED CITING ABSENCE: $detail" ;;
        esac
    fi
    if [ "$rc" != "0" ] && [ "$rc" != "1" ]; then
        mark="FAIL"; FAILS=$((FAILS+1))
    fi

    printf '[%s] [%-1s %-6s] %-46s %s\n' "$mark" "$section" "$verdict" "$path" "$detail"
}

echo "=== A. traversal in the existing part, absent tail (expect REJECT) ==="
run A R '/../newfile'
run A R '/../../newfile'
run A R '/../../..'
run A R '/home/../../newfile'
run A R '/home/documents/../../../etc/newfile'
run A R '/a/./../../newfile'

echo
echo "=== B. .. inside the part that does not exist (expect REJECT) ==="
run B R '/newdir/../../newfile'
run B R '/newdir/../..'
run B R '/newfile/../..'
run B R '/home/newdir/../../../newfile'
run B R '/a/b/newdir/../../../../newfile'
run B R '/home/documents/newdir/../../../../etc/newfile'

echo
echo "=== C. absent tail on an existing symlink pointing outside (expect REJECT) ==="
run C R '/link-to-etc/newfile'
run C R '/link-to-home/newfile'
run C R '/link-to-root/newfile'
run C R '/link-to-parent/newfile'
run C R '/chain-a/newfile'
run C R '/link-to-etc/sub/newfile'
run C R '/good-dir/inner-link/newfile'
run C R '/link-to-etc/../home/newfile'

echo
echo "=== D. escapes appearing only after the absent tail (expect REJECT) ==="
run D R '/link-to-inside/../../../newfile'
run D R '/link-to-inside/newdir/../../../../newfile'
run D R '/good-dir/../../../newfile'
run D R '/link-to-root/../etc/newfile'

echo
echo "=== E. out, back, then absent (expect REJECT) ==="
run E R '/link-to-etc/../../home/documents/newfile'
run E R '/home/../link-to-root/home/newfile'
run E R '//../newfile'

echo
echo "=== F. absent parent, escaping remainder (expect REJECT) ==="
run F R '/newdir/../../etc/newfile'
run F R '/home/newdir/../../../root/newfile'

echo
echo "=== G. separator / normalisation tricks, absent tail (expect REJECT) ==="
run G R '/home//..//..//newfile'
run G R '/home/documents//../..//../newfile'
run G R '/home/documents/newdir//../../../..//newfile'

echo
echo "=== H. nasty combinations (expect REJECT) ==="
run H R '/link-to-etc/../../newfile'
run H R '/home/../link-to-root/newfile'
run H R '/link-to-parent/newdir/newfile'
run H R '/..//newfile'
run H R '/home/documents/../../../newfile'
run H R '/link-to-etc/./../newfile'

echo
echo "=== I. must be ACCEPTED (absent, inside the root) ==="
run I A '/newfile'
run I A '/newdir/newfile'
run I A '/newdir/'
run I A '/new file.txt'
run I A '/newdir/.hidden'
run I A '/newdir/file.name.with.dots.txt'
run I A '/newdir/-leading-dash.txt'
run I A '/home/newfile'
run I A '/home/documents/newfile'
run I A '/home/documents/newdir/newfile'
run I A '/home/documents/./newfile'
run I A '/home//newfile'
run I A '//newfile'
run I A '/home/documents/../newfile'
run I A '/home/newdir/../newfile'
run I A '/home/newdir/../../newfile'
run I A '/newdir/../newfile'
run I A '/home/documents/newdir/../../newfile'
run I A '/link-to-inside/newfile'
run I A '/link-to-inside/newdir/newfile'
run I A '/новый/файл.txt'
run I A '/新しい/ファイル.txt'
run I A '/newdir/новый/файл.txt'

echo
echo "=== J. component length (one accept, three rejects) ==="
C256="/$(printf 'c%.0s' $(seq 1 256))"
HEBREW="$(printf '/\\u05d0%.0s' $(seq 1 200) 2>/dev/null || true)"
# printf's \u is not portable; build the Hebrew path with the resolver's
# own byte truth: א is 2 bytes.
HEBREW="/$(python3 -c 'print("א"*200)' 2>/dev/null)"
DEEP="/a"
for _ in $(seq 1 199); do DEEP="$DEEP/a"; done
run J A "/$C255_NAME"
run J R "$C256"
run J R "$HEBREW"
run J A "$DEEP"

echo
echo "=== K. names that read as escapes and are not (expect ACCEPT) ==="
run K A '/etc/passwd'
run K A '/etc/newdir/newfile'
run K A '/root/newfile'
run K A '/proc/self/newfile'
run K A '/sys/class/newfile'
run K A '/dev/newfile'

echo
echo "=== K.1 rulings ==="
run K A '/link-to-nothing'
run K R '/link-to-nothing-out'
run K A '/home\..\..\newfile'
run K R '/home/documents/notes.txt/..'
run K R '/home/documents/notes.txt/newfile'
echo "  -- K.1b deviation that coexists: a trailing slash on a file ACCEPTs:"
run K A '/home/documents/notes.txt/'

echo
echo "=== M. blind-list lines folded in 13 Sep 2026 (attack-list-1b.md §M) ==="
echo "  -- M.1 dot-runs that are not traversal (expect ACCEPT)"
run M A '/.../newfile'
run M A '/..../newfile'
run M A '/.. /newfile'
echo "  -- M.2 percent-encoded traversal (expect ACCEPT — nothing decodes it)"
run M A '/%2e%2e/newfile'
run M A '/%2E%2E%2Fhost/newfile'
run M A '/..%00/newfile'
echo "  -- M.3 relative symlink target climbing out (expect REJECT; fixture link-rel-out)"
run M R '/link-rel-out/newfile'
run M R '/link-rel-out/../newfile'
echo "  -- M.4 chain plus traversal after the hop (expect REJECT)"
run M R '/chain-a/newdir/../../newfile'
run M R '/chain-a/../../newfile'
echo "  -- M.5 degenerate tails on a dangling symlink (one accept, one reject — ruling 4)"
run M A '/link-to-nothing/..'
run M R '/link-to-nothing-out/..'
echo "  -- M.6 length crossed with normalisation (expect ACCEPT)"
run M A "/$A255_NAME/../$C255_NAME"
echo "  -- M.7 depth accounting at 20 levels (balanced accept, off-by-one reject)"
run M A "$A20/$U20/newfile"
run M R "$A20/$U21/newfile"

echo
echo "=== N. the textual-mode escape, found 18 Sep 2026 (expect REJECT) ==="
echo "  -- An absent component, then \`..\`, then a symlink pointing outside"
echo "     the root. Before the fix every line below was ACCEPTED, and the"
echo "     location it returned canonicalised outside the root. The absent"
echo "     component must not switch checking off for what follows it."
run N R '/nope/../link-to-etc'
run N R '/nope/../link-to-home'
run N R '/nope/../link-to-root'
run N R '/_/..//link-to-etc'
run N R '/_///../link-to-etc'
run N R '//_//../link-to-etc'
echo "  -- the same shape with the escaping link in the MIDDLE of the path"
run N R '/nope/../link-to-etc/newfile'
run N R '/nope/../link-to-etc/passwd'
run N R '/_/..//good-dir/inner-link/newfile'
echo "  -- and through a chain that ends outside"
run N R '/nope/../chain-a'
run N R '/nope/../chain-a/newfile'
echo "  -- a dangling outside-pointing link reached the same way (ruling 1 vs"
echo "     the step rule: the target decides, and this target is outside)"
run N R '/nope/../link-to-nothing-out'
run N R '/nope/../link-rel-out'
echo "  -- deeper absent runs before the same link"
run N R '/a/b/c/../../../link-to-etc'
run N R '/nope/deeper/../../../link-to-etc/newfile'
echo "  -- controls: the same shapes pointing INSIDE must still be ACCEPTED,"
echo "     and so must ordinary absent paths, or the fix has gone too far"
run N A '/nope/../newfile'
run N A '/nope/..'
run N A '/_/..//newfile'
run N A '/nope/../home/documents'
run N A '/nope/../link-to-inside/newfile'
run N A '/link-to-nothing/..'

echo
echo "=== Q. chains that leave the root and come back (expect REJECT) ==="
echo "  -- Ruling 5, 18 Sep 2026: a chain that leaves the root at ANY hop is"
echo "     refused, even when it ends inside the root. Before the ruling every"
echo "     REJECT line below was ACCEPTED: the resolver followed the whole chain"
echo "     with canonicalize and checked only where it landed."
echo "  -- out and back: <root>/q-back -> <parent>/q-outside/q-relay -> <root>"
run Q R '/q-back'
run Q R '/q-back/notes.txt'
run Q R '/q-back/home/documents/notes.txt'
echo "  -- the same chain reached under an absent prefix, so the \`..\` cannot"
echo "     hide it"
run Q R '/nope/../q-back'
run Q R '/nope/../q-back/notes.txt'
echo "  -- the hop out sits in the MIDDLE of a longer chain"
run Q R '/q-mid'
run Q R '/q-mid/notes.txt'
run Q R '/q-mid/home/documents/notes.txt'
echo "  -- a link that LANDS on an ancestor of the root (the <root>/a -> /home"
echo "     shape). The spelling can walk straight back into the root and the"
echo "     final location is genuinely inside; leaving is what decides."
run Q R '/q-a'
run Q R '/q-a/notes.txt'
echo "  -- controls: the same shapes staying inside must still be ACCEPTED, and"
echo "     so must the ordinary path, or the ruling has gone too far"
run Q A '/home/documents/notes.txt'
run Q A '/nope/../home/documents/notes.txt'
run Q A '/nope/../link-to-inside/notes.txt'
run Q A '/link-to-inside/notes.txt'

echo
echo "accepts: $ACCEPTS   rejects: $REJECTS"
if [ "$ACCEPTS" != "$EXPECTED_ACCEPTS" ] || [ "$REJECTS" != "$EXPECTED_REJECTS" ]; then
    echo "hand-test-1b: line totals changed -- expected $EXPECTED_ACCEPTS accepts" >&2
    echo "  and $EXPECTED_REJECTS rejects, saw $ACCEPTS and $REJECTS. A case was" >&2
    echo "  added, removed or reclassified. Update EXPECTED_* deliberately." >&2
    FAILS=$((FAILS+1))
fi
rm -rf "$OUTSIDE_DIR"
rm -rf "$ROOT"
if [ "$FAILS" -gt 0 ]; then
    echo "hand-test-1b: $FAILS line(s) FAILED"
    exit 1
fi
echo "hand-test-1b: every line behaved as required"
exit 0
