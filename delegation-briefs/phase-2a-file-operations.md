# Delegation brief — Phase 2a: file operations

**Dispatched:** 13 Sep 2026 — build and blind list dispatched together as one
batch, `deleg_f8c377a6` (task 0 = this phase's build, task 1 = the blind list
required by §13).
**Phase:** 2a of `BUILD-PLAN.md` (PHASE 2, "The environment").
**Builds on:** Phases 1 and 1b, both built and **signed off hands-on by Muffin
on 13 Sep 2026**. Do not change the resolver. Read `resolver/src/lib.rs` and
understand it before writing anything; treat it as fixed.

---

## 1. What you are building (the whole job)

**A new Rust crate that performs six file operations inside an Atrium
environment, all of them routed through the existing `atrium-resolver`.**

The six operations are: **create a directory, write a file, read a file, list a
directory, move, and delete**. That is the entire phase. Not seven, not a
config file, not a watcher, not an effect log.

The work is not "implement file operations" — that is `std::fs` and is easy.
The work is that **every path argument is resolved and contained before anything
touches the disk**, so that no operation can be aimed at anything outside the
environment root, and **nothing outside the root changes**.

Phase 1 built the part that decides whether a path is allowed. You are building
the part that *acts* on that decision. That is why this brief is long: the
mistake this phase can make is not a wrong answer, it is **deleting a real file
on the user's computer**.

Your complete behavioural specification is a **file on disk**:
`attack-list-2a.md`, in the project root. That file was written before this
brief and **before any of the code exists** — deliberately, so it tests what the
operations must do instead of describing what you built. Read it in full. It is
not optional and it is not background reading; §9 below is organised around it.

---

## 2. Who you are working for (AGENT-RULES.md §1, verbatim)

> The person you are working with **does not read code and will not review it**.
> This project is built entirely by AI agents.
>
> That fact changes what your job is. You are not writing code for a reviewer to
> check. **You are the only thing standing between a bug and production**, and the
> compiler is the only other reviewer this codebase gets.
>
> Consequences you must internalise:
>
> - "It should work" is not a status. Run it.
> - A test you wrote is weak evidence. It shares the assumptions of the code you
>   wrote, including the wrong ones.
> - Do not describe what you intended. Describe what you observed.
> - If you did not run it, say you did not run it.

---

## 3. How to report (AGENT-RULES.md §3, verbatim)

> **Report what happened, not what you meant to happen.**
>
> Every report separates:
>
> - **Observed** — you ran it and saw this. Include the actual output.
> - **Changed** — files you created or modified, and what changed in them.
> - **Not done** — anything you skipped, stubbed, or left incomplete.
> - **Uncertain** — anything you are guessing about.
>
> Never write "fixed", "works", or "done" for something you did not execute.
>
> When you hit an error, **paste the error verbatim**. Do not summarise it. Do not
> paraphrase it. The user relays your output to another model that needs the exact
> text.
>
> If you claim a phase is complete, list exactly what the user should do to verify
> it themselves. Do not verify it for them.

---

## 4. The design source (DESIGN.md §3.1–3.4, verbatim, current text)

> ### 3.1 Shape
>
> A **closed environment with its own filesystem**. One directory on the host is
> the environment root. Everything an agent sees lives inside it. Paths are
> translated: the agent asks for `/home/documents`, the app resolves it to
> `<root>/home/documents`. The agent never learns the real path exists.
>
> Specific host folders can be **mounted** into the environment at a chosen path
> (`/mnt/project` → a real directory). Same mechanic as a VM shared folder.
>
> **v1: host access is OFF entirely.** Environment root only. Not a denied
> request — the option does not exist.
>
> ### 3.2 Path resolution — the most important code in the project
>
> Every path an agent supplies is resolved and checked against the environment root
> **before anything runs**. Without this check, `../../..` walks straight out of
> the sandbox.
>
> **Clarification, 12 Sep 2026 — mounts do not widen the resolver.** This sentence
> previously read *"the allowed roots"*, plural, which read as though a path could
> be checked against more than one root; §3.1's mounted host folders reinforced
> that reading. Resolved by design decision (Muffin, 12 Sep 2026): **the resolver
> always takes exactly one root, and that is permanent.**
>
> When mounts arrive they get their own layer *above* the resolver. That layer maps
> a virtual path to a `(root, subpath)` pair and then calls `resolve()` unchanged.
> Mounts are real in the design but host access is off entirely in v1, so v1 has
> exactly one root and no mount table.
>
> Reason for keeping the signature narrow: `resolve()` is the one function where a
> mistake is dangerous. Widening it to accept a set of roots would mean
> re-verifying it — and its whole attack list — every time the mount logic changed.
> Keeping it single-root means the mount layer can be wrong without the sandbox
> being wrong.
>
> Phase 2 onward builds on the current signature, as built and verified.
>
> This is a known bug class (path traversal). It is the single place where a
> mistake is dangerous rather than annoying. It is built and verified first, alone,
> before any other subsystem exists. See `BUILD-PLAN.md` Phase 1.
>
> ### 3.3 The shell is real
>
> The terminal runs actual commands on the host, with its working directory
> confined to the environment root.
>
> It is **not** a reimplemented fake shell. Reimplementing coreutils, git, python
> and npm is years of work that would never be 1:1, and a fake shell could never
> run real builds — which the long-term goal requires.
>
> Consequence: commands change files outside Atrium's own file layer, so the
> explorer would go stale. A **filesystem watcher** on the environment root emits
> the resulting effects. The `ran` effect covers the command and its output; the
> watcher covers what the command did.
>
> ### 3.4 Snapshots
>
> The environment root is snapshotted **before every task**. Copy-on-write if the
> filesystem supports it, otherwise a plain copy.
>
> This makes nearly every filesystem failure an undo rather than a loss. It is the
> cheapest and most valuable safety mechanism in the design.
>
> Snapshots protect the environment root only. Once host access exists, changes
> out there are not covered — another reason host access stays off in v1.

---

## 5. The build plan (BUILD-PLAN.md PHASE 2, §2a and its verification, verbatim)

> # PHASE 2 — The environment
>
> **Rust only. Still no UI, still no agent.**
>
> ## 2a. File operations
>
> Create, read, write, move, delete, list — all routed through Phase 1's resolver.
>
> ## 2b. The real shell
>
> Run actual commands with the working directory confined to the environment root.
> Capture stdout, stderr and exit code.
>
> ## 2c. Snapshot and restore
>
> Snapshot the environment root. Copy-on-write if the filesystem supports it,
> otherwise a plain copy. Restore from a snapshot.
>
> Snapshot lands here, before anything autonomous touches files. **Nothing
> autonomous runs before undo exists.**
>
> ## 2d. Filesystem watcher
>
> Watch the environment root for changes made outside the file-operation layer
> (i.e. by shell commands). Emit change notifications. They have nowhere to go yet
> — Phase 3 gives them one.
>
> **Verification (user):**
>
> - Drive the file operations by hand. Try to escape. Fail.
> - Run shell commands. Confirm the working directory is confined.
> - Try `cd /` and then a destructive command. Confirm it cannot reach the host.
> - Snapshot, delete everything in the environment, restore, confirm it is back.
> - Run a shell command that creates a file. Confirm the watcher noticed.

**You are building 2a only.** 2b, 2c and 2d are named above only so you know
what you are not building. The verification section describes the whole phase;
your part of it is the first bullet.

---

## 6. Hard rules and paranoia for this subsystem (AGENT-RULES.md §6–7, verbatim)

> ## 6. Hard rules
>
> These are not preferences.
>
> **The sandbox path resolver is the only way to turn a virtual path into a real
> one.** No other code constructs a real path. Not for convenience, not for a
> special case, not temporarily.
>
> **Never write code that reaches outside the environment root.** Host access does
> not exist in v1. If a task seems to require it, the task is wrong — stop and ask.
>
> **Never disable, bypass, or add an exception to a gate rule to make something
> work.** If a rule blocks legitimate work, that is a finding to report, not an
> obstacle to route around.
>
> **Never auto-apply an effect classification.** Pending means pending.
>
> **The effect vocabulary is defined in exactly one place in code.** Do not spell
> effect names out elsewhere.
>
> **No core file is edited to add a surface.** If adding a surface requires
> touching the core, the plugin architecture is broken — report it.
>
> **Do not add a permission, a state, or an effect kind without saying so
> explicitly.** These are small fixed vocabularies by design.
>
> ## 7. Code that must be treated as dangerous
>
> Two subsystems where "seems to work" is not good enough, and where you should be
> slower and more paranoid than feels necessary:
>
> **Path resolution (`DESIGN.md` §3.2).** Path traversal is a known bug class with
> known tricks — symlinks, canonicalisation order, unicode separators, null bytes,
> trailing dots. Resolve fully, then check. Never check, then resolve.
>
> **The lock manager (`DESIGN.md` §10).** Concurrency bugs are invisible until they
> are catastrophic, and they do not reproduce reliably. Prefer the boring, obvious
> implementation over the clever one.
>
> For both: **if you find yourself writing something clever, stop.** Clever is how
> these fail.

---

## 7. Stop and ask (AGENT-RULES.md §5, verbatim — apply via your report)

> **Stop and ask the user** when any of these are true:
>
> - The design document does not cover the case you have hit.
> - Following the design would produce something that clearly does not work.
> - You need to change a decision recorded in `DECISIONS.md`.
> - You are about to touch the sandbox path resolver (`DESIGN.md` §3.2) for any
>   reason other than the phase that builds it.
> - You are about to add a dependency.
> - You would need to weaken a gate rule, a permission, or a lock to make
>   something work.
> - You have tried the same fix twice and it has not held.
>
> **Asking is cheap. A wrong assumption in a codebase nobody reads is not.**

You cannot ask a question mid-task. Instead: **do the safe, obvious thing,
complete everything else, and put the question in `what_is_uncertain` with
enough detail to answer it without re-reading your code.** Never guess silently
and never weaken a check to make something pass.

---

## 8. Crate layout (decided — do not re-open)

**A new crate. Do not modify `resolver/` in any way except the one exception in
§9.9.**

```
fileops/                       a new crate, sibling to resolver/
  Cargo.toml                   name = "atrium-fileops", edition 2021
  src/lib.rs                   the six operations, the error type, the path type
  src/main.rs                  a `run` mode and a `demo` mode (like resolver's)
  tests/fileops_tests.rs       cargo tests
  hand-test-2a.sh              the hands-on harness (see §9.8)
  README.md                    what it does, and the symlink limitation (§K)
  .hermes/environment.json     copied from resolver/, see §9.9
```

**Rationale, so you can make consistent decisions:** `resolver/` is the one
signed-off component in the project. Adding file operations to it would put new,
unverified code inside the verified crate and make `resolver/`'s
"signed off" status a lie about the whole crate. A sibling crate that *depends*
on it keeps the verified thing untouched and the new thing separable.

**This is the layout decision and it is not yours to revisit.** If you believe
it is wrong, do it anyway and say so in `what_is_uncertain`.

**The one dependency, and why it is not the thing §7's rule guards.** `fileops`
depends on `atrium-resolver` by **path**:

```toml
[dependencies]
atrium-resolver = { path = "../resolver" }
```

No external crate is added. No third-party code enters the build. Nothing is
downloaded. The dependency is on code already in this project, already verified
by hand, which is the entire point of the phase. **Do not add any other
dependency, and do not add this one without saying so explicitly in your report**
— `AGENT-RULES.md` §5 lists "about to add a dependency" as a stop-and-ask item,
and the honest position is that this is a path reference to our own verified
code rather than the risk that rule exists for. Stating it beats deciding it
silently. If `resolver/`'s `Cargo.toml` needs a `[lib]` section name that already
exists, use it as-is.

---

## 9. Exact build spec

### 9.1 The path type — the single most important decision in your code

**A real path and a virtual path are different types, and the compiler must not
let you mix them up.**

```rust
/// A path as the agent wrote it. Always starts with "/".
pub struct VirtualPath(String);

/// A path that has been through the resolver and is known to be inside the
/// environment root. The only type the operations accept.
pub struct RealPath(PathBuf);
```

**No operation may accept a `PathBuf` or a `&str` for a path argument.** Each
one takes `&VirtualPath`, resolves it through `atrium_resolver::resolve`, and
only then has a `RealPath` to act on. `RealPath`'s constructor is private to the
crate.

This is not decoration. It is the mechanism that makes "no other code constructs
a real path" (`AGENT-RULES.md` §6) true rather than aspirational: if the only way
to get a `RealPath` is through `resolve()`, then forgetting to resolve is a
**compile error**, not a bug that ships. A codebase where every path argument is
a bare `String` cannot enforce §6 at all.

### 9.2 The six operations

```
create_dir(virtual_path)                 -> creates it, and any missing parents
write_file(virtual_path, bytes)          -> refuses if the parent is missing
read_file(virtual_path)                  -> returns bytes
list_dir(virtual_path)                   -> returns entries, sorted by name
move_path(from, to)                      -> BOTH are resolved, both are checked
delete_path(virtual_path, recursive)     -> see 9.4
```

**Signature shape, for consistency:** each returns
`Result<T, FileOpError>`. Nothing panics on a bad path. Nothing unwraps.

### 9.3 Behaviour requirements

Each numbered item is a requirement, not a suggestion. The attack list tests
every one of them; §9.7 names which.

1. **Every path argument is resolved before any disk access.** All arguments,
   including a move's source *and* its destination. §D.1 is the line that exists
   because this is easy to half-do.
2. **A path that fails to resolve is refused, and nothing at all happens.** No
   partial effect, no empty file created as a side effect of the attempt.
3. **`write_file` does not create missing parent directories.** It refuses and
   the reason names the missing parent. (`create_dir` creates parents; `write_file`
   does not. Two deliberate behaviours — §F.2 and §F.3.)
4. **`create_dir` creates intermediate directories**, and is **not an error** if
   the directory already exists. §F.3, §F.6.
5. **`delete_path` takes an explicit `recursive` flag.**
   - Without it: refuses a directory that is not empty, and refuses the root.
     Removes an empty directory or a file. §E.3, §E.5, §E.6.
   - With it: removes the tree. §E.4, §E.1.
6. **A recursive walk never follows a symlink.** Links are removed *as links*.
   This is the single most important requirement in the phase: a recursive delete
   that follows links deletes the user's real files. §E.1, §E.2.
7. **The environment root itself is never removed or moved**, whatever flags are
   given. §E.6, §D.5.
8. **`list_dir` returns entries in a stable order, sorted by name**, and says for
   each whether it is a file, a directory, or a symlink. §G.4 — an unstable order
   cannot be verified by eye, which is how this project verifies things.
9. **Each entry in a failure names the specific thing that was wrong** — the `..`
   step, the symlink and its target, the missing parent, the non-empty directory,
   the directory-where-a-file-was-expected. Not a bare "denied", and not a raw
   operating-system code with no explanation. This is a requirement, not polish:
   Phase 1's gate is Muffin reading those reasons and judging them.
10. **Output names virtual paths, never resolved ones.** §H.4/H.5, and read the
    "known" note in the attack list's §H: the resolver's own CLI prints the real
    path on success, and you must deliberately not copy that. If you want the real
    path for your own debugging, put it behind a flag that is off by default.
11. **`resolve()` is called; it is never reimplemented, bypassed, or
    second-guessed.** If you find yourself doing string work on paths to decide
    containment, stop — that is the bug class this project exists to avoid.
12. **No path is ever constructed by joining a virtual path onto a string.**
    That is what `resolve()` is for.

### 9.4 What `resolve()` already handles, so you do not duplicate it

Read `resolver/src/lib.rs`. These are settled and **you must not re-check them**;
double-checking is how a second, weaker notion of containment gets into a
codebase:

- `..` climbing above the root — rejected, naming the step.
- symlinks, including chained ones, pointing outside — rejected.
- a dangling symlink whose target is outside — rejected (Phase 1b).
- NUL bytes, relative paths, empty paths — rejected with their own reasons.
- a single component over 255 bytes — rejected, measured by the resolver.
- absent paths whose resolved location is inside the root — **accepted** (Phase
  1b). This is what lets you create anything at all.

### 9.5 Known non-goals — document, do not attempt to solve

1. **The symlink-object limitation** (`attack-list-2a.md` §K, and read it: it is
   observed behaviour with the probe output shown). `resolve()` follows a
   symlink's final component, so `delete /inside-link` removes the *target* and
   leaves the link dangling. The fix needs a change to the verified resolver —
   which §7's stop-and-ask list forbids you to make. **Reproduce §K.1–K.3, record
   what you observed, and do not fix it.** Put it in your report. It is a known,
   deliberately-deferred defect, and a fix attempted quietly is worse than the
   defect.
2. **The shell, snapshots and the watcher** are 2b/2c/2d. Not yours.
3. **An effect log** is Phase 3. When this phase does something, there is
   nowhere to report it yet. Do not invent one.

### 9.6 The error type

One enum, in `src/lib.rs`, with a `Display` implementation that produces the
sentences requirement 9 demands. Shape it however is cleanest, but it must:

- carry enough to name the specific failure (which path, which step, which reason);
- wrap resolver failures rather than flattening them — a resolver rejection is
  passed through with its reason intact, not replaced by "denied";
- have **no variant that says "denied" without a reason**.

### 9.7 The harness — `fileops/hand-test-2a.sh`

This is what Muffin runs. It is the gate. Build it to the same shape as
`resolver/hand-test-1b.sh` (read that file — match its structure, its output
format and its exit codes), and it must do all of the following:

- Build a throwaway environment root under `/tmp`, and a throwaway directory
  **outside** it. Create the outside directory itself; it is the escape detector.
- Lay down the fixtures in `attack-list-2a.md`'s "Fixtures" section, including
  every symlink, and the `trap/` directory *containing* an outside-pointing link
  (`trap/escape`) — that one is the whole point of §E.1.
- **Record, before the run:** the full sorted listing of `<outside>`, the
  checksum of `<outside>/sentinel.txt`, and the checksum and modification time
  of the real host `/etc/passwd`.
- Run **every line** of the attack list. Print each line's operation, its virtual
  path argument(s), and its observed result. One line per check.
- **After every line, re-check** the outside listing and the sentinel checksum,
  and the host `/etc/passwd` checksum. A change prints `!! OUTSIDE TOUCHED` and
  fails the run. This check must not consult anything the program reports about
  itself — it looks at the disk. (§J.1, §J.2.)
- **Independently confirm** each MUST DO IT INSIDE line actually happened, at its
  real location inside the root (§J.5), and each MUST NOT EXIST line really is
  absent (§J.4). These are the mirror of the escape check: without them, a
  program that reports success and does nothing passes.
- Print a final count and exit **0 only if every line behaved**. Any refusal that
  cites a host permission error where the line required a sandbox-internal
  outcome is a **failure**, not a pass (§B.3, §B.5, §B.6).
- Clean up both throwaway directories. Never touch anything else.

**Prove the harness can fail.** Temporarily break one expectation, watch the
harness report it and exit non-zero, then revert. A harness that has never failed
is not evidence. Say in your report which line you broke and what it printed.

### 9.8 Tests — `fileops/tests/fileops_tests.rs`

`cargo test` must pass, and must include coverage for every line of the attack
list. Follow Phase 1/1b's standard, which is high:

- **Test the absent cases**, so a passing test can only pass for the right
  reason. Where a check could be satisfied by the operating system rather than
  your code, build the test so the OS cannot be what did it.
- **Assert the reason, not just the outcome.** A rejection test asserts the
  message names the specific thing, in the same spirit as Phase 1b's length
  tests.
- **For §E.1, assert the outside file still exists by checksum** after a
  recursive delete of the tree containing the link. This is the one test whose
  failure mode is destroying the user's data; treat it as the most important test
  in the file.
- **For §E.2, the test must fail by hanging** if links are followed — so keep it
  in the suite, and make sure `cargo test` cannot hang forever silently.
- Do **not** weaken a test to make it pass. If a line's required behaviour is
  impossible, that is a finding for your report.

### 9.9 Two project files outside your crate

The rule is that you touch nothing outside `fileops/`. There are **two
exceptions, and only two**:

1. **`fileops/.hermes/environment.json`** — copy it verbatim from
   `resolver/.hermes/environment.json` and change only the `recipe.name`. Without
   it, `hermes verify` in the new crate invents a readiness check for a
   long-running server that does not exist and reports `ok=false` forever. This
   is a real, already-diagnosed trap (HANDOFF §5): every new CLI crate in this
   project needs this file, and every one has forgotten it once.
2. **A note appended to `STATUS.md`**, under the heading
   **"Agent-reported, unverified"** — *not* "Verified hands-on". Append only.
   Do not move, edit, reorder or delete any existing entry. Include: what you
   built, every command you ran with its real output, what Muffin should run by
   hand, and anything you left incomplete.

**Nothing else.** Not `DESIGN.md`, not `DECISIONS.md`, not `BUILD-PLAN.md`, not
`attack-list-2a.md`, not `attack-list.md`, not `resolver/`. If you believe one of
them is wrong, say so in your report.

### 9.10 Tooling notes (real, from Phases 0, 1 and 1b)

- Known false positive: the standalone file linter reports E0670 "async fn not
  permitted in Rust 2015" on edition-2021 files — it ignores `Cargo.toml`.
  **`cargo build` is the arbiter.**
- The project directory contains a space in its name (`Nexus project`); quote
  paths in shell commands.
- `find` on this host is a wrapper that does not support `-print` and has
  returned empty for files `ls` shows. Distrust it; cross-check with `ls`.
- A 255-byte filename is the maximum. You cannot create a 256-byte one, so the
  length tests must use absent paths.

---

## 10. What NOT to do

- No UI, no agent loop, no shell, no snapshots, no watcher, no gate, no locks,
  no mounts, no effect log, no config files, no "while I'm here" features.
- **Do not modify `resolver/`.** Not to add a function, not to fix the symlink
  limitation, not to tidy anything. §K records it as deferred on purpose.
- No external dependencies.
- Do not re-implement any check the resolver already performs (§9.4).
- Do not weaken a containment check to make a line pass. If a line cannot be
  satisfied, report it.
- No clever implementations. Boring and obvious, every time.

---

## 11. Required report shape (output_schema for the delegation call)

```json
{
  "what_was_built": "one paragraph: the crate, the six operations, and how a path argument gets from VirtualPath to RealPath before any disk access",
  "files_changed": ["every file path created or modified, including the two exceptions in §9.9"],
  "commands_run_with_output": "verbatim transcript: cargo build, cargo test, the demo run, hand-test-2a.sh, and the deliberate harness-failure proof from §9.7 — real output, unedited",
  "outside_root_evidence": "the escape check's real results: the outside directory's listing and sentinel checksum before and after the full run, and the host /etc/passwd checksum before and after. Paste actual values.",
  "symlink_limitation_observed": "what you observed for §K.1, K.2 and K.3 — actual commands and actual results, not a description of the code",
  "recursive_delete_evidence": "the §E.1 and §E.2 runs, and the checksums proving the outside files survived a recursive delete of a tree containing a link to them",
  "attack_lines_not_satisfied": "any line of attack-list-2a.md you could not satisfy, with the line number and why. Empty if none — and if you write 'none', every line must genuinely have been run.",
  "what_was_not_done": "anything skipped, stubbed, or left incomplete",
  "what_is_uncertain": "anything you are guessing about, including any spec point you found ambiguous and the safe choice you made instead"
}
```

---

## 12. Dispatch parameters (for the parent agent, not the subagent)

- `delegate_task`, role: leaf.
- goal: "Build Phase 2a of Atrium per the brief below — verbatim."
- context: this entire file. The child reads it **from disk**
  (`delegation-briefs/phase-2a-file-operations.md`) rather than receiving it
  pasted, following Phase 1's 11 Sep deviation: a byte-exact read cannot drift,
  a hand-pasted spec silently builds the wrong thing. State explicitly that if
  the dispatch text appears to conflict with the file, **the file wins**. The
  child must also read `attack-list-2a.md` from disk — it is the spec.
- Include the `skill_view(name='atrium')` instruction, as Phases 1 and 1b did.
- output_schema: §11 above.
- One subsystem in the batch: this is the only task.
- After return, the parent independently re-runs build/test/demo/hand-test and
  its own spot attacks before reporting. Muffin then drives `attack-list-2a.md`
  by hand — that is the gate.

---

## 13. Independence requirement (for the parent agent)

As with Phases 1 and 1b, `attack-list-2a.md` was written by the head that wrote
this brief and will verify the result, so the two share assumptions. Before the
phase is accepted, a **blind second list** (`blind-attack-list-2a.md`) must come
from a subagent that has seen **none** of: `fileops/` source, this brief,
`attack-list-2a.md`, `attack-list.md` or `attack-list-1b.md`. It gets only a
plain description of what the sandbox must guarantee and is asked what file
operations it would try. Every novel line it produces is run against the built
code. Phase 1's and 1b's blind lists each found lines the first list missed both
times; this is a gate, not a courtesy.

---

## 14. Open items before dispatch — for the parent, not the subagent

1. **Muffin's go.** Nothing is dispatched until he says so.
2. **Path-dependency wording.** §8 declares the one dependency explicitly rather
   than leaving it implicit. `AGENT-RULES.md` §5 makes adding a dependency a
   stop-and-ask; the position taken here is that a **path** reference to our own
   verified crate is not what that rule guards. The child is told to say so in
   its report. If this reading is wrong, it should be corrected in
   `DECISIONS.md`, not left as a precedent in a brief.
3. **§K is a deferred defect, not a solved one.** It must appear in `STATUS.md`
   and eventually in `DECISIONS.md`, with the two options and their cost. It is
   recorded in the attack list; it is not yet a decision. Phase 2d and Phase 5
   both touch links, so this needs deciding before either.
4. **`DESIGN.md` §3.1 vs the resolver's real-path output.** §H.4/H.5 in the
   attack list require 2a's output to name virtual paths. That is a new
   requirement living only in an attack list so far. If it is right, it belongs
   in `DESIGN.md`.
