# Attack list — Phase 2e: strong confinement for `run commands`

**Written 26 Sep 2026, before any cage code exists.** That ordering is the point:
this file says what the cage must do. Written afterwards it becomes a description
of the code instead of a test of it. Phases 1, 1b, 2a, 2b, 2c and 2d were all
built this way, and 2b's and 2c's blind lists each found a real defect precisely
because the list existed independently of the code.

**Authority:** `BUILD-PLAN.md` §2e, quoted in full below; `DESIGN.md` §3.1 (the
root is a closed environment; host access is off entirely in v1; the root is its
own mount point) and §3.3 ("working directory confined is not confinement"); the
plan's Phase 2 verification step *"Try `cd /` and then a destructive command.
Confirm it cannot reach the host."*; `DECISIONS.md` — "Strong confinement for
`run commands` is its own phase" and "Phase 2e: bubblewrap, an explicit mount
list, and a root that is its own mount point".

**This is a working copy.** If it and `DESIGN.md` disagree, stop and say so — do
not pick one silently (`AGENT-RULES.md` §2).

**A note on what is already decided versus what this list tests.** The mechanism
is decided (bubblewrap, approved 26 Sep 2026). This list does **not** re-open
that; it tests whether the thing built to that decision actually holds. Where a
line depends on a choice Muffin made — the network being shut, the four-variable
environment allowlist, the read-only system binds — the line is written to hold
him to his own decision, not to second-guess it.

---

## The requirement, verbatim

> **The requirement, mechanism-agnostic.** A command run through the shell must be
> unable to reach anything outside the environment root — not "should not", not
> "we check its paths first". Inside the command's own view of the filesystem,
> outside the root **does not exist**. Read, write, delete, rename, `cd /`, a
> script that walks upward, a program that opens `/etc/passwd` by absolute path:
> all of them must fail because there is nothing there to reach.

> **Verification (user):**
>
> - **The moved step, kept verbatim:** *"Try `cd /` and then a destructive command.
>   Confirm it cannot reach the host."*
> - Plus, inside the shell, try to reach the host and fail: `cat /etc/passwd`, list
>   and write to a real directory outside the environment.
> - Then the other direction: confirm a file created **inside** the environment does
>   appear on the host inside the environment root — the cage must not be so tight
>   that the sandbox cannot be used.

---

## What is being tested, in plain words

Everything built so far assumes a boundary. Phase 1 decides which paths are
allowed; 2a acts on them; 2b runs real programs; 2c can put the environment back;
2d notices what changed. Every one of those rests on the claim that nothing
reaches outside the root.

**For file operations that claim is true, because the code checks every path. For
a real command it is false.** A command is not limited to paths: it can `cd /`, it
can open `/etc/passwd` by absolute name, it can write anywhere the user can. 2b
says this in its own header and `hand-test-2b.sh` §G demonstrates it with five
lines marked `HOLE`.

**2e is the phase that makes the claim true for a command.** It does it by
building, for each command, a private view of the filesystem in which the
environment root is the whole of `/`. "Outside the root" stops being somewhere
that is forbidden and becomes somewhere that is not there.

So this list is about two failure directions, and they are equally serious:

- **The cage leaks** — something outside the root is reachable from inside.
- **The cage lies** — it appears to work but is not actually confining anything,
  or it is so tight the sandbox cannot be used, or it silently degrades to running
  the command with no cage at all.

The third failure is the one that would do the most damage and is the easiest to
miss, because everything passes: a missing `bwrap` binary, a namespace refused by
the kernel, and a fallback that shrugs and runs the command anyway. **Fail closed
is therefore §F, its own section, not a paragraph.**

---

## The vocabulary of a result

- **CAGED** — the command runs, and the stated observable is true *inside the
  cage*: the path is absent, the write failed, the program ran. The command's exit
  status and output are the evidence.
- **REFUSED** — the command does **not** run, and the refusal names a reason. No
  process is spawned. Checked by the absence of the command's own side effects
  **and** by the runner's reported reason. A refusal is never a silent no-op.
- **SEALED** — the cage could not be built, and the runner refused to run the
  command at all. Distinguished from REFUSED: SEALED is the *fail-closed* path
  (no cage → no run), and this list treats a SEALED that is actually a CAGED as the
  worst possible outcome (§F).
- **NOT BROKEN** — the property this phase must not destroy: a file created inside
  the environment appears at the root, and ordinary programs still run. A cage
  that passes every leak test and fails these is a failure, not a success.
- **NOT HERE** — belongs to another phase; listed so the gap is not silent.

Three independent checks run alongside the lines. Any one of them fails the run
regardless of what the program printed about itself:

- **`!! OUTSIDE TOUCHED`** — the harness owns a real directory outside the root,
  holding a sentinel file, and records its full listing, its entries' checksums and
  the sentinel's checksum before the run. It re-checks after **every** line. Any
  change is an escape. This is the check that does not depend on the program
  telling the truth. **It must be built the way 2b's is** — deliberately **not**
  `ls -laR`, which prints each directory's `..` entry and so fires on unrelated
  churn in `/tmp` (defect #83, 26 Sep 2026). 2b's `fingerprint_dir` is the model.
- **`!! HOST /etc/passwd CHANGED`** — the host's `/etc/passwd` is checksummed, and
  its mtime recorded, before the run and after every line. A cage that lets a
  command modify it is the most direct possible failure of the requirement.
- **`!! HOST PATH DISCLOSED`** — no line of the runner's default output may
  contain the environment root's real on-disk path, on any line including the
  refusals. This is `DESIGN.md` §3.1's "the agent never learns the real path
  exists", and 2a's §H.4/H.5 set the precedent.

**A specific danger for this phase's own harness.** The cage under test *is* a
program that runs other programs, and the harness drives it. So the harness must
never let the thing under test choose what gets executed outside the cage. Every
line spawns the cage through the built binary; **no line may fall back to running
a bare command on the host to "check something"**, because that would put the
host's own state into the result.

---

## Fixtures

Inside a throwaway root (`/tmp/atrium-2e-root` by default):

```
home/work/                        (empty dir — a cwd that resolves)
home/documents/notes.txt          ("hi")
home/documents/sub/deep.txt       ("deep")
etc/                              (a decoy: an "etc" INSIDE the root)
etc/passwd                        ("root:x:0:0:sandbox-decoy:/:/bin/sh")
outside-link -> <the outside dir> (absolute symlink pointing OUT)
outside-hard -> <outside>/sentinel.txt  (hard link — only possible if the root
                                   shares a filesystem with the outside dir, so
                                   this fixture is created and its success or
                                   failure is itself a result: see §D)
mode-444.txt                      (chmod 444 — a write must fail on its own merits)
```

Outside the root (`/tmp/atrium-2e-outside`), owned by the harness:

```
sentinel.txt        ("OUTSIDE SECRET")
sub/keep.txt        ("keep")
```

**Why the root contains its own `etc/passwd`.** The requirement's own example is
`cat /etc/passwd`. A cage that makes `/etc/passwd` absent **and** a cage that
merely shadowed the host's with the root's own would both print nothing useful —
so the decoy distinguishes them. The line asserts that `/etc/passwd` inside the
cage is **either absent or the decoy, and never the host's real file** (2,943
bytes on this machine, `root:x:0:0:root:/root:/bin/bash`).

**Why the outside directory is harness-owned and not a real host path.** Same
reason as 2c: pointing a test at `/etc` makes the failure mode *writable* against
the real machine. A harness-owned directory makes it harmless and **visible** —
if the cage leaks, `sentinel.txt`'s contents appear inside the cage, and the
outside fingerprint changes.

---

## A. The moved verification step — `cd /` then a destructive command

`A.0` is the plan's own step, kept verbatim.

| # | line | expected |
|---|---|---|
| A.0 | cage a command: `cd / && pwd` | CAGED — prints `/`; the *cwd* is the cage's root, not the host's |
| A.1 | `cd / && ls -A` | CAGED — output contains **no** host-only entry; the root's own entries may appear |
| A.2 | `cd / && rm -rf ./*` | CAGED — nothing outside the root is removed; `!! OUTSIDE TOUCHED` silent; host `/etc/passwd` unchanged |
| A.3 | `cd / && rm -rf /` | CAGED — the command may fail; the host must be untouched and the process must terminate (no hang) |
| A.4 | `cd /` then a destructive command on a **known host path**: `rm -rf /home/muffin` | CAGED — `/home/muffin` does not exist in the cage, so `rm` reports "No such file or directory" **and the real one is untouched** |
| A.5 | `cd / && find / -maxdepth 3 -name '*.txt' 2>/dev/null` | CAGED — every hit is inside the root or in a bound read-only system dir; the host's outside directory is never named |
| A.6 | `cd / && sh -c 'cd ../../..; pwd'` | CAGED — walking upward out of the root lands back inside the cage's `/`, never on the host |
| A.7 | the same as A.2, run twice in a row | CAGED both times — a second cage is not weaker than the first (no state carried between runs) |

**Why A.2 is here and is the plan's own line.** "Try `cd /` and then a destructive
command. Confirm it cannot reach the host." It is the only line in the whole phase
that comes from the plan verbatim, and it is the one a reader will look for.

---

## B. Reaching the host and failing — the plan's second line

| # | line | expected |
|---|---|---|
| B.1 | `cat /etc/passwd` | CAGED — absent **or** the root's decoy; never the host's 2,943-byte file |
| B.2 | `ls /etc` | CAGED — absent, or the root's own `etc` only |
| B.3 | `cat /home/muffin/.zshrc` | CAGED — "No such file or directory"; `/home/muffin` does not exist |
| B.4 | list a real directory outside the environment: `ls -A <outside>` | CAGED — the path is not reachable; the command sees nothing |
| B.5 | write to a real directory outside the environment: `touch <outside>/pwned` | CAGED — fails; `<outside>/pwned` does not exist on the host afterwards |
| B.6 | `cat <outside>/sentinel.txt` | CAGED — not readable; the outside directory is not in the cage's tree |
| B.7 | `cat /proc/1/cmdline` | CAGED — either unreadable or about the cage's own init, never a host process's command line |
| B.8 | `cat /proc/self/environ` | CAGED — contains **only** allowlisted names (§A of the requirements below), never a host variable |
| B.9 | `cat /sys/kernel/security/lsm` | CAGED — absent (no `/sys` bind), so the cage's own LSM config is not even discoverable |
| B.10 | open a host path through a **relative** walk: `sh -c 'cd /; while :; do cd ..; pwd; done'` | CAGED — never leaves `/`; the command terminates under the runner's time limit |
| B.11 | try to mount something: `mount -t tmpfs tmpfs /mnt` | CAGED — fails; a caged command cannot widen its own view |
| B.12 | try `chroot` a directory: `chroot / /bin/true` | CAGED — fails (needs privilege), and does not create an escape |

**Why B.5 is here rather than only in A.** A.2 destroys; B.5 is the smallest
possible *write* to a real outside path, and a cage can plausibly block `rm` (which
needs to unlink) while allowing a create. They are different syscalls on different
objects.

---

## C. The other direction — the cage must not be too tight

The plan states this as a verification line. A cage that seals everything is not
the requirement; it is a different failure.

| # | line | expected |
|---|---|---|
| C.1 | write a file at `/home/work/created.txt` inside, then check the **real root** | NOT BROKEN — the file is present at `<root>/home/work/created.txt` with its contents |
| C.2 | `python3 -c 'print(1+1)'` | NOT BROKEN — prints `2` |
| C.3 | `git --version` | NOT BROKEN — prints a version |
| C.4 | `awk 'BEGIN{print 3*4}'` and `printf 'a\nb\n' \| wc -l` | NOT BROKEN — `12` and `2` |
| C.5 | `sh -c 'mkdir -p a/b/c && touch a/b/c/f && ls a/b/c'` | NOT BROKEN — the tree is created inside the root and is visible at the real root afterwards |
| C.6 | stdout, stderr and exit code are still reported faithfully: `sh -c 'echo out; echo err >&2; exit 7'` | NOT BROKEN — three-way status preserved: exit 7, both streams intact. A cage that collapses or drops streams fails here |
| C.7 | `HOME` is writable and is the cage's own root, not the host's home | NOT BROKEN — a program writing to `$HOME` writes **inside the environment root**, never to `/home/muffin` |
| C.8 | `TERM` and `LANG` are set as the allowlist promises | NOT BROKEN — both present with the values the runner set |
| C.9 | a command's output cap still applies inside the cage (emit more than the cap) | NOT BROKEN — truncated and **reported as truncated**, never silent (2b §F.3) |
| C.10 | the runner's time limit still applies inside the cage (`sleep` past it) | NOT BROKEN — reported as a timeout, not as exit 0, and not as a hang |
| C.11 | a survivor: `sh -c '(sleep 30) & echo started'` | NOT BROKEN *and not leaked* — the run returns promptly; no `sleep 30` is left running on the host after the runner reports. This is 2b §N.8's reader-grace problem in its new form |
| C.12 | `pwd` inside the cage equals `/` when the cwd was `/`, and the virtual path the agent asked for, not a real host path | NOT BROKEN — the virtual-path contract survives the cage |

---

## D. The hard link — open item L.4, and the root as its own mount point

2a found this: a **hard link is a second name for one file, not a path**, so no
path check can see it. `DECISIONS.md` hands it to 2e. `DESIGN.md` §3.1 requires the
root to be its own mount point. This section is where that requirement is tested.

| # | line | expected |
|---|---|---|
| D.1 | the fixture creation itself: `ln <outside>/sentinel.txt <root>/outside-hard` | **the fixture's success is the finding.** If the link is created, the root shares a filesystem with the outside dir and the channel is open. Reported as such, not silently tolerated |
| D.2 | if D.1 succeeded, `cat /outside-hard` from inside the cage | CAGED — the file must not be readable. **This is the escape this phase exists to close** |
| D.3 | from the **host**, try `ln <outside>/sentinel.txt <root>/hostlink` while the sandbox is up | REFUSED by the filesystem — `Invalid cross-device link`, because the root is its own mount point. If the link **succeeds**, the mount point is not in force and this is a FAIL, not a note |
| D.4 | inside the cage, `ln` two files **within** the root | CAGED *success* — same-filesystem links must still work. A root that forbade all hard links would break real builds |
| D.5 | inside the cage, `ln /home/work/notes.txt /usr/bin/link` | CAGED — fails: read-only, and across devices |
| D.6 | `stat -c %d` on the cage's `/` compared with the device of the outside directory | the two devices differ. Equal devices mean the root is **not** its own mount point |
| D.7 | run the binary with a root that is an ordinary directory inside a larger volume | REFUSED with a reason — the mount-point precondition is enforced, not assumed |
| D.8 | the same, with the root being a path that does not exist | REFUSED, and no process is started |
| D.9 | the outside directory and the root on the **same** filesystem, deliberately | REFUSED — this is D.7 stated as an explicit fixture rather than relying on the machine's layout |
| D.10 | bind-mount trick: from the host, make a **bind** of the outside directory appear inside the root before the run | CAGED — the cage's own mount list decides what exists; a host-side bind does not survive into it |

**Why D.3 is graded as a FAIL and not a comment.** `DESIGN.md` §3.1 says the root
is its own mount point. If a hard link can still be made into it from the host
while the sandbox is up, the sentence is false and the hole 2a found is still
open. Recording it as "observed, not fixed" is exactly the outcome that would let
this phase be signed off with the defect intact.

---

## E. The environment — nothing from the host, and only what was allowed

Muffin's requirement A, 26 Sep 2026: `--clearenv`, then pass back **only** an
explicit allowlist — `PATH`, `HOME`, `TERM`, `LANG`. Nothing else.

| # | line | expected |
|---|---|---|
| E.1 | with `SSH_AUTH_SOCK`, `GH_AUDIT_TOKEN`, `HERMES_TEST`, `DBUS_SESSION_BUS_ADDRESS` and `KITTY_PUBLIC_KEY` set **on the host**, run `env` inside the cage | CAGED — **none** of those five names appears |
| E.2 | the same `env` output | CAGED — **every** name present is in `{PATH, HOME, TERM, LANG, PWD, SHLVL, _}`, and nothing else. `PWD`/`SHLVL` are added by the shell and `_` by glibc's `env`; they are permitted **by name, with that reason in the test**, and are the only permitted extras |
| E.3 | `echo $SSH_AUTH_SOCK`, `$GH_AUDIT_TOKEN`, `$HERMES_TEST` | CAGED — all three empty |
| E.4 | `PATH` inside the cage | CAGED — exactly the fixed minimal PATH the runner sets (`/usr/bin:/bin`), never the host's PATH |
| E.5 | `HOME` inside the cage | CAGED — the cage's root, never `/home/muffin` |
| E.6 | a variable set to an **empty string** on the host | CAGED — does not appear. An empty value must not be mistaken for "unset, so leave it" |
| E.7 | a host variable whose name collides with one the runner sets: `LANG` | CAGED — the runner's value wins, and the host's value is not visible anywhere |
| E.8 | `env` output is compared to the allowlist as a **set**, not by grepping for a few known-bad names | CAGED — a new host variable added tomorrow cannot leak past a test that only lists today's names |
| E.9 | `TERM` and `LANG` are actually set (not merely absent) | CAGED — both present and non-empty, because a program that needs them must not be broken by the allowlist |
| E.10 | run a program that reads `HOME` to place a cache: `sh -c 'echo cache > $HOME/.cache-test && cat $HOME/.cache-test'` | NOT BROKEN — works, and the file lands **inside the root**, not in the host's home |

**Why E.8 exists.** E.1's five names are today's known leaks, measured. A test that
only asserts those five are absent passes forever while a sixth appears. Comparing
the **whole** set to the allowlist is the assertion that cannot rot.

**This is where the letter of the requirement meets a real constraint, and the
list records it rather than pretending.** "Only the allowlisted names" cannot be
literally true: a POSIX shell adds `PWD` and `SHLVL` unconditionally, and glibc's
`env` prints `_`. Measured: the count is **7, not 4**. E.2 is therefore written as
*a closed set of seven with three named and justified*, and **nothing outside those
seven is permitted**. An assertion of "exactly four names" would pass only against
a cage that did not use a shell — a lie about the mechanism.

---

## F. Fail closed — the cage that is not there

The dangerous version of this feature is one that silently degrades to running the
command uncaged. This section exists to make that the loudest failure in the run.

| # | line | expected |
|---|---|---|
| F.1 | run with `PATH` emptied so `bwrap` is not findable | **SEALED** — the command is REFUSED with a reason naming the missing cage. **No process runs.** The command's own side effect (e.g. writing a file) does not happen |
| F.2 | the same, with a **decoy** `bwrap` earlier on `PATH` that exits 0 without doing anything | SEALED — a cage that reports success without confining is refused. The runner must verify the cage is real, not merely that a program named `bwrap` ran |
| F.3 | run with the binary pointed at a path that is not executable | SEALED, with a reason |
| F.4 | user namespaces refused (simulated by making the binary's own probe fail) | SEALED — REFUSED with a reason; **never** a fallback run |
| F.5 | the root is not its own mount point (D.7) | SEALED — REFUSED, and no process starts |
| F.6 | any SEALED line: the command's stdout reaches the caller **as a refusal**, and the exit status is not success | SEALED — the failure is reported, never returned as exit 0 |
| F.7 | `grep` the code for a code path that spawns the command **without** the cage arguments | **no such path exists.** A structural line: the only spawn site builds the cage first, so "run uncaged" is not expressible |
| F.8 | a refusal must not leave a half-built root, a stray process, or a lock behind | SEALED — the runner's own state is unchanged after a refusal |
| F.9 | a refusal reason never names the host's real path or the cage's internal paths in a way that discloses layout | SEALED — plain words, virtual paths, no real path on any default-output line (`!! HOST PATH DISCLOSED`) |
| F.10 | the same refusal twice in a row is identical | SEALED — deterministic; a refusal that varies is a refusal that can be argued with |

**Why F.2 is here.** The obvious implementation of "is the cage available?" is to
run something and check the exit code — and then every test above passes against a
cage that does nothing. A decoy that exits 0 is the cheapest possible way to catch
that, and it is the line most likely to be omitted.

---

## G. The mount list — what exists inside, and what does not

Muffin's requirement B, 26 Sep 2026: system folders a real shell needs are bound
read-only; `/home`, and any host path not explicitly bound, must not exist inside
the cage.

| # | line | expected |
|---|---|---|
| G.1 | `ls /` inside the cage | CAGED — contains only the root's own entries plus the bound read-only system dirs (`usr`, `bin`, `sbin`, `lib`, `lib64`) and the runtime mounts (`proc`, `dev`). **No** host-only entry |
| G.2 | writing to a bound read-only system dir: `touch /usr/evil`, `echo > /usr/bin/evil`, `mkdir /usr/newdir` | CAGED — every one fails with a read-only error, **and nothing appears at the real `/usr` on the host** |
| G.3 | `/home` exists and contains exactly the root's own contents | CAGED — `/home` is **inside** the root, so it must be present and hold the root's `home/work`. Its **host** counterpart is what must not be reachable |
| G.4 | `/home/muffin` — the host's real home path | CAGED — **absent**. This is the sharp form of "`/home` must not exist" and the line that actually matters |
| G.5 | `/root`, `/var`, `/opt`, `/srv`, `/boot`, `/media`, `/mnt` | CAGED — none exists inside the cage (nothing binds them) |
| G.6 | `/etc` | CAGED — absent, or the root's own `etc` only; never the host's |
| G.7 | `/tmp` inside the cage | CAGED — if present it is the cage's own private one, and a file written there does **not** appear in the host's `/tmp` |
| G.8 | `/dev` | CAGED — a fresh minimal `/dev`; no host device is reachable through it |
| G.9 | `/proc` | CAGED — a private proc: `ps -e` shows only the cage's own processes (measured: ~5 lines) |
| G.10 | a system dir bound read-only is still **usable**: `ls /usr/bin \| wc -l` and running `/usr/bin/git --version` | NOT BROKEN — read-only means not writable, not unreadable |
| G.11 | the cage's `/dev/null`, `/dev/stdout` etc. work | NOT BROKEN — `sh -c 'echo x > /dev/null'` succeeds and `> /dev/stdout` prints |
| G.12 | **the host's `/home` and `/home/muffin` cannot be reached even by an absolute path walked from `/`** | CAGED — `find / -maxdepth 2 -name 'muffin*'` returns nothing |
| G.13 | a later added bind must not silently re-expose a host path: with an **extra** bind configured, re-run G.4 | CAGED — still absent, or the change is caught here. *Recorded as the line that would catch a future edit under `/home`* |

**Why G.4 is separated from G.3, and this is the correction that matters.** The
plan originally proposed "`ls /home` inside the cage fails or is empty". Measured:
it **returns rc=0 and prints `work`** — because `/home` is *inside the environment
root*, bound at `/`, so the root's own `home/work` **is** the sandbox's
`/home/work`. That is correct and required. A test asserting "`ls /home` fails"
would have **failed against a correct cage**, and worse, would have pushed the
implementation toward not having `/home` at all — breaking the sandbox's own files.
G.3 and G.4 split the requirement into the two things that are actually true:
`/home` present with the root's contents, `/home/muffin` absent.

---

## H. The network — shut, and the fetch path is not a hole in the cage

Muffin's decision D1, 26 Sep 2026: **the network inside the cage is SHUT**, and the
agent's ability to look things up comes from a separate fetch/search tool the loop
calls **outside** bubblewrap — never from the cage.

| # | line | expected |
|---|---|---|
| H.1 | `sh -c '(exec 3<>/dev/tcp/1.1.1.1/443)'` | CAGED — unreachable |
| H.2 | resolve a name: `getent hosts github.com`, `nslookup` if present | CAGED — fails; no DNS |
| H.3 | `curl`/`wget`/`git clone` out | CAGED — fail; if the binary is absent, that is also a pass, but the line must be run so its absence is recorded rather than assumed |
| H.4 | a program that opens a socket internally: `python3 -c 'import socket;socket.create_connection(("1.1.1.1",443),2)'` | CAGED — raises, and does not hang past the runner's limit |
| H.5 | `cat /etc/resolv.conf` and `/etc/hosts` | CAGED — absent (no `/etc` bind), so name resolution has nothing to read |
| H.6 | the **fetch tool path** run outside the cage: a fetch of a real URL | NOT HERE / NOT BROKEN — it must work, and it must emit a `fetched` effect. **This is the requirement D1 rests on: the capability moved, it did not disappear** |
| H.7 | the fetch tool given a `file://` URL, or a host path | REFUSED — a fetch is not a way to read a local file, and not a way around Phase 2e |
| H.8 | the fetch tool given a URL that **fails** (unreachable host) | REFUSED — and **emits no `fetched` effect**, because it brought nothing in. A failed fetch recorded as a successful one is a false effect |
| H.9 | no line anywhere in this harness opens the cage's network "to make a test work" | structural — the mount list and flags are read from the runner's own construction, and a grep asserts no `--share-net` or equivalent appears in the cage invocation |

**Why H.6 is in this list at all.** It is the one line that guards the *decision*
rather than the code. If the fetch tool does not exist when the cage shuts the
network, the agent is left unable to look anything up — which is what Muffin
explicitly required must not happen. H.6 fails until that tool exists, so the gap
cannot be forgotten while the cage is signed off.

**Why H.9 is structural rather than behavioural.** A cage whose network is opened
for one test and shut for the others would pass H.1–H.5 and be wrong. The only way
to catch that is to read the construction. Stated as such, because a behavioural
list alone cannot see it.

---

## I. Process, signal and lifetime

A cage adds a process boundary, and boundaries lose things.

| # | line | expected |
|---|---|---|
| I.1 | `pwd` inside the cage with several different virtual cwds | CAGED — each reports the path the agent asked for, translated correctly, never a real host path |
| I.2 | a command that exits non-zero: exit code preserved exactly | CAGED — the same three-way status as 2b (exit code / signal / timeout); never collapsed into one number |
| I.3 | a command killed by a signal | CAGED — reported as a signal, not as exit 0. *2b's worst-outcome rule (attack-list-2b.md §C.6) applies unchanged* |
| I.4 | stdin is not the user's terminal: a command reading stdin gets immediate EOF | CAGED — no hang, no read from the terminal |
| I.5 | a command that ignores SIGTERM and runs past the limit | CAGED — reported as a timeout; the cage itself must not be left running on the host |
| I.6 | after a timeout, no `bwrap` process is left behind on the host | CAGED — checked by listing the host's processes for the cage's own name after the run |
| I.7 | two cages run in sequence: the second is unaffected by the first | CAGED — no leaked mount, no leaked namespace, no leaked directory |
| I.8 | a command that daemonises and exits leaves a survivor (**2b §N.8 / K.6**) | NOT BROKEN — the runner returns promptly, does not wait for the survivor's lifetime, and does not claim the survivor's output |
| I.9 | a cage run concurrently with a second cage run | CAGED both — if Phase 2e does not support concurrency, that is stated plainly here rather than left to be discovered |
| I.10 | the runner's own exit status after a CAGED, a REFUSED and a SEALED line | the three are distinguishable by the caller. A SEALED reported as success is the F-section failure arriving through the status channel |

---

## J. The boring half — ordinary use must simply work

| # | line | expected |
|---|---|---|
| J.1 | `echo hi` | NOT BROKEN — `hi` |
| J.2 | `ls` in a populated cwd | NOT BROKEN — the entries |
| J.3 | `mkdir newdir && pwd` | NOT BROKEN — the directory exists inside the root afterwards |
| J.4 | a pipeline: `printf 'a\nb\n' \| wc -l` | NOT BROKEN — `2` |
| J.5 | `exit 3` | NOT BROKEN — status `exit 3` |
| J.6 | a command that writes to both streams and reads its own file | NOT BROKEN |
| J.7 | `cargo --version`, `rustc --version` if on the image | NOT BROKEN if present; if absent because nothing binds them, that is **recorded** — a build tool missing from the cage is a real limitation for the long-term goal and must not be a silent one |
| J.8 | a non-UTF-8 argument | NOT HERE — not representable; the API takes `String` (2b §K.7). No line can be run |

---

## K. What this phase does not do

Written down, not built (`AGENT-RULES.md` §4).

1. **The fetch/search tool itself is Phase 5's.** 2e owns the decision that the
   cage's network stays shut and that the capability lives outside it; it does not
   build the tool. H.6 fails until Phase 5 lands it, deliberately.
2. **Network isolation is not the same as no-network in the design sense.**
   `--unshare-all` gives the command no network interface. Whether a *later*
   controlled egress is wanted is a product decision, not this phase's.
3. **The root is private to Atrium.** Recorded in `DECISIONS.md` and here: because
   the mount namespace is ours and unprivileged, a second terminal on the host sees
   the underlying directory, not the tmpfs. 2e does not change that; a
   root-at-boot design would, and it needs root once at setup.
4. **Landlock, containers and chroot are not used.** Rejected in `DECISIONS.md`
   with reasons; recorded here so their absence is a decision, not an oversight.
5. **This cage confines a command's filesystem view.** It is not a seccomp policy
   — a caged command can still use any syscall available to the user, subject to
   the namespace. Syscall-level restriction is not in 2e's requirement and is not
   claimed.
6. **Resource limits (CPU, memory, disk) are not in 2e.** A caged command can still
   consume the machine. Named here because the cage's name invites the assumption
   that it covers this; it does not.
7. **The timeout remains 2b's safety stop**, not `DESIGN.md` §7.1's user-facing
   kill switch (Phase 4).

---

## Independence

This list was written by the head that also wrote the plan and will verify the
result, so the two share assumptions. Before the phase is accepted, a **blind
second list** must come from a subagent that has seen **none** of: the `shell/`
source, the plan issue, this file, or any other attack list. It gets only the
requirement text and the rules, and is asked what it would try.

The blind list is **a gate, not an extra**. Its value is mechanism-level: 2b's
blind list found a real defect, 2c's found a real defect. Lines it contributes are
appended as a new section and never folded into the existing ones — so it stays
visible which head raised what.

**The specific risk this phase's independence check addresses.** The requirement
has a sharp shape — "outside does not exist" — and it is easy to write a cage that
satisfies the shape and not the intent: bind the root, bind `/usr` read-only, and
declare victory, while the environment still leaks (measured: a bare bwrap cage
hands the command `SSH_AUTH_SOCK`, `GH_AUDIT_TOKEN` and the whole `HERMES_*` set),
or the process list is still the host's, or the root is still a directory inside a
larger volume so the hard link still works. **Each of those three was found by
probing this phase rather than by reading it** — which is exactly why the blind
list is a gate.

---

## How this list was derived

Not invented from nothing. Its sources, in order:

1. `BUILD-PLAN.md` §2e and its verification block, quoted above.
2. `DESIGN.md` §3.1 (closed environment; host access off; the root is its own
   mount point) and §3.3 ("working directory confined" is not confinement; the
   cage is 2e and is not optional).
3. `DECISIONS.md` — the 2e phase decision, the L.4 hand-forward, and the 26 Sep
   2026 entry recording the mechanism, the allowlist, the read-only binds and the
   fail-closed rule.
4. **Muffin's decisions of 26 Sep 2026** — the bubblewrap dependency, D1 (network
   shut, capability moved outside), D2 (Atrium's own namespace), and requirements
   A and B, each of which owns a section here (E, G).
5. `STATUS.md`'s 26 Sep entry — the CI probe (run 36244899076) and the three
   undecided observations it left (network, process list, environment leak).
6. **The probes run by this session before writing this list** — the mechanism
   measurements, the fail-closed paths, the `--tmpfs /` contradiction, and the
   `ls /home` correction. Scripts: `/tmp/probe-2e*.sh`, `/tmp/probe-ab.sh`.
7. The shape of Phases 1, 1b, 2a and 2b: fixtures, lettered sections, an
   independent outside sentinel, refusals checked by absence rather than by exit
   code, and a blind second list before acceptance.

**The ordering claim this file rests on:** §D, §E, §F and §G are in this list
*before* any cage code exists, because they are the four ways this phase fails
silently —

- **§D**: the root is not really its own mount point, so the hard link 2a found
  still works and the phase is signed off with the hole intact;
- **§E**: the environment leaks, so the cage hides less than it shows;
- **§F**: the cage is absent and a fallback runs the command anyway, so every
  other line passes against nothing;
- **§G**: the mount list is wrong in one entry, so one host path is reachable and
  no behavioural test notices because no test named it.

Every one of them produces something that looks like success.
