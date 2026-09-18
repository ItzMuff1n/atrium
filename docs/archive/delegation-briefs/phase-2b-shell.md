# Delegation brief — Phase 2b: the real shell

**Status:** DISPATCHED 14 Sep 2026 as `deleg_3ca1b68b` (build child, leaf, kimi-k3),
with the independent blind second list (§13) dispatched separately as
`deleg_ade9c6f9`.

**Authority:** `BUILD-PLAN.md` PHASE 2, §2b and its dated correction. `DESIGN.md`
§3.3. `attack-list-2b.md` is the spec.

**If anything in the dispatch message conflicts with this file, this file wins.**
Phase 1 lost time to a hand-pasted spec drifting from the file; the fix is that
the child reads the file from disk and the file is authoritative.

---

## 1. What you are building (the whole job)

A new Rust crate, `shell/`, that **runs a real command** and gives back what it
did. One function, roughly:

```
run(root, cwd_virtual_path, program, args, options) -> Result<Outcome, RunError>
```

It starts the program with its **working directory** set to the real directory
that `cwd_virtual_path` resolves to inside the environment root, with a **fixed,
non-inherited environment**, input **not** connected to the user's terminal, a
**time limit**, and a **cap on captured output**; and it returns stdout, stderr,
the exit code (or the terminating signal), and whether output was truncated.

**The honest scope of this phase, which you must not overstate anywhere:**

> 2b guarantees **where a command starts** and **what it is handed** — the
> directory, the environment, its input, and that its output and exit code come
> back unaltered. It does **not** confine what the command can then reach. A real
> command can `cd /` and act on the host.

That is not a defect you should fix. It is `BUILD-PLAN.md` §2e, a required later
phase, and §2b's own dated correction says so. Your harness must **demonstrate**
that hole and mark it, never hide it.

**What this means you do NOT build:** no cage, no bubblewrap, no namespace, no
Landlock, no container. Those are §2e, with their own attack list. Building any
of them here is out of scope and would break the split the project relies on
(isolated verification — see `DECISIONS.md` "Strong confinement for `run
commands` is its own phase").

---

## 2. Who you are working for (`AGENT-RULES.md` §1, verbatim)

The person you are working with **does not read code and will not review it**.
This project is built entirely by AI agents.

That fact changes what your job is. You are not writing code for a reviewer to
check. **You are the only thing standing between a bug and production**, and the
compiler is the only other reviewer this codebase gets.

Consequences you must internalise:

- "It should work" is not a status. Run it.
- A test you wrote is weak evidence. It shares the assumptions of the code you
  wrote, including the wrong ones.
- Do not describe what you intended. Describe what you observed.
- If you did not run it, say you did not run it.

---

## 3. How to report (`AGENT-RULES.md` §3, verbatim — your output_schema applies it)

**Report what happened, not what you meant to happen.**

Every report separates:

- **Observed** — you ran it and saw this. Include the actual output.
- **Changed** — files you created or modified, and what changed in them.
- **Not done** — anything you skipped, stubbed, or left incomplete.
- **Uncertain** — anything you are guessing about.

Never write "fixed", "works", or "done" for something you did not execute.

When you hit an error, **paste the error verbatim**. Do not summarise it. Do not
paraphrase it. The user relays your output to another model that needs the exact
text.

If you claim a phase is complete, list exactly what the user should do to verify
it themselves. Do not verify it for them.

---

## 4. The design source (`DESIGN.md` §3.3, verbatim, current text)

> The terminal runs actual commands on the host, with its working directory
> confined to the environment root.
>
> **Correction, 14 Sep 2026 — "working directory confined" is not confinement.**
> This section says the command runs *on the host*, and that is exactly what
> happens: it runs as a real program with the user's own access. Confining the
> directory it starts in does not confine what it does. It can `cd /`; it can open
> an absolute path; it can do anything the user can. The boundary the rest of this
> design rests on — that nothing reaches outside the root — **does not exist for a
> real command**, and no path check can create it, because a path check looks at
> paths and a program is not limited to paths. (Observed in Phase 2a: a **hard
> link**, a second name for one file, let the sandbox read *and overwrite* a file
> outside the root. A command is a much larger version of the same gap.)
>
> **Strong confinement is therefore its own build phase — `2e`,** and it is a
> requirement, not an enhancement: commands must be unable to reach outside the
> root because, in the command's own view of the filesystem, outside the root does
> not exist. It gates Phase 5 — nothing autonomous exercises a shell that can reach
> the host. Mechanism (an OS-level cage; bubblewrap, user namespaces, or Landlock —
> all checked available on this machine 14 Sep 2026) is `BUILD-PLAN.md` 2e's
> decision; the *requirement* lives here. See also §8, where "run commands" is
> described as a different weight class — this is why.
>
> It is **not** a reimplemented fake shell. Reimplementing coreutils, git, python
> and npm is years of work that would never be 1:1, and a fake shell could never
> run real builds — which the long-term goal requires.
>
> Consequence: commands change files outside Atrium's own file layer, so the
> explorer would go stale. A **filesystem watcher** on the environment root emits
> the resulting effects. The `ran` effect covers the command and its output; the
> watcher covers what the command did.

**Also relevant, `DESIGN.md` §3.1 (verbatim, current):** paths are translated —
the agent asks for `/home/documents`, the app resolves it to
`<root>/home/documents`. **The agent never learns the real path exists.** Keep
this in mind for §H of the attack list: a command's *own output* is allowed to
contain the real path (you must not redact it), but **your** messages never may.

---

## 5. The build plan (`BUILD-PLAN.md` §2b, verbatim, current)

> ## 2b. The real shell
>
> Run actual commands with the working directory confined to the environment root.
> Capture stdout, stderr and exit code.
>
> > **Correction, 14 Sep 2026 — what "confined" does and does not mean.**
> > "Confined" here means the **starting directory** only. The command itself runs
> > as a real program with the user's own access, on the host. This section
> > previously read as though the sandbox held against a command; it does not, and
> > no path check can make it. Observed in 2a: a **hard link** inside the root let
> > the sandbox read *and overwrite* a file outside it, because a hard link is a
> > second name for one file, not a path — nothing about it looks wrong to a path
> > check. A real command is worse: it can `cd /` and act anywhere the user can.
> > **The cage is `2e`, below, and it is not optional.**

**The verification step that belongs to 2e, not to you:** "Try `cd /` and then a
destructive command. Confirm it cannot reach the host." You must **show that it
currently can** (§G of the attack list). You must not make it not happen.

---

## 6. Hard rules and paranoia for this subsystem (`AGENT-RULES.md` §6–7, verbatim)

### §6. Hard rules

These are not preferences.

**The sandbox path resolver is the only way to turn a virtual path into a real
one.** No other code constructs a real path. Not for convenience, not for a
special case, not temporarily.

**Never write code that reaches outside the environment root.** Host access does
not exist in v1. If a task seems to require it, the task is wrong — stop and ask.

**Never disable, bypass, or add an exception to a gate rule to make something
work.** If a rule blocks legitimate work, that is a finding to report, not an
obstacle to route around.

**Never auto-apply an effect classification.** Pending means pending.

**The effect vocabulary is defined in exactly one place in code.** Do not spell
effect names out elsewhere.

**No core file is edited to add a surface.** If adding a surface requires
touching the core, the plugin architecture is broken — report it.

**Do not add a permission, a state, or an effect kind without saying so
explicitly.** These are small fixed vocabularies by design.

### §7. Code that must be treated as dangerous

Two subsystems where "seems to work" is not good enough, and where you should be
slower and more paranoid than feels necessary:

**Path resolution (`DESIGN.md` §3.2).** Path traversal is a known bug class with
known tricks — symlinks, canonicalisation order, unicode separators, null bytes,
trailing dots. Resolve fully, then check. Never check, then resolve.

**The lock manager (`DESIGN.md` §10).** Concurrency bugs are invisible until they
are catastrophic, and they do not reproduce reliably. Prefer the boring, obvious
implementation over the clever one.

For both: **if you find yourself writing something clever, stop.** Clever is how
these fail.

### A third, for this phase — added by the parent agent, not from §7

**Process handling is the same class of dangerous as path resolution and
concurrency, and it is new here.** Deadlocks on two pipes, a child that is never
reaped, a signal reported as exit 0, a timeout that leaves the process running —
every one of these is invisible in a passing test and obvious in a hanging
machine. §F of the attack list is about exactly this. **Prefer the boring
implementation: spawn, read both streams on their own threads, poll for exit,
kill and reap on timeout.** Nothing clever.

---

## 7. Stop and ask (`AGENT-RULES.md` §5, verbatim — apply via your report)

**Stop and ask the user** when any of these are true:

- The design document does not cover the case you have hit.
- Following the design would produce something that clearly does not work.
- You need to change a decision recorded in `DECISIONS.md`.
- You are about to touch the sandbox path resolver (`DESIGN.md` §3.2) for any
  reason other than the phase that builds it.
- You are about to add a dependency.
- You would need to weaken a gate rule, a permission, or a lock to make
  something work.
- You have tried the same fix twice and it has not held.

**Asking is cheap. A wrong assumption in a codebase nobody reads is not.**

**You cannot ask mid-run. What you do instead:** make the safe choice, write it
in `what_is_uncertain` with the question stated plainly, and carry on. Do **not**
make an unsafe choice to keep moving.

---

## 8. Crate layout (decided — do not re-open)

```
shell/                        a new crate, sibling to resolver/ and fileops/
  Cargo.toml                  name = "atrium-shell", edition 2021
  src/lib.rs                  the runner, the outcome type, the error type
  src/main.rs                 a `run` mode and a `demo` mode (like fileops')
  tests/shell_tests.rs        cargo tests
  hand-test-2b.sh             the hands-on harness (§9.7)
  README.md                   what it does, and the boundary it does not have
  .hermes/environment.json    copied from resolver/, see §9.9
```

**Rationale, so you can make consistent decisions:** `resolver/` is signed off by
hand and must not gain unverified code. `fileops/` is signed off by hand
(14 Sep 2026) for a different job — file operations — and process execution is
not file operations. A separate crate keeps each verified thing separable, which
is why `fileops/` is separate from `resolver/` for the same reason.

**`fileops/` is signed off and must not be modified. Not one line.** If
something you need is in it, copy the idea, not the file, or report it.

**The one dependency, and why it is not the thing §5's rule guards.** `shell`
depends on `atrium-resolver` by **path**, exactly as `fileops` does:

```toml
[dependencies]
atrium-resolver = { path = "../resolver" }
```

You need it for one thing: turning the virtual cwd into a real directory. No
external crate is added; nothing is downloaded; no third-party code enters the
build. **Say it explicitly in your report** (`AGENT-RULES.md` §5 lists "about to
add a dependency" as a stop-and-ask item). **Do not add any other dependency.**
Everything in §9 — including the time limit, the output cap, reading two pipes at
once, and detecting a terminating signal — is achievable with `std` alone, and
must be. If you conclude it is not, **stop and report that**, do not reach for a
crate.

**You do not depend on `atrium-fileops`.** If you want a fixture on disk, create
it with `mkdir`/`printf` in your own harness and in your tests. A gate that
depends on another crate's binary still working is a gate with two failure modes.

---

## 9. Exact build spec

### 9.1 The path type — the same discipline as 2a, for the same reason

**A virtual path and a real path are different types and the compiler must not
let you mix them up.**

```rust
/// A directory as the agent wrote it. Always starts with "/".
pub struct VirtualPath(String);

/// A path that has been through the resolver and is known to be inside the
/// environment root. The only thing `spawn` may be given as a cwd.
pub struct RealDir(PathBuf);
```

`run()` takes `&VirtualPath` for the cwd — **never** a `&str` or `&PathBuf` —
resolves it through `atrium_resolver::resolve`, and only then has a `RealDir` to
start the process in. `RealDir`'s constructor is private to the crate.

This is the mechanism that makes `AGENT-RULES.md` §6 ("no other code constructs a
real path") true rather than aspirational: if the only way to get a `RealDir` is
through `resolve()`, then forgetting to resolve is a **compile error**.

**The program and its arguments are NOT paths and must not be resolved, checked,
or rewritten.** See §9.3 requirement 8. This is the one place this phase differs
from 2a and it is deliberate.

### 9.2 The runner

```
run(root, cwd_virtual, program, args, opts) -> Result<Outcome, RunError>

Outcome {
    stdout:  Vec<u8>,        // exact bytes, unaltered
    stderr:  Vec<u8>,        // exact bytes, unaltered
    status:  ExitStatus,     // exited(code) | signalled(n) | timed_out
    truncated: { stdout: bool, stderr: bool },
}

opts { timeout: Duration, max_output_bytes: usize }
```

`ExitStatus` must distinguish **three** things: an exit code, death by a signal,
and a timeout. **They are not the same and must not collapse into one number.**
A command killed by `SIGKILL` reported as exit 0 is the worst single outcome this
phase can produce (§C.6).

Pick a sane default `timeout` and `max_output_bytes`; make them settable. State
both values in your report, and put them in `README.md` — the user needs to know
what the limits are to judge the behaviour.

### 9.3 Behaviour requirements

Each numbered item is a requirement, not a suggestion. The attack list tests
every one; §9.7 names which.

1. **The cwd is resolved before anything is spawned.** A path that fails to
   resolve is `REFUSED` and **no process starts** — not "starts in the wrong
   place", not "starts and fails". §B.9 is checked on disk.
2. **The child's working directory is exactly the resolved directory.** Not the
   root, not the process's own cwd, not a guess. §A.1–A.7.
3. **The child does not inherit the parent's environment.** Start from an empty
   environment and set a small fixed set yourself. §D.1/§D.2. **This is a
   security requirement, not tidiness** — the user's shell environment on this
   machine holds API keys.
4. **`PATH` must be set and usable.** Decide it deliberately (a fixed default is
   fine); say what you chose. §D.3.
5. **`HOME` and `TMPDIR` point inside the environment root.** §D.4/§D.5. No new
   directory needs creating; the root itself is fine.
6. **The child's stdin is not the user's terminal.** §E.1–E.3. A command that
   reads stdin must see EOF immediately, not block on a person.
7. **stdout and stderr are captured separately and unaltered.** Bytes are bytes:
   no trimming, no newline-adding, no lossy UTF-8 conversion, no merging. A
   command that emits 100 MB is capped — and the cap is **reported**. §C.1–C.11,
   §F.3–F.6.
8. **The program and its arguments are passed through untouched.** No shell
   parsing, no splitting, no quoting, no resolving — `run` takes a program and an
   argv, and a line of shell is the caller's business (`sh -c '<line>'`). §C.10.
   Do **not** re-implement or second-guess the resolver's checks on the command's
   own arguments: a command touching a path that does not exist yet must work
   (§J.7). `AGENT-RULES.md` §6 is about *our* code constructing real paths; it is
   not a licence to inspect the arguments of somebody else's program.
9. **A command that never exits is stopped at the limit and reported as timed
   out.** §F.1. Never as exit 0, never left running.
10. **The child is always reaped.** After a normal exit, a timeout, or a signal:
    no zombies, no orphans. §F.2.
11. **Both pipes are read concurrently.** A command flooding stdout while stderr
    is full must not deadlock. §F.5 — this is the classic failure and it looks
    exactly like a hang.
12. **Every refusal names the virtual path and the specific thing that was
    wrong** — the `..` step, the symlink, the missing directory, the
    file-where-a-directory-was-expected. Not a bare "denied", not a raw OS error
    with no explanation. §B, §H.1/§H.3.
13. **The runner's own output names virtual paths, never resolved ones.** §H.1.
    If the command's own output contains the real path, pass it through
    **unaltered** — do not redact it (§H.4): redaction corrupts data and is the
    runner lying about what happened. The two rules are about different things.
14. **A debug flag may reveal real paths, off by default.** §H.5.
15. **`resolve()` is called; it is never reimplemented, bypassed or
    second-guessed.** If you find yourself doing string work on paths to decide
    containment, stop — that is the bug class this project exists to avoid.

### 9.4 What `resolve()` already handles, so you do not duplicate it

Read `resolver/src/lib.rs`. These are settled and **you must not re-check them**:

- `..` climbing above the root — rejected, naming the step.
- symlinks, including chained ones, pointing outside — rejected.
- a dangling symlink whose target is outside — rejected (Phase 1b).
- NUL bytes, relative paths, empty paths — rejected with their own reasons.
- a single component over 255 bytes — rejected, measured by the resolver.
- absent paths whose resolved location is inside the root — **accepted** (Phase
  1b).
- a path that exists but is a **file**, not a directory — this one is **yours to
  handle**: `resolve()` answers "is this inside the root", not "is this a
  directory". §B.3/§B.4 are the lines. Check it yourself, after resolving, and
  name it in the reason.

### 9.5 Known non-goals — document, do not attempt to solve

1. **The cage.** Not built here. `DESIGN.md` §3.3, `BUILD-PLAN.md` §2e. Your
   harness must **show the hole** (`attack-list-2b.md` §G) and mark it.
2. **Snapshots (2c) and the watcher (2d).** Not yours.
3. **The effect log** is Phase 3. A command ran; there is nowhere to report it.
   Do not invent one.
4. **The kill switch** (`DESIGN.md` §7.1) is Phase 4 and is user-facing. Your
   time limit is a safety stop so the runner cannot be hung — it is **not** the
   kill switch and must not be described as one (`attack-list-2b.md` §K.1).
5. **A terminal UI / PTY.** Not this phase. Capture is pipes; no interactive
   terminal, no `isatty`, no job control.

### 9.6 The error type

One enum in `src/lib.rs` with a `Display` that produces the sentences requirement
12 demands. It must:

- carry enough to name the specific failure (which virtual path, which step,
  which reason);
- wrap resolver failures rather than flattening them — and **re-word any resolver
  reason that names a real on-disk path**, exactly as `fileops/src/lib.rs` does
  (read it: `impl Display for FileOpError`, the `Rejected` arm, which matches
  `SymlinkEscapes`/`TraversalAboveRoot`/`EscapesRoot` and names the virtual path
  instead). The resolver's `Display` names real paths in those variants. This is
  a **copyable pattern, not a copyable file** — `fileops/` must not be modified;
- have **no variant that says "denied" without a reason**;
- and carry the **standing consequence** in a comment: if the resolver ever grows
  another variant whose reason names a real path, this re-wording must be updated.

### 9.7 The harness — `shell/hand-test-2b.sh`

This is what Muffin runs. It is the gate. Match the structure, output format and
exit codes of `fileops/hand-test-2a.sh` (read it) and `resolver/hand-test-1b.sh`.
It must:

- build a throwaway root under `/tmp` and a throwaway directory **outside** it;
  create every fixture in `attack-list-2b.md`'s Fixtures section itself, with
  `mkdir`/`printf`/`ln` — **not** by calling another crate's `fixtures` mode;
- record **before** the run: the full sorted listing of the outside directory,
  the checksum of its sentinel, and the checksum and mtime of the host
  `/etc/passwd`;
- run **every line** of the attack list. One line per check, printing the line,
  what was run, and what was observed;
- re-check the outside directory and the host `/etc/passwd` **after every line**.
  A change prints `!! OUTSIDE TOUCHED` and fails the run. **This check must not
  consult anything the runner says about itself** — it looks at the disk;
- **positively confirm** each `RUNS` line whose effect is a file: the file exists
  inside the root at its real location (§I.2). "Nothing escaped" alone is passed
  by a runner that does nothing;
- **confirm each `REFUSED` line by the absence of its marker**, inside the root
  and outside it (§I.3);
- compare exit codes against what the shell itself returns for the same command,
  run by the harness directly (§I.4) — not against a number typed from memory;
- print the §G `HOLE` lines **clearly marked as holes**, with a header saying in
  plain words that these are expected to reach the host and that they are 2e's.
  They must not be counted as failures, and they must not be quietly omitted;
- print a final count, and exit **0 only if every line behaved**;
- clean up both throwaway directories, deleting only the §G marker paths it
  created, and touching nothing else;
- set the environment canary (§D.1) **in its own environment** before invoking
  the runner, so a leaked environment is detectable by name.

**Prove the harness can fail.** Temporarily break one expectation, watch it print
`FAIL` and exit non-zero, revert. Say in your report which line you broke and
what it printed. A harness that has never failed is not evidence.

**Also prove the escape detector is not vacuous.** The harness's `!! OUTSIDE
TOUCHED` check must be shown to fire for a real escape — e.g. point one
throwaway line at the outside sentinel deliberately, watch it fire, remove it.
Say what you did and what it printed. For Phase 2a this check was taken on trust
at first and it turned out to need proving.

### 9.8 Tests — `shell/tests/shell_tests.rs`

`cargo test` must pass and must include coverage for every line of the attack
list. Follow Phase 1/1b/2a's standard, which is high:

- **Test the absent and boundary cases**, so a passing test can only pass for the
  right reason. Where a check could be satisfied by the operating system rather
  than your code, build the test so the OS cannot be what did it.
- **Assert the reason, not just the outcome**, for every `REFUSED` line.
- **§C.6 (signal vs exit code) and §F.1/§F.2 (timeout then reaped) are the two
  most important tests in the file.** Their failure mode is a crash or a hang
  being reported as success, and a machine with a runaway process on it.
- **§F.5 (both pipes flooded) must be in the suite**, and `cargo test` must not
  be able to hang forever silently.
- **§D.1 (the environment canary) must be a real test**, with the canary set in
  the test process's own environment.
- **§C.9 (all 256 byte values) must assert the exact bytes and the exact length.**
- Do **not** weaken a test to make it pass. If a line's required behaviour is
  impossible, that is a finding for your report.

### 9.9 Two project files outside your crate

The rule is that you touch nothing outside `shell/`. There are **two exceptions,
and only two**:

1. **`shell/.hermes/environment.json`** — copy it verbatim from
   `resolver/.hermes/environment.json` and change only the `recipe.name`. Without
   it, `hermes verify` invents a readiness check for a long-running server that
   does not exist and reports `ok=false` forever. Real, already-diagnosed trap
   (`HANDOFF.md` §5): every new CLI crate needs this file and every one has
   forgotten it once.
2. **A note appended to `STATUS.md`**, under the heading **"Agent-reported,
   unverified"** — *not* "Verified hands-on". Append only. Do not move, edit,
   reorder or delete any existing entry. Include: what you built, every command
   you ran with its real output, what Muffin should run by hand, and anything you
   left incomplete.

**Nothing else.** Not `DESIGN.md`, not `DECISIONS.md`, not `BUILD-PLAN.md`, not
`attack-list-2b.md`, not `attack-list-2a.md`, not `attack-list.md`, not
`attack-list-1b.md`, not `resolver/`, not `fileops/`. If you believe one of them
is wrong, say so in your report.

### 9.10 Tooling notes (real, from Phases 0, 1, 1b and 2a)

- Known false positive: the standalone file linter reports E0670 "async fn not
  permitted in Rust 2015" on edition-2021 files — it ignores `Cargo.toml`.
  **`cargo build` is the arbiter.**
- The project directory contains a space in its name (`Nexus project`); quote
  paths in shell commands.
- `find` on this host is a wrapper that does not support `-print` and has
  returned empty for files `ls` shows. Distrust it; cross-check with `ls`.
- **Run `cargo fmt` on your own files before you report.** Phase 2a's child left
  two files unformatted (44 hunks across whole files), and it had to be fixed
  afterwards. `cargo fmt --check` must be clean for `shell/`.
- A 255-byte filename is the maximum you can create.
- `hermes verify --json` in the crate is the project's formal check
  (**phases: build, test**). It must report `ok: true` before you report.
- Do not run `rm -rf` on anything but the throwaway directory your own script
  created, and check the variable is non-empty first.

---

## 10. What NOT to do

- No cage, no bubblewrap, no namespaces, no Landlock, no containers — that is 2e.
  Do not "helpfully" add confinement, and do not weaken §G to make the run look
  clean.
- No UI, no agent loop, no PTY or interactive terminal, no snapshots, no watcher,
  no gate, no kill switch, no locks, no mounts, no effect log, no config files,
  no "while I'm here" features.
- **Do not modify `resolver/` or `fileops/`.** Not one line, not to add a
  function, not to tidy.
- **No external dependencies.** Everything in §9 is `std`-only work. If you
  believe it is not, report that instead of reaching for a crate.
- Do not re-implement any check the resolver already performs (§9.4).
- Do not resolve, validate or rewrite the command's own arguments (§9.3 req. 8).
- Do not redact a command's output (§9.3 req. 13).
- Do not report a signal or a timeout as an exit code (§9.3 req. 1, §C.6, §F.1).
- Do not weaken a containment check, an expectation or a test to make a line
  pass. If a line cannot be satisfied, report it.
- No clever implementations. Boring and obvious, every time.

---

## 11. Required report shape (output_schema for the delegation call)

```json
{
  "what_was_built": "one paragraph: the crate, run(), how a virtual cwd becomes a real directory before anything is spawned, and the three-way status type",
  "files_changed": ["every file path created or modified, including the two exceptions in §9.9"],
  "commands_run_with_output": "verbatim transcript: cargo build, cargo test, cargo fmt --check, the demo run, hand-test-2b.sh, hermes verify --json, and BOTH deliberate proofs from §9.7 (broken expectation, and escape detector fired) — real output, unedited",
  "cwd_evidence": "the §A runs: actual pwd output and the actual on-disk locations of the files created, plus the §A.8 real-path observation pasted verbatim",
  "environment_evidence": "the §D runs: the canary's absence pasted verbatim, the child's actual `env` output, and where $HOME/$TMPDIR writes landed",
  "stream_evidence": "the §C runs: stdout/stderr bytes and exit codes, the signal case (§C.6), and the all-256-bytes case (§C.9) with its exact length",
  "limits_evidence": "the §F runs: the timeout reported as a timeout, proof the child is gone afterwards (the actual check), the truncation report, and the both-pipes-flooded case",
  "hole_evidence": "the §G runs with their real results, showing the host was reached — marked as a hole, not as a pass, and confirming the outside sentinel was untouched",
  "chosen_limits": "the default timeout and max_output_bytes you picked, and the PATH you chose, with one line of reasoning each",
  "attack_lines_not_satisfied": "any line of attack-list-2b.md you could not satisfy, with the line number and why. Empty if none — and if you write 'none', every line must genuinely have been run.",
  "what_was_not_done": "anything skipped, stubbed, or left incomplete",
  "what_is_uncertain": "anything you are guessing about, including any spec point you found ambiguous and the safe choice you made instead"
}
```

---

## 12. Dispatch parameters (for the parent agent, not the subagent)

- `delegate_task`, role: leaf.
- goal: "Build Phase 2b of Atrium per the brief below — verbatim."
- context: this entire file. The child reads it **from disk**
  (`delegation-briefs/phase-2b-shell.md`) rather than receiving it pasted. State
  explicitly that if the dispatch text appears to conflict with the file, **the
  file wins**. The child must also read `attack-list-2b.md` from disk — it is the
  spec — and `fileops/src/lib.rs` as a pattern for the re-wording (§9.6) and
  `fileops/hand-test-2a.sh` as a pattern for the harness (§9.7).
- Include the `skill_view(name='atrium')` instruction, as Phases 1, 1b and 2a did.
- output_schema: §11 above.
- One subsystem in the batch: this is the only task. The blind second list
  (§13) is a separate dispatch.
- After return, the parent independently re-runs build/test/fmt/demo/hand-test and
  its own spot attacks before reporting. Muffin then drives
  `attack-list-2b.md` by hand — that is the gate.

---

## 13. Independence requirement (for the parent agent)

As with Phases 1, 1b and 2a, `attack-list-2b.md` was written by the head that
wrote this brief and will verify the result, so the two share assumptions. Before
the phase is accepted, a **blind second list** (`blind-attack-list-2b.md`) must
come from a subagent that has seen **none** of: the `shell/` source, this brief,
`attack-list-2b.md`, `attack-list.md`, `attack-list-1b.md` or
`attack-list-2a.md`. It gets only a plain description of what running commands
must guarantee and is asked what it would try. Every novel line it produces is
run against the built code. The blind lists for Phases 1, 1b and 2a each found
lines the first list missed — for 2a the hard-link hole, which the first list had
missed entirely. This is a gate, not a courtesy.

---

## 14. Open items before dispatch — for the parent, not the subagent

1. **The 2b/2e split.** `attack-list-2b.md` §G deliberately demonstrates the hole
   and marks it. The requirement that closes it is 2e. If Muffin reads the split
   differently, this brief and the attack list change — the requirement does not.
2. **The path-dependency reading, now used a second time.** `fileops`'s brief
   (§14.2) said: if a path reference to our own verified crate is *not* what
   `AGENT-RULES.md` §5's dependency rule guards, that reading "should be
   corrected in `DECISIONS.md`, not left as a precedent in a brief". It is now
   about to be used twice, so it becomes a `DECISIONS.md` entry rather than a
   third brief precedent. Done as part of this dispatch.
3. **The time limit is a new kind of thing.** It is a safety stop, not the
   `DESIGN.md` §7.1 kill switch. Recorded in the attack list's §K.1 and in the
   crate's README; if it should be a numbered decision, say so.
4. **`PATH`, `HOME`, `TMPDIR` values** are the child's implementation call, made
   explicit in its report. Nothing in the design fixes them.
5. **The §A.8 real-path disclosure.** A command can print the sandbox's real on-
   disk location; under 2e it cannot. Not a 2b defect; recorded so it is not
   discovered later as a surprise.
