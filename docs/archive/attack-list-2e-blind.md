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

## Found by a second round of probes (author, 26 Sep 2026) — a real-path DISCLOSURE

The 12 blind items and the author's own list both missed this. It is **not an escape** —
every reach-out test held — it is an information leak of the kind 2e's own promise covers.

`crates/shell/README.md` and `attack-list-2b.md` §A.8 both say the root's real path must
become unlearnable, and that closing it is "2e's to close". `pwd` was closed by making the
view virtual. `mountinfo` re-discloses it:

```
$ cat /proc/self/mountinfo          # inside the cage
2058 809 0:27 /atrium-mountinfo / rw,nosuid,nodev master:3 - tmpfs tmpfs ...
2061 2058 0:34 /@/usr /usr ro,... - btrfs /dev/nvme0n1p3 rw,...,subvolid=256,subvol=/@
```

Measured, root at `/dev/shm/atrium-mountinfo`:

| probe | observed |
|---|---|
| `grep atrium-mountinfo /proc/self/mountinfo` | **1 hit** — the root's real directory name |
| the same in `/proc/1/mountinfo` | **1 hit** |
| `grep atrium-mountinfo /proc/mounts`, `/proc/self/mounts` | 0 hits |
| `grep atrium-mountinfo /proc/self/maps` | 0 hits |
| host block device / btrfs subvolume in `mountinfo` and `/proc/mounts` | **`nvme0n1p3`, `/@`** |
| `ls -l /dev/nvme0n1p3` inside | does not exist |
| `head -c 16 /dev/nvme0n1p3` inside | does not exist |
| the root's path in atrium's own `REFUSE` line | **0 hits** — that assertion still holds |
| `pwd` inside | `/home/work` — closed |

**Cause, and why it is a design point rather than a slip.** The mount's root field reads
`/atrium-mountinfo` because the environment root is a *subdirectory of an already-mounted
tmpfs* (`/dev/shm`). The kernel reports the path of the mount's root within its source
filesystem. Any root that is a subdirectory of some existing mount will do this.

**What it is worth, stated plainly:** nothing outside the root becomes reachable. What leaks
is the root's directory name and the host's disk identity (`nvme0n1p3`, a btrfs subvolume id)
— reconnaissance, and a direct failure of the documented "the agent never learns the real
path exists" promise. It is the class the author's own list already tracks as
`!! HOST PATH DISCLOSED` (§F.9), so the vocabulary exists; the mechanism does not close it.

**Candidate fixes, none applied** (they change the root-mount design, and the phase is
blocked on the 2b decision):

1. Make the root the root of its own mount rather than a subdirectory of one, so the root
   field reads `/` — e.g. a dedicated tmpfs mounted at the root path.
2. A private `/proc` that hides the mount table. `--proc /proc` already creates a fresh
   procfs and does **not** suppress this, because `mountinfo` reports the namespace's mount
   table, which is where the bind mounts live.
3. Accept and document it — which would require correcting `README.md`, `attack-list-2b.md`
   §A.8 and the 2e decision text, because they currently over-claim.

Whichever is chosen, the test belongs in `cage_tests.rs` beside the `pwd` test: run
`grep <root-basename> /proc/self/mountinfo` inside and assert no hit.


## The channel every list here missed: the session keyring (26 Sep 2026) — FOUND, FIXED, TESTED

Found by probing after an independent attack list spent its budget researching
kernel keyrings and `open_by_handle_at`. Neither the author's list, the independent
list, `attack-list-2b.md`, `hand-test-2e.sh` nor `hand-test-2b.sh` mentioned keyctl
or a keyring once — measured, `grep -ci 'keyctl\|keyring'` returned 0 in all five.

**What was measured, before any fix.** A secret was placed in the host shell's
session keyring, and the cage was started from that shell:

| from inside the cage | observed |
|---|---|
| `keyctl show` | lists the host's session keyring, same id (`882676938`) |
| `keyctl print <host-key-id>` | **`SECRET-IN-SESSION`** |
| `keyctl add user atrium-from-cage P @s` | **succeeded**; the host then saw the key |
| `keyctl unlink <host-key-id> @s` | **succeeded**; the host's key was GONE and unreadable |

So the boundary leaked **read, write and delete** — not a disclosure, an escape
channel with the host's own credentials on it.

**Why `--clearenv` could not have caught it.** A keyring is kernel state, not an
environment variable. `lib.rs` gives the reason the environment is cleared at all —
"the user's shell environment on this machine holds API keys — this is a security
requirement, not tidiness" — and a forked child inherits a keyring the way it
inherits a file descriptor. Clearing the variables closed one door and left this one
open.

**Scope.** Only the **session** keyring crosses. `@u` and `@us` are separate inside
the cage (`keyctl print` on a key placed in the host's user keyring returns
`Permission denied`, and `keyctl show @u` lists a different ring).

**The fix.** The child enters a new, empty session keyring in `pre_exec`, before it
becomes the cage: `cage::join_new_session_keyring()`, one `keyctl(2)` syscall with
`KEYCTL_JOIN_SESSION_KEYRING`. Chosen over shelling out to the `keyctl` binary
either inside the cage or in `pre_exec`, because a `PATH` lookup in `pre_exec` is
both an allocation and an attacker-influenced resolution; and over `--unshare-user`,
because entering a child user namespace needs the same `setgroups` privilege that
`--unshare-all` already establishes here, with considerably more blast radius.

**It fails closed.** `cage::keyring_is_severable()` forks a throwaway process that
takes its own session keyring and reports the result, before anything is spawned. A
machine that refuses is `CageError::KeyringNotSeverable` — a refusal for the whole
run, because the alternative is a cage that reaches the user's keys.

**Verified after the fix**, with the host as the independent witness: the host can
still read its own key, the cage cannot read it by id or by name, the cage's write
does not appear in the host's keyring, the host's key is not deleted, and the cage
still runs python3, git and writes files. The regression test
`the_hosts_session_keyring_does_not_cross_the_cage` was checked against the defect —
with `join_new_session_keyring()` removed it fails with `THE HOST'S SESSION KEYRING
CROSSED THE CAGE`, and `hand-test-2e.sh`'s new D.4 fails two of its three lines.

## What this changes about the phase's claims

`--clearenv` plus an allowlist does **not** mean "the command starts with nothing of
the user's". It means "the command starts with none of the user's *environment
variables*". Kernel state inherited across a fork — keyrings, and whatever else a
future audit finds — has to be severed on its own, one channel at a time, and only a
probe shows which are open.


## Sweep: inherited kernel state, six items (26 Sep 2026)

Muffin asked for this after the keyring find, as one targeted sweep for the same
class: **kernel state a forked child inherits that clearing the environment does
not touch.** Every item below was probed BEFORE any fix, so "crosses" and "cannot
cross" are measured rather than argued.

### Item 2 — inherited descriptors: CROSSED, now fixed

A descriptor is not a path, so the cage's view of the filesystem does not govern it.
Measured: the embedding process opened fd 9 on a file outside the root and fd 8 on
another (no `CLOEXEC`). From inside the cage:

```
/proc/self/fd/8 -> /home/muffin/atrium-sweep-outside/written-by-cage.txt
/proc/self/fd/9 -> /home/muffin/atrium-sweep-outside/secret.txt
read fd 9:  HOST-CONTENT-DO-NOT-LEAK      <- the host file's contents
write fd 8: wrote OK                      <- 5 bytes landed on the host
```

Read and write to host paths with the boundary fully in place — as complete a bypass
as the keyring was. Rust's own `File` sets `CLOEXEC`, so atrium's opens were safe;
the leak is whatever the process *embedding* the runner happens to hold.

**Fix.** Every descriptor at or above 3, except the cage's status descriptor, is
closed in the child before it execs (`cage::close_all_but`), using `close_range(2)`
with a per-descriptor fallback for kernels before 5.9. It **verifies** the closure
with `fcntl(F_GETFD)` and refuses to exec if any survived — so this is a checked
guarantee, not an intention. Measured after: only `0 1 2 3` remain, the read is
empty, the write is refused, and a run with 100 descriptors inherited still shows
`0 1 2 3`.

### Item 1 — terminal injection (TIOCSTI / TIOCLINUX): precondition absent; injection itself UNPROVEN here

`--new-session` is in the cage and the command holds **no terminal descriptor at
all**: fd 0 is `/dev/null`, fd 1 and 2 are pipes, and `TIOCSTI` on each returns
`Inappropriate ioctl for device`. There is nothing for the ioctl to aim at.

Honest limit, and it is why this is not called proven: the injection itself could
not be demonstrated on this machine. `/proc/sys/dev/tty/legacy_tiocsti` is **0**, so
TIOCSTI requires `CAP_SYS_ADMIN`, which a caged process does not have (`CapEff` =
`0000000000000000`). A control run with a real pty and **no cage** also failed to
inject, so the probe cannot detect injection here and the caged result proves
nothing on its own. What is asserted instead is the stronger structural fact: no
terminal descriptor reaches the command, whether or not the ioctl would work. The
detector behind that assertion was validated by running it uncaged under a pty,
where it does report `fd0 IS-A-TTY`.

### Item 3 — outward signals and ptrace: BLOCKED by the namespace

With a host pid as the target, from inside the cage: `kill -0` → `No such process`,
`kill -TERM` → `No such process`, `PTRACE_ATTACH` → `rc=-1 errno=3 No such
process`. The private PID namespace means no host pid is addressable, and `/proc`
lists 4-5 processes, all the cage's own. Worth recording that the host's
`yama/ptrace_scope` is **0** — permissive — so the block here is the namespace, not
a policy setting that could be relaxed elsewhere.

### Item 4 — abstract unix sockets and D-Bus: BLOCKED

`/run/user/1000` does not exist inside, so the session bus's socket path is not
reachable (`No such file or directory`), and connecting to an abstract socket fails.
The host has seven abstract sockets listening including `@/tmp/.X11-unix/X0` and
`@/tmp/.ICE-unix/3010`. Note the limit on this evidence: the abstract-socket half
was inferred from the network namespace being unshared rather than proven by a
connection to a *live* host abstract socket — the harness connects to a name it
constructs. The filesystem half is directly observed.

### Item 5 — SysV/POSIX IPC and /dev/shm: BLOCKED

`/dev/shm` inside is the cage's own (`drwxr-xr-x 2 ... 40`, empty). The host's
marker file was not readable inside, and a write inside did not appear on the host.
The private IPC namespace is visible in `ipcs -m`: the host lists two shared-memory
segments (`shmid 0` and `1`, owned by `muffin`) and the cage lists **none**.
`/dev/mqueue` does not exist inside.

### Item 6 — resource limits: GAP, not fixed

The limits inside are the host's own: `ulimit -u` = 127069, `ulimit -n` = 1048576,
`ulimit -v` = unlimited, identical on both sides, and `/proc/self/cgroup` inside is
`0::/` — the run is in no cgroup of its own. A bounded burst of 200 processes
inside started 200 and they were all gone afterwards, which shows only that nothing
lingers; it says nothing about what an unbounded fork would do. **A command that
forks without limit can exhaust this machine.** Constraining it means cgroups,
which is its own design, deliberately not built in this topic. Filed as
[issue #89](https://github.com/ItzMuff1n/atrium/issues/89), parked.

### What the six items have in common

Five of the six are kernel state inherited across a fork — descriptors, a keyring,
and, before they are unshared, namespaces. `--clearenv` closes exactly one channel:
environment variables. Everything else has to be severed on its own, and only a
probe shows which channels are open. Both real leaks found in this phase (the
keyring, the descriptors) were found by probing rather than by reading, and both
were missed by two independently written attack lists.
