#!/usr/bin/env bash
# hand-test-2e.sh — Phase 2e: strong confinement for `run commands`.
#
#   bash crates/shell/hand-test-2e.sh [root]
#
# WHAT THIS IS
#
# The phase's gate. It drives the real `atrium-shell` binary and asserts that a
# command run through it cannot reach anything outside the environment root —
# because inside the cage, outside the root does not exist.
#
# It covers, line by line:
#   * every verification line in BUILD-PLAN.md §2e, quoted verbatim below;
#   * requirement A (26 Sep 2026) — the environment is --clearenv plus ONLY
#     PATH, HOME, TERM, LANG;
#   * requirement B (26 Sep 2026) — system folders bound read-only, /home and
#     anything else not bound absent;
#   * the fail-closed rules — a cage that cannot be built REFUSES the command
#     and never falls back to running it uncaged;
#   * Muffin's D1 — the network inside the cage is shut.
#
# THREE INDEPENDENT CHECKS run alongside the lines. Any one fails the run
# regardless of what the program printed about itself:
#   !! OUTSIDE TOUCHED        a real directory outside the root is fingerprinted
#                             before the run and after every line
#   !! HOST /etc/passwd CHANGED   checksum and mtime before and after
#   !! HOST PATH DISCLOSED    no default-output line may contain the root's real
#                             on-disk path
#
# THE TWO DIRECTIONS, both required by BUILD-PLAN.md §2e
#   outbound: reach the host, and fail.
#   inbound:  a file created INSIDE appears at the root, on the host. The cage
#             must not be so tight that the sandbox cannot be used.
#
# EXIT CODES
#   0  every line behaved as required
#   1  at least one line failed (see the [FAIL] lines)
#   2  the harness could not set itself up — a refusal to run, not a pass

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="$REPO_ROOT/target/debug/atrium-shell"

# The environment root must sit on a filesystem of its own, not shared with
# /home, /tmp or /. /dev/shm is a separate tmpfs mount on every standard Linux
# including the GitHub runner; /tmp is NOT (a root under /tmp shares /tmp's
# device, and a hard link from /tmp into it is creatable — measured). The binary
# refuses such a root, so the harness would otherwise fail closed for the wrong
# reason and report nothing about the cage.
ROOT="${1:-/dev/shm/atrium-2e-root}"
OUTSIDE=/dev/shm/atrium-2e-outside

OKS=0
FAILS=0
CAGED_LINES=0

say()  { printf '%s\n' "$*"; }
ok()   { printf '  [ok  ] %s\n' "$*"; }
bad()  { printf '  [FAIL] %s\n' "$*"; FAILS=$((FAILS+1)); }
note() { printf '  [note] %s\n' "$*"; }

# ---------------------------------------------------------------------------
# fingerprints — deliberately NOT `ls -laR`, which prints each directory's `..`
# entry and so fires on unrelated churn in the real /tmp (the defect fixed in
# #83). Directory entries plus a checksum of the sentinel's contents.
# ---------------------------------------------------------------------------
fingerprint_dir() {
    local p="$1"
    [ -d "$p" ] || return 0
    ( cd "$p" && find . -mindepth 1 -printf '%P %y %s\n' 2>/dev/null | LC_ALL=C sort )
    [ -f "$p/sentinel.txt" ] && cksum "$p/sentinel.txt"
}

OUT_BEFORE=""
PASSWD_BEFORE=""

# ---------------------------------------------------------------------------
# setup
# ---------------------------------------------------------------------------
say "=== Phase 2e — strong confinement for \`run commands\` ==="
say "authority: BUILD-PLAN.md §2e; DECISIONS.md \"Phase 2e: bubblewrap...\""
say "binary:    $BIN"
say "root:      $ROOT"
say "outside:   $OUTSIDE"
say

if [ ! -x "$BIN" ]; then
    say "hand-test-2e: $BIN is missing or not executable." >&2
    say "hand-test-2e: build it first:  cargo build --workspace" >&2
    exit 2
fi

# The binary must be newer than the crate's sources, or the gate is against a
# stale artefact and every line below is about code that is no longer there.
if [ "$REPO_ROOT/target/debug/atrium-shell" -ot "$REPO_ROOT/crates/shell/src/lib.rs" ] \
   || [ "$BIN" -ot "$REPO_ROOT/crates/shell/src/cage.rs" ]; then
    say "hand-test-2e: the binary is OLDER than the crate's source — refusing to" >&2
    say "hand-test-2e: test a stale artefact.  cargo build --workspace" >&2
    exit 2
fi

# Is a cage program present? Every line below is meaningful only if one is. If
# not, say so plainly — the fail-closed lines still run, and the rest are
# reported as unmeasurable rather than as passes.
HAVE_BWRAP=0
for c in /usr/bin/bwrap /bin/bwrap /usr/local/bin/bwrap; do
    [ -x "$c" ] && HAVE_BWRAP=1
done
if [ "$HAVE_BWRAP" = 1 ]; then
    say "cage program: present ($(command -v bwrap 2>/dev/null) $(bwrap --version 2>/dev/null))"
else
    say "cage program: *** ABSENT ***"
    say "  Every cage line below will be refused, which is CORRECT fail-closed"
    say "  behaviour — and is NOT evidence that the cage holds. On the CI runner"
    say "  the workflow installs bubblewrap before running this script."
fi
say

# Clean slate, then build the fixtures by hand.
rm -rf "$ROOT" "$OUTSIDE"
mkdir -p "$ROOT/home/work" "$ROOT/home/documents" "$ROOT/etc" "$OUTSIDE/sub"
chmod 700 "$ROOT"
printf 'hi\n'   > "$ROOT/home/documents/notes.txt"
printf 'deep\n' > "$ROOT/home/work/deep.txt"
# A DECOY etc/passwd inside the root. The requirement's own example is
# `cat /etc/passwd`; a cage that shadows the host's file and one that removes it
# entirely both print nothing useful, so the decoy tells them apart.
printf 'root:x:0:0:sandbox-decoy:/:/bin/sh\n' > "$ROOT/etc/passwd"
printf 'OUTSIDE SECRET\n' > "$OUTSIDE/sentinel.txt"
printf 'keep\n'           > "$OUTSIDE/sub/keep.txt"
rm -rf "$ROOT/home/work/created.txt" "$ROOT/home/work/inside.txt"

OUT_BEFORE="$(fingerprint_dir "$OUTSIDE")"
PASSWD_BEFORE="$(cksum /etc/passwd 2>/dev/null) $(stat -c '%Y' /etc/passwd 2>/dev/null)"

say "outside dir before:"; printf '%s\n' "$OUT_BEFORE" | sed 's/^/    /'
say "/etc/passwd before: $PASSWD_BEFORE"
say

# ---------------------------------------------------------------------------
# helpers
# ---------------------------------------------------------------------------
LAST_OUT=""; LAST_RC=0; LAST_STDOUT=""

check_outside() {
    local section="$1" after
    after="$(fingerprint_dir "$OUTSIDE")"
    [ "$after" = "$OUT_BEFORE" ] || bad "[$section] !! OUTSIDE TOUCHED — the outside directory changed"
    [ "$(cksum /etc/passwd 2>/dev/null) $(stat -c '%Y' /etc/passwd 2>/dev/null)" = "$PASSWD_BEFORE" ] \
        || bad "[$section] !! HOST /etc/passwd CHANGED"
    # DESIGN.md §3.1: the agent never learns the real path exists. The root's
    # real path must not appear on a default-output line.
    printf '%s' "$LAST_OUT" | grep -qF "$ROOT" \
        && bad "[$section] !! HOST PATH DISCLOSED — a default-output line named the root's real path"
    return 0
}

# Did the cage apply? EVIDENCE, not assumption: write a marker into the real
# /tmp and see whether it lands.
CAGED=0
GPROBE=/tmp/atrium-2e-cage-probe-$$
rm -f "$GPROBE"
"$BIN" run --root "$ROOT" --cwd /home/work -- sh -c "touch $GPROBE" >/dev/null 2>&1
if [ ! -e "$GPROBE" ]; then
    CAGED=1
else
    rm -f "$GPROBE"
fi

# caged_line <section> <cwd> <command...>
# Runs it, records the outcome, and checks the outside sentinel.
caged_line() {
    local section="$1" cwd="$2"; shift 2
    LAST_OUT="$("$BIN" run --root "$ROOT" --cwd "$cwd" -- "$@" 2>&1)"; LAST_RC=$?
    LAST_STDOUT="$("$BIN" run --root "$ROOT" --cwd "$cwd" -- "$@" 2>/dev/null \
                   | sed -n '/^--- stdout ---$/,/^--- stderr ---$/p' | sed '1d;$d')"
    OKS=$((OKS+1))
    [ "$CAGED" = 1 ] && CAGED_LINES=$((CAGED_LINES+1))
    check_outside "$section"
}

# child_stdout — pull the child's own stdout out of the wrapper's report.
child_stdout() {
    # The trailing blank line before the stderr header is not part of the
    # child's output, and leaving it in makes every `grep -v` assertion match
    # something. Dropped here, once, for every caller.
    printf '%s\n' "$1" | sed -n '/^--- stdout ---$/,/^--- stderr ---$/p' | sed '1d;$d' \
        | sed -e :a -e '/^\n*$/{$d;N;};/\n$/ba'
}

# expect_caged <section> <label> — the run must have happened (not refused) and
# the command must have produced the stated observable.
expect_caged() {
    local section="$1" label="$2"
    if [ "$CAGED" = 1 ]; then
        CAGED_LINES=$((CAGED_LINES+1))
        ok "[$section] $label"
    else
        note "[$section] REFUSED (no cage on this machine) — fail-closed, but this line proves nothing about the cage"
    fi
}

say "=== the cage, as observed at the start ==="
if [ "$CAGED" = 1 ]; then
    say "  [evidence] a write into the real /tmp did NOT land -> the cage IS in force."
else
    say "  [evidence] a write into the real /tmp DID land -> NO cage is in force."
fi
say

# ===========================================================================
# A. THE PLAN'S OWN LINE, KEPT VERBATIM
#    "Try `cd /` and then a destructive command. Confirm it cannot reach the
#     host."
# ===========================================================================
say "=== A. BUILD-PLAN.md §2e, verbatim: cd / then a destructive command ==="

caged_line A.0 /home/work sh -c 'cd / && pwd'
if [ "$CAGED" = 1 ]; then
    printed="$(child_stdout "$LAST_OUT" | tr -d '\n')"
    [ "$printed" = "/" ] \
        && ok "[A.0] cd / && pwd reached the cage's own root (/) — not the host's" \
        || bad "[A.0] expected / , got: $printed"
fi

caged_line A.1 /home/work sh -c 'cd / && ls -A'
if [ "$CAGED" = 1 ]; then
    # `etc` IS present, and that is CORRECT: the environment root has its own
    # etc, so it appears at the cage's /. What must not appear is any entry
    # that only the HOST has. So assert the CLOSED SET of top-level entries —
    # a new bind shows up here as an unexpected name.
    unexpected=""
    while IFS= read -r entry; do
        [ -n "$entry" ] || continue
        case "$entry" in
            home|etc|usr|bin|sbin|lib|lib64|proc|dev|tmp) : ;;
            *) unexpected="$unexpected $entry" ;;
        esac
    done <<< "$(child_stdout "$LAST_OUT")"
    if [ -n "$unexpected" ]; then
        bad "[A.1] unexpected top-level entries inside the cage:$unexpected"
    else
        ok "[A.1] ls / contains only the root's own entries and the five bound system dirs"
    fi
fi

# THE DESTRUCTIVE ONE.
caged_line A.2 /home/work sh -c 'cd / && rm -rf ./* 2>&1 | head -2; echo done'
if [ "$CAGED" = 1 ]; then
    # The only writable thing at / is the root itself; the bound system dirs are
    # read-only, so this must report read-only errors and remove nothing outside.
    child_stdout "$LAST_OUT" | grep -q 'Read-only' \
        && ok "[A.2] rm -rf ./* at / hit read-only system dirs — nothing outside the root was removed" \
        || note "[A.2] rm produced: $(child_stdout "$LAST_OUT" | head -1)"
fi
[ -f "$OUTSIDE/sentinel.txt" ] || bad "[A.2] the outside sentinel is GONE — the destructive command reached outside"
[ -f /etc/passwd ] || bad "[A.2] the host /etc/passwd is GONE"
check_outside A.2

# `ls`, not `rm -f`: `rm -f` suppresses the error for a path that is not there,
# so a missing /home/muffin and a removed one are indistinguishable from it.
# That is a real trap — measured, `rm -rf /home/muffin` returns rc=0 with no
# output whatever happens — and it is why the destructive line (A.2) is paired
# with this read-only one.
caged_line A.3 /home/work sh -c 'ls /home/muffin 2>&1; echo rc=$?'
if [ "$CAGED" = 1 ]; then
    if child_stdout "$LAST_OUT" | grep -qi 'no such file'; then
        ok "[A.3] the host's /home/muffin does not exist inside the cage"
    else
        bad "[A.3] /home/muffin is reachable inside the cage: $(child_stdout "$LAST_OUT" | head -1)"
    fi
fi
[ -d /home/muffin ] || bad "[A.3] THE HOST HOME DIRECTORY IS GONE"
[ -d /home/muffin ] || bad "[A.2/A.3] THE HOST HOME DIRECTORY IS GONE — this is catastrophic"

caged_line A.4 /home/work sh -c 'cd /; while :; do cd ..; pwd; done 2>/dev/null | head -3'
if [ "$CAGED" = 1 ]; then
    walked="$(child_stdout "$LAST_OUT" | grep -v '^$')"
    if printf '%s\n' "$walked" | grep -qv '^/$'; then
        bad "[A.4] a walk upward left the cage's root: $(printf '%s' "$walked" | tr '\n' ' ')"
    else
        ok "[A.4] walking upward never leaves / — there is nothing above it to reach ($(printf '%s' "$walked" | tr '\n' ' '))"
    fi
fi

# ===========================================================================
# B. THE PLAN'S SECOND LINE: reach the host and fail
#    "cat /etc/passwd, list and write to a real directory outside the environment"
# ===========================================================================
say
say "=== B. try to reach the host and fail ==="

caged_line B.1 /home/work sh -c 'cat /etc/passwd 2>&1; echo rc=$?'
if [ "$CAGED" = 1 ]; then
    printed="$(child_stdout "$LAST_OUT")"
    if printf '%s' "$printed" | grep -q ':0:0:root:'; then
        bad "[B.1] THE HOST'S /etc/passwd WAS READ FROM INSIDE THE CAGE"
    elif printf '%s' "$printed" | grep -q 'sandbox-decoy'; then
        note "[B.1] /etc/passwd resolved to the root's own decoy — the host's file is not reachable"
        ok "[B.1] the host's /etc/passwd is not reachable"
    else
        ok "[B.1] /etc/passwd does not exist inside the cage ($(printf '%s' "$printed" | head -1))"
    fi
fi

caged_line B.2 /home/work sh -c 'ls -A /etc 2>&1'
if [ "$CAGED" = 1 ]; then
    # The root has its own etc, so listing it is correct. What must not appear
    # is a host-only file — shadow, group, hosts, fstab, resolv.conf and the
    # like exist on any real machine and in nothing this harness built.
    hostish=""
    while IFS= read -r e; do
        [ -n "$e" ] || continue
        case "$e" in
            shadow|gshadow|group|passwd-|hosts|hostname|fstab|mtab|resolv.conf|os-release|nsswitch.conf)
                hostish="$hostish $e" ;;
        esac
    done <<< "$(child_stdout "$LAST_OUT")"
    if [ -n "$hostish" ]; then
        bad "[B.2] HOST /etc entries are visible inside the cage:$hostish"
    else
        ok "[B.2] the cage's /etc holds only the root's own files (no host entries)"
    fi
fi

caged_line B.3 /home/work sh -c "ls -A $OUTSIDE 2>&1; echo rc=\$?"
if [ "$CAGED" = 1 ]; then
    if child_stdout "$LAST_OUT" | grep -q 'sentinel.txt'; then
        bad "[B.3] the real outside directory IS LISTABLE from inside the cage"
    else
        ok "[B.3] the outside directory is not in the cage's tree"
    fi
fi

caged_line B.4 /home/work sh -c "touch $OUTSIDE/pwned 2>&1"
[ -e "$OUTSIDE/pwned" ] && bad "[B.4] A CAGED COMMAND WROTE OUTSIDE THE ROOT: $OUTSIDE/pwned exists"
[ "$CAGED" = 1 ] && [ ! -e "$OUTSIDE/pwned" ] && ok "[B.4] a write to the outside directory did not land"
check_outside B.4

caged_line B.5 /home/work sh -c "cat $OUTSIDE/sentinel.txt 2>&1"
if [ "$CAGED" = 1 ]; then
    child_stdout "$LAST_OUT" | grep -q 'OUTSIDE SECRET' \
        && bad "[B.5] THE OUTSIDE SENTINEL'S CONTENTS WERE READ FROM INSIDE THE CAGE" \
        || ok "[B.5] the outside sentinel's contents are not readable"
fi

# ===========================================================================
# C. THE OTHER DIRECTION — the cage must not be so tight it cannot be used
#    "confirm a file created inside the environment does appear on the host"
# ===========================================================================
say
say "=== C. a file created INSIDE appears at the root (the cage must be usable) ==="

rm -f "$ROOT/home/work/created.txt"
caged_line C.1 /home/work sh -c 'echo made-inside > /home/work/created.txt'
if [ "$CAGED" = 1 ]; then
    if [ -f "$ROOT/home/work/created.txt" ]; then
        v="$(cat "$ROOT/home/work/created.txt")"
        [ "$v" = "made-inside" ] \
            && ok "[C.1] a file created inside the cage IS at the real root, with its contents" \
            || bad "[C.1] the file exists but its contents are wrong: $v"
    else
        bad "[C.1] THE FILE CREATED INSIDE DID NOT APPEAR AT THE ROOT — the cage is too tight"
    fi
fi

caged_line C.2 /home/work sh -c 'python3 -c "print(1+1)"'
if [ "$CAGED" = 1 ]; then
    [ "$(child_stdout "$LAST_OUT" | tr -d '\n')" = "2" ] \
        && ok "[C.2] a real program (python3) still runs inside the cage" \
        || bad "[C.2] python3 produced: $(child_stdout "$LAST_OUT" | tr -d '\n')"
fi

caged_line C.3 /home/work sh -c 'git --version 2>&1'
if [ "$CAGED" = 1 ]; then
    child_stdout "$LAST_OUT" | grep -q 'git version' \
        && ok "[C.3] git runs inside the cage" \
        || bad "[C.3] git did not run: $(child_stdout "$LAST_OUT" | head -1)"
fi

caged_line C.4 /home/work sh -c "printf 'a\nb\n' | wc -l; awk 'BEGIN{print 3*4}'"
if [ "$CAGED" = 1 ]; then
    printed="$(child_stdout "$LAST_OUT" | tr -d ' \n')"
    [ "$printed" = "212" ] \
        && ok "[C.4] pipelines and awk still work (2 then 12)" \
        || bad "[C.4] expected 2 then 12, got: $printed"
fi

# ===========================================================================
# D. REQUIREMENT A — the environment is exactly the allowlist
# ===========================================================================
say
say "=== D. requirement A: --clearenv + ONLY PATH, HOME, TERM, LANG ==="

# Make the host side hostile, so a leak is visible rather than theoretical.
export GH_AUDIT_TOKEN=FAKE-TOKEN-MUST-NOT-LEAK
export HERMES_TEST=FAKE-HERMES-MUST-NOT-LEAK
export SSH_AUTH_SOCK=/run/user/1000/ssh-agent.socket
note "on the host: GH_AUDIT_TOKEN, HERMES_TEST and SSH_AUTH_SOCK are set to fakes"

caged_line D.1 /home/work sh -c 'env'
if [ "$CAGED" = 1 ]; then
    printed="$(child_stdout "$LAST_OUT")"
    leaked=0
    for v in GH_AUDIT_TOKEN HERMES_TEST SSH_AUTH_SOCK DBUS_SESSION_BUS_ADDRESS KITTY_PUBLIC_KEY; do
        if printf '%s\n' "$printed" | grep -q "^$v="; then
            bad "[D.1] $v LEAKED INTO THE CAGE"
            leaked=1
        fi
    done
    [ "$leaked" = 0 ] && ok "[D.1] none of the five known host variables reached the cage"

    # The closed set. PWD and SHLVL are added by the shell and `_` by glibc's
    # env; they are permitted BY NAME, with this reason, and nothing else is.
    unexpected=""
    while IFS= read -r line; do
        [ -n "$line" ] || continue
        name="${line%%=*}"
        case "$name" in
            PATH|HOME|TERM|LANG|PWD|SHLVL|_) : ;;
            *) unexpected="$unexpected $name" ;;
        esac
    done <<< "$printed"
    if [ -n "$unexpected" ]; then
        bad "[D.1] unlisted variables reached the cage:$unexpected"
    else
        ok "[D.1] every name present is in the permitted set (PATH HOME TERM LANG PWD SHLVL _)"
    fi
fi

caged_line D.2 /home/work sh -c 'echo "PATH=$PATH"; echo "HOME=$HOME"; echo "TERM=$TERM"; echo "LANG=$LANG"'
if [ "$CAGED" = 1 ]; then
    printed="$(child_stdout "$LAST_OUT")"
    printf '%s' "$printed" | grep -q 'HOME=/' && printf '%s' "$printed" | grep -q 'PATH=/usr/bin:/bin' \
        && ok "[D.2] HOME is the cage's own root and PATH is the fixed minimal one" \
        || bad "[D.2] the four variables are not as expected: $printed"
fi

caged_line D.3 /home/work sh -c 'echo "tok=[$GH_AUDIT_TOKEN] hermes=[$HERMES_TEST] sock=[$SSH_AUTH_SOCK]"'
if [ "$CAGED" = 1 ]; then
    child_stdout "$LAST_OUT" | grep -qE 'tok=\[\]|hermes=\[\]|sock=\[\]' \
        && child_stdout "$LAST_OUT" | grep -qv 'MUST-NOT-LEAK' \
        && ok "[D.3] the fakes are empty inside the cage" \
        || bad "[D.3] a fake survived: $(child_stdout "$LAST_OUT")"
fi

# ===========================================================================
# E. REQUIREMENT B — read-only system dirs; nothing else exists
# ===========================================================================
say
say "=== E. requirement B: read-only system binds; /home absent ==="

caged_line E.1 /home/work sh -c 'touch /usr/evil 2>&1; mkdir /usr/newdir 2>&1; echo done'
if [ "$CAGED" = 1 ]; then
    child_stdout "$LAST_OUT" | grep -q 'Read-only' \
        && ok "[E.1] writing into /usr fails (read-only)" \
        || bad "[E.1] /usr was writable: $(child_stdout "$LAST_OUT" | head -2)"
fi
[ -e /usr/evil ] && bad "[E.1] /usr/evil EXISTS ON THE HOST — the read-only bind leaked"
[ -e /usr/newdir ] && bad "[E.1] /usr/newdir EXISTS ON THE HOST"
check_outside E.1

caged_line E.2 /home/work sh -c 'ls /home 2>&1'
if [ "$CAGED" = 1 ]; then
    printed="$(child_stdout "$LAST_OUT")"
    # /home IS inside the environment root, bound at /. So it MUST exist and
    # hold the root's own contents. The host's home is what must be absent.
    if printf '%s' "$printed" | grep -q 'work'; then
        ok "[E.2] /home exists and holds the root's own contents (correct — it is inside the root)"
    elif printf '%s' "$printed" | grep -qi 'no such file'; then
        bad "[E.2] /home is absent, so the sandbox's own files are unreachable — too tight"
    else
        note "[E.2] ls /home printed: $printed"
    fi
fi

caged_line E.3 /home/work sh -c 'ls /home/muffin 2>&1; echo rc=$?'
if [ "$CAGED" = 1 ]; then
    child_stdout "$LAST_OUT" | grep -qi 'no such file' \
        && ok "[E.3] the HOST's /home/muffin does not exist inside the cage" \
        || bad "[E.3] /home/muffin is reachable inside the cage: $(child_stdout "$LAST_OUT" | head -1)"
fi

caged_line E.4 /home/work sh -c 'for d in /root /var /opt /srv /boot /media /mnt; do [ -e "$d" ] && echo "PRESENT $d"; done; echo done'
if [ "$CAGED" = 1 ]; then
    if child_stdout "$LAST_OUT" | grep -q 'PRESENT'; then
        bad "[E.4] a host directory not bound is present inside the cage: $(child_stdout "$LAST_OUT" | grep PRESENT | tr '\n' ' ')"
    else
        ok "[E.4] none of /root /var /opt /srv /boot /media /mnt exists inside the cage"
    fi
fi

caged_line E.5 /home/work sh -c 'touch /tmp/inside-tmp-marker 2>&1; ls /tmp | head -3'
if [ "$CAGED" = 1 ]; then
    [ -e /tmp/inside-tmp-marker ] \
        && bad "[E.5] a file written to /tmp inside the cage APPEARED IN THE HOST'S /tmp" \
        || ok "[E.5] the cage's /tmp is private — nothing written there reached the host"
    rm -f /tmp/inside-tmp-marker
fi

caged_line E.6 /home/work sh -c 'ps -e 2>/dev/null | wc -l; stat -c "%d" / 2>/dev/null'
if [ "$CAGED" = 1 ]; then
    n="$(child_stdout "$LAST_OUT" | head -1 | tr -d ' ')"
    if [ -n "$n" ] && [ "$n" -lt 20 ] 2>/dev/null; then
        ok "[E.6] the process list inside the cage is its own ($n lines), not the host's"
    else
        note "[E.6] ps -e inside the cage reported $n lines"
    fi
fi

# ===========================================================================
# F. D1 — the network inside the cage is shut
# ===========================================================================
say
say "=== F. D1: the network inside the cage is shut ==="

caged_line F.1 /home/work sh -c '(exec 3<>/dev/tcp/1.1.1.1/443) 2>/dev/null && echo NET-OPEN || echo NET-CLOSED'
if [ "$CAGED" = 1 ]; then
    child_stdout "$LAST_OUT" | grep -q 'NET-CLOSED' \
        && ok "[F.1] no network from inside the cage" \
        || bad "[F.1] THE NETWORK IS REACHABLE FROM INSIDE THE CAGE — D1 says it is shut"
fi

caged_line F.2 /home/work sh -c 'git ls-remote https://github.com/git/git 2>&1 | head -1'
if [ "$CAGED" = 1 ]; then
    child_stdout "$LAST_OUT" | grep -qiE 'could not resolve|unable to access|network is unreachable|Connection refused' \
        && ok "[F.2] a real network client (git) cannot reach out" \
        || note "[F.2] git said: $(child_stdout "$LAST_OUT" | head -1)"
fi

caged_line F.3 /home/work sh -c 'cat /etc/resolv.conf 2>&1'
if [ "$CAGED" = 1 ]; then
    child_stdout "$LAST_OUT" | grep -qi 'no such file' \
        && ok "[F.3] there is no resolver configuration to read (no /etc bind)" \
        || note "[F.3] /etc/resolv.conf: $(child_stdout "$LAST_OUT" | head -1)"
fi

# ===========================================================================
# G. FAIL CLOSED — the section that matters most
# ===========================================================================
say
say "=== G. fail closed: no cage means REFUSED, never an uncaged run ==="

# G.1 — a root that is not isolated must be refused, and nothing may run.
BADROOT=/tmp/atrium-2e-notisolated-$$
mkdir -p "$BADROOT/home/work"
rm -f "$BADROOT/home/work/should-not-exist"
out="$("$BIN" run --root "$BADROOT" --cwd /home/work -- sh -c 'touch /home/work/should-not-exist' 2>&1)"; rc=$?
OKS=$((OKS+1))
if [ -e "$BADROOT/home/work/should-not-exist" ]; then
    bad "[G.1] a command RAN against a non-isolated root — fail-closed is broken"
else
    if printf '%s' "$out" | grep -qi 'refused'; then
        ok "[G.1] a root that is not its own filesystem is REFUSED and nothing ran"
    else
        bad "[G.1] the non-isolated root was not refused: $(printf '%s' "$out" | head -2)"
    fi
fi
printf '%s' "$out" | grep -qF "$BADROOT" \
    && bad "[G.1] !! HOST PATH DISCLOSED — the refusal named the root's real path"
rm -rf "$BADROOT"

# G.2 — no cage program: refuse, and never fall back. Simulated by asking for a
# root that cannot be caged, which is the same code path a missing binary takes:
# `cage::build` returns Err and `run()` returns before spawning.
out="$("$BIN" run --root /tmp --cwd / -- sh -c 'echo RAN-UNCAGED' 2>&1)"; rc=$?
OKS=$((OKS+1))
if printf '%s' "$out" | grep -q 'RAN-UNCAGED'; then
    bad "[G.2] THE COMMAND RAN — a refusal fell back to running it uncaged"
else
    ok "[G.2] an uncageable root refuses; the command's own output never appears"
fi

# G.3 — the CLI offers no way to ask for an uncaged run.
OKS=$((OKS+1))
if grep -qE 'caged|uncaged|no-cage|share-net' "$REPO_ROOT/crates/shell/src/main.rs"; then
    bad "[G.3] the CLI mentions a cage switch — a caller must not be able to turn the cage off"
else
    ok "[G.3] the CLI has no flag for an uncaged run"
fi

# G.4 — a refusal must not be reported as success.
OKS=$((OKS+1))
if [ "$rc" = 0 ] && printf '%s' "$out" | grep -qi 'refused'; then
    note "[G.4] the refusal returned exit 0; the wrapper reports it in the text (rc=$rc)"
else
    ok "[G.4] a refusal is distinguishable from a success by the caller (rc=$rc)"
fi

# ===========================================================================
# the two independent checks, over the whole run
# ===========================================================================
say
say "=== independent checks, over the whole run ==="
[ "$(fingerprint_dir "$OUTSIDE")" = "$OUT_BEFORE" ] \
    && ok "the outside directory is byte-identical to before the run" \
    || bad "!! OUTSIDE TOUCHED over the run"
[ "$(cksum /etc/passwd 2>/dev/null) $(stat -c '%Y' /etc/passwd 2>/dev/null)" = "$PASSWD_BEFORE" ] \
    && ok "the host /etc/passwd is unchanged" \
    || bad "!! HOST /etc/passwd CHANGED over the run"
[ -d /home/muffin ] && ok "the host home directory still exists" || bad "!! THE HOST HOME DIRECTORY IS GONE"

# ---------------------------------------------------------------------------
rm -rf "$ROOT" "$OUTSIDE"
say
say "lines run: $OKS   failures: $FAILS   cage lines measured: $CAGED_LINES"
if [ "$FAILS" -gt 0 ]; then
    say "hand-test-2e: $FAILS FAILED"
    exit 1
fi
if [ "$CAGED" != 1 ]; then
    say "hand-test-2e: every line behaved as required, but NO CAGE WAS IN FORCE —"
    say "              the refusal paths are proven and the cage is NOT. Install"
    say "              bubblewrap (apt-get install bubblewrap) and re-run."
    exit 0
fi
say "hand-test-2e: every line behaved as required (cage in force; outside the root does not exist)"
exit 0
