# atrium-shell

Phase 2b of Atrium — **the real shell** (`BUILD-PLAN.md` §2b). It runs a real
command and gives back what it did.

```
atrium-shell run --root <dir> --cwd <vpath> [--timeout-ms N]
                 [--max-output-bytes N] [--show-real] -- program [args...]
atrium-shell demo
```

## Read this first: what it guarantees, and what it does not

**It guarantees where a command starts, and what it is handed.**

- The working directory is the directory that the virtual path resolves to
  inside the environment root, and the path is resolved by Phase 1's resolver
  **before anything is spawned**. A path that fails to resolve is refused and
  **no process starts**.
- The command gets a **fixed, non-inherited environment** — not the user's. On
  this machine the user's session environment holds API keys, so this is a
  security requirement, not tidiness.
- The command's **input is not the user's terminal** (stdin is `/dev/null`).
- stdout and stderr come back as **exact bytes**, separately, with the exit
  code — or the terminating signal, or a timeout, as three distinct things.

**It does not guarantee what the command can then reach.**

A command is a real program running as the user. It can `cd /`; it can open an
absolute path; it can do anything the user can. **That is by design**, and it is
not a defect in this phase:

> `DESIGN.md` §3.3: "working directory confined" is **not** confinement. Closing
> this is `BUILD-PLAN.md` **§2e**, a required later phase, and it gates the agent
> loop (Phase 5).

`hand-test-2b.sh` §G **demonstrates that hole and marks it** — five lines that
reach the host, printed as `HOLE`, not as passes. They are marked so a passing
run cannot be misread as confinement. Do not treat a green 2b run as a sandbox.

## The limits, and why they are what they are

| Setting | Value | Why |
|---|---|---|
| time limit | **30 s** default, `--timeout-ms` | A safety stop so the runner cannot be hung by a command that never ends. **It is not the kill switch** — that is `DESIGN.md` §7.1 / Phase 4 and is user-facing. See §K.1 of the attack list. |
| output cap | **1 MiB** per stream default, `--max-output-bytes` | Bounds memory on a flood. Truncation is always **reported**; silent truncation would make a flood look like a small command. |
| `PATH` | `/usr/bin:/bin` | Fixed and minimal. Nothing from the user's session. |
| `HOME`, `TMPDIR` | the environment root | So a command writing `"$HOME/..."` or `"$TMPDIR/..."` lands inside the root. |
| stdin | `/dev/null` | A command that reads stdin sees EOF at once and never blocks on a person. |
| reader grace | 200 ms after the command ends | See "Survivors" below. |

## Survivors — a command's children are not killed

A command can background something that outlives it (`sh -c '(sleep 30) &'`).
That survivor is **not killed**. Two consequences, both deliberate and both
recorded rather than hidden (`attack-list-2b.md` §N.9):

1. **The runner does not wait for it.** It returns when the *command* ends, after
   a short grace period for its reader threads. Before this was handled, a
   30-second survivor made the runner block for the full 30 s while its own 2 s
   limit had already expired — a real defect, found by the blind attack list and
   fixed. `cargo test n8_` holds the fix in place (proven load-bearing: without
   it the test fails after 12 s).
2. **The survivor keeps running.** Killing a process group is containment, and
   containment is §2e. A survivor per task is leakage that accumulates, so if
   §2e does not end up covering it, something else must.

## The status line

```
OK status=<exit N|signal N|timed-out> stdout=<n> bytes stderr=<m> bytes[ stdout-truncated][ stderr-truncated]
```

A **signal is not an exit code** and neither is a **timeout**. The worst single
outcome this phase could produce is a command killed by `SIGKILL` being reported
as exit 0, so the three are separate variants in the type, never one number.

Refusals are one line, `REFUSE <vpath> — <reason>`, exit code 1. The reason names
the **virtual** path and the specific thing that was wrong — never the sandbox's
real location on disk. Exit codes: `0` success, `1` refusal, `2` usage error.

## What it deliberately does not do

- ~~**No cage, no bubblewrap, no namespaces, no Landlock, no container.**~~ **No
  longer true as of Phase 2e (26 Sep 2026).** The runner cages by default: every
  command runs inside a bubblewrap sandbox whose view of the filesystem has the
  environment root at `/`, with the network and the host environment closed, a fresh
  PID namespace, and the session keyring and inherited descriptors severed. See
  "Phase 2e" below and `crates/shell/src/cage.rs`. There is still no Landlock and no
  container, and `run()` cannot be asked to skip the cage — there is no such option
  reachable from the CLI.
- **No shell parsing.** `run` takes a program and an argv; a line of shell is the
  caller's business (`sh -c '<line>'`). The program and its arguments are passed
  through untouched — not resolved, not checked, not rewritten. A command may
  therefore touch paths that do not exist yet (§J.7).
- **No PTY, no interactive terminal, no job control.**
- **No effect log** (Phase 3), **no snapshots** (2c), **no watcher** (2d),
  **no kill switch** (Phase 4).

## A command can learn where the sandbox lives

Under 2b, `pwd` printed the root's **real** on-disk path, because that is the
command's own output and the runner does not rewrite it — redacting a command's
output would corrupt data (a build log, a checksum, a diff) and would be the runner
lying about what happened. Under §2e the command's view of the filesystem has the
root at `/`, so there is no real path for it to print — **which is what 2b's own
documents asked for**: "Under §2e ... there is no real path for it to print. This is
a requirement on §2e, not a defect here", and `attack-list-2b.md` §A.8/§H.4, whose
closing list reads "A command can learn the sandbox's real path. **2e's to close.**"
That requirement is now met for `pwd` and for the resolver's messages.

It is **not** met by `/proc/self/mountinfo`, which reports the mount table — the
root's host path and the backing device — inside the cage. Measured, with only
`/proc/self/mountinfo` and `/proc/1/mountinfo` affected (`/proc/mounts` and
`/proc/self/maps` are clean). Closing that needs the root to be the root of its own
mount rather than a subdirectory of one; recorded in
`docs/archive/attack-list-2e-blind.md`.

The runner's **own** sentences never name a real path. Resolver rejections are
re-worded from the resolver's structured error fields — the same pattern
`fileops/src/lib.rs` uses. This is asserted in two independent places: the helper
`assert_refused` in the test suite refuses any message containing the root's or
the outside directory's real path, and the harness checks every `REFUSE` line's
output for both. `n_disclosure_no_real_path_in_any_refusal` makes it explicit.
If the resolver ever grows another variant whose reason names a real path, that
re-wording must be updated; the comment in `src/lib.rs` says so.

## Files

- `src/lib.rs` — the runner, `run()`, `Outcome`, `ExitStatus`, `RunError`.
- `src/main.rs` — the `run` and `demo` modes.
- `tests/shell_tests.rs` — 44 tests covering every line of `attack-list-2b.md`.
- `hand-test-2b.sh` — the hands-on harness (the gate). `bash shell/hand-test-2b.sh`.
- `attack-list-2b.md` (project root) — the spec, written **before** this code.

One dependency, by path, on this project's own verified resolver:

```toml
atrium-resolver = { path = "../resolver" }
```

No external crate. `std` only otherwise. The reason this is not the dependency
`AGENT-RULES.md` §5 guards is recorded in `DECISIONS.md`.

## Known limitations, recorded not hidden

- **§K / L.1 — symlinks are always followed** by the resolver, so a link whose
  target is outside cannot be addressed as an object at all. Inherited from
  Phases 1/2a; needs a non-following resolver variant, which is a stop-and-ask
  item with its own verification. Not this phase's to fix.
- **§N.19 — TOCTOU.** The cwd is resolved to a path, then used to start a
  process; the two are not the same atomic act. A concurrent writer could swap
  the directory in between. Same class as 2a recorded; belongs with a
  resolution-hardening phase.
- **§N.17 — non-UTF-8 arguments are not representable.** The API takes `String`,
  so the question is answered by the type rather than tested.
- **§N.22 — concurrency semantics are undecided.** Whether the output cap is
  per-run, and whether two concurrent runs are isolated, has no requirement yet.
- **§N.23 — a command can write anywhere in the root, including wherever the app
  keeps its own state.** Nothing separates sandbox content from app files. Needs
  deciding before anything durable lives in the root (Phase 2c is first exposed).

### Phase 2e — three limits the cage introduces, measured not guessed

- **A tool routed through `/etc/alternatives` does not work inside the cage.** The
  cage binds `/usr`, `/bin`, `/sbin`, `/lib`, `/lib64` read-only and nothing else,
  and on a Debian-family host the standard tool symlinks are routed through
  `/etc/alternatives`, which is therefore absent inside. Measured on the GitHub
  runner 26 Sep 2026: `awk` fails inside the cage there, while on a host where the
  link is relative (`awk -> gawk`, staying within `/usr`) it works. The v1 rule is
  "no host access", so this is the design meeting a real host rather than a bug —
  but a command relying on that indirection will fail, and `hand-test-2e.sh` says so
  out loud rather than skipping quietly. Worth a decision: bind `/etc/alternatives`
  read-only, or accept it.
- **A command killed by a signal is reported as an exit code, not a signal.** The
  cage program waits for the command and reports its status in shell encoding,
  normalising it: measured, `kill -9 $$` and `exit 137` both come back as `137`, so
  the distinction §C.6 exists to preserve is destroyed before atrium sees it. This
  applies **only** to a caged run; the uncaged runner still reports `signal 9`.
  `timed-out` is unaffected: it means this runner's own timer fired.
- **There is no resource limit.** `ulimit -u`, `-n` and `-v` inside are the host's
  own values and the run is in no cgroup of its own, so a command that forks or
  allocates without bound can exhaust the machine while staying inside its root.
  Measured and filed as issue #89; constraining it means cgroups, which is its own
  design.

The real path is **not** disclosed by `pwd` — that is what the cage is for, and 2b's
README asked for it. It IS still disclosed by `/proc/self/mountinfo`, which reports
the mount table including the root's host path and the backing device (measured;
`/proc/mounts` and `/proc/self/maps` do not). Filed with a real fix as issue #88's
follow-up and recorded in `docs/archive/attack-list-2e-blind.md`; closing it properly
means making the environment root the root of its own mount rather than a
subdirectory of one.
