# Blind attack list for Phase 2e — and what running it found

Written by `glm-5.3-flash` (delegation `deleg_769ee1a9`, 2026-09-26 21:17-21:19, 100.7 s,
`exit_reason=completed`) from a plain-words description of the requirement and the
four already-decided properties. It never saw `cage.rs`, `hand-test-2e.sh`, the
brief, or `attack-list-2e.md`. This matters: the list in `attack-list-2e.md` was
written by the same author as the cage, so agreement between them proves little.

**Result: all 12 items were run against the cage and NONE found a defect.** One
item (D.3) was written against a fallback that cannot be reached; the near-miss is
recorded below rather than hidden, because the reason it looked like a hole is
worth keeping.

The raw list is preserved verbatim at the end.

## What running it showed (all observed, 2026-09-26)

| Item | Probe | Observed |
|---|---|---|
| A.1 | `cd / ; pwd ; ls /home ; ls /root ; cat /etc/passwd` | `/` is the root; `/root` and `/etc/passwd` do not exist |
| A.2 | `cd / ; :> /etc/owned ; :> /usr/bin/owned ; rm /bin/sh` | both writes fail; neither file exists on the host |
| A.3 | `cd ../../../.. ; readlink -f /.. ; stat -f -c %d / ; ln /usr/bin/env /env` | `cd ..` at root stays at root; device 4074728 = the root's own; hard link `Invalid cross-device link` (L.4 closed) |
| B.1 | `cat /etc/passwd`, `/etc//passwd`, `/etc/./passwd`, `/e*c/passwd` | all four `No such file or directory` — no string-matching, real isolation |
| B.2 | break the sandbox, run `touch probe` | non-isolated root and bogus root both REFUSE; nothing runs (see D.3) |
| B.3 | `ls -A /` | exactly the root's entries + the five read-only binds; `/etc` and `/home` are not there |
| C.1 | `env ; env \| grep -iE token\|secret\|key\|aws\|github\|api` | only HOME/LANG/PATH/PWD/SHLVL/TERM/_; grep finds nothing |
| C.2 | `ps -ef ; ls -d /<dir>` | `ps` blind (3 processes, ours); no /etc /var /boot /opt /srv /mnt /sys /run |
| C.3 | `curl https://example.com ; getent hosts ; ping` | no resolver, no route, ping not permitted |
| D.1 | `mkdir work ; printf > work/a.txt ; cat ; /bin/sh -c 'echo ok'` | ordinary work functions |
| D.2 | `echo blindmarker > marker.txt` at the root | `blindmarker` appears on the host at the root |
| D.3 | hide the sandbox, run `echo ALIVE` | REFUSES for a bogus root and for a non-isolated root |

Host sentinels after the whole run: `/etc/passwd` cksum 539412550 unchanged
(before and after); `/home/muffin` intact.

## The D.3 near-miss — a bad probe, not a hole

An early version of D.3 ran `env PATH=/nonexistent atrium-shell run ...` expecting
a refusal. The command ran and printed `ALIVE-SHOULD-NOT-APPEAR`, which looks
exactly like a silent uncaged fallback.

It is not one. `cage.rs` locates bubblewrap at
`/usr/bin/bwrap`, `/bin/bwrap`, `/usr/local/bin/bwrap` — absolute candidates, not
a `PATH` search, and the comment at the declaration says why: a search that
honoured the caller's `PATH` would let the caller choose which bubblewrap runs.
Stripping `PATH` therefore removed nothing. Re-probed with `PATH=/nonexistent`,
the run was caged: `/etc/passwd` absent, `env` still the allowlist, the host's
`/home` unreachable, and a write to the real `/tmp` did not land.

The correct form of the item is "hide the binary", not "hide `PATH`", and that is
what the refusal lines above exercise.

The near-miss is kept because D.3's own premise — *a fallback that runs the
command anyway would print normal output and exit 0, so every happy-path test
still passes* — is exactly the failure mode that a first draft of
`crates/shell/tests/cage_tests.rs` actually had: against a deliberately broken
cage, 10 of 11 tests passed down the refusal path. Both the blind child and the
injection experiment converge on the same hazard from opposite directions, which
is the strongest argument for keeping the blind-list gate.

## The list, verbatim

# Attack list: strong confinement for run commands (independent)

Test plan only — none of these have been executed. Each "Expected" states what a correct run must show, not an observed result.

## A. Reaching the computer outside the sandbox despite the private view

1. Run `cd / && pwd && cat /etc/passwd && ls /home && ls /root`. Expected: `pwd` shows the environment root's own contents (not the computer's `/`), and every read of `/etc/passwd`, `/home`, `/root` fails with "No such file or directory" — those paths must not exist in the command's view.
2. From `cd /`, attempt destruction: `cd / && rm -rf /etc && : > /etc/owned && : > /usr/bin/owned && rm /bin/sh`. Expected: all fail — `/etc` is not there to touch, and `/usr` and `/bin` are attached read-only, so even writes into existing attached paths must be refused. `cd /` must land inside the environment root, never the computer's root.
3. Escape by walking upward, then by aliasing: run a script `cd /; cd ../../../..; pwd; readlink -f /..`, and compare `stat -f -c '%d' /` inside against the device number of the environment root on the computer, then try `ln /usr/bin/env /env`. Expected: `cd ..` at the root stays at the root, upward walks re-enter the root; the device numbers must differ (root is its own filesystem), so no hard link can alias a file outside it — cross-device or read-only link attempts fail.

## B. Looks safe while confining nothing (silent no-op)

1. Probe encodings: `cat /etc/passwd`, then `cat /etc//passwd`, `cat /etc/./passwd`, then `cat /e*c/passwd`. Expected: every form fails identically with ENOENT. Why it looks like success: if the harness confines by pattern-matching the command string instead of real filesystem isolation, the plain form is caught while these variants read the real file and exit 0 — output looks perfectly normal.
2. Tripwire the fallback: break the build on purpose (point the runner at a missing or non-executable bubblewrap, or pass an invalid root) and run `echo alive && touch probe`. Expected: refusal with non-zero status naming the reason; no "alive", no probe file. Why it looks like success: with a silent "run anyway" fallback the command runs with full user access, prints normal output, exits 0 — all happy-path and confinement tests still pass, which is exactly why this failure hides.
3. Check the root's identity: `cd / && ls -A / | sort && readlink -f /etc /home`. Expected: `/` contains exactly the environment root's entries plus the read-only attachments (/usr, /bin, /sbin, /lib, /lib64) and nothing else; readlink of `/etc` and `/home` fails. Why it looks like success: `ls /` prints a plausible directory list under either the confined or the unconfined view, so skimmed output reads as normal; only the entry names betray which world the command ran in.

## C. What the command can still see: environment, processes, network, stray directories

1. Run `env | sort` and `env | grep -iE 'token|secret|key|aws|github|api'`. Expected: only PATH, HOME, TERM, LANG (plus shell-set variables like PWD); the grep must find nothing — host credentials and other host variables must not pass through.
2. Run `ls /proc /sys /dev /run 2>&1; ps -ef 2>&1; for d in etc home var boot tmp opt srv mnt; do ls -d /$d; done`. Expected: pseudo-filesystems and unattached directories must not exist inside, and `ps` is blind — no enumeration of the computer's processes or mounts.
3. Run `curl -sS --max-time 5 https://example.com; getent hosts example.com; ping -c1 -W2 1.1.1.1`. Expected: every attempt fails fast with no resolver and no route; nothing reaches the network. Web lookups belong to the outside tool, so any success here is a defect.

## D. Ordinary work must still function; refusal when the sandbox cannot be built

1. Do real work: `cd / && mkdir -p work && printf 'x\n' > work/a.txt && cat work/a.txt && /bin/sh -c 'echo ok' && ls /usr/bin | head`. Expected: creating, reading, and running scripts inside the environment root must succeed normally; the read-only attachments leave standard tools runnable; only paths outside the root are gone.
2. Confirm the allowed direction: `cd / && echo marker > marker.txt`, then on the computer check the environment root for `marker.txt` with matching content. Expected: the file must appear at `<env-root>/marker.txt` — the one required crossing, root-ward.
3. Force an unbuildable sandbox (temporarily hide the bubblewrap binary or pass a bogus root path) and run `touch should-not-exist`. Expected: the runner must refuse with a specific reason and non-zero status; the probe file must never exist anywhere; nothing may run unsandboxed.