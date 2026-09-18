# Delegation brief — Phase 2c: snapshot and restore

> **Built in-session by the parent (the Hermes orchestrator session), 14 Sep 2026,
> not delegated.** Reason, observed: both recent build children died at the
> 900-second cap with no report (2a's child on an HTTP 429 quota wall, 2b's child
> on `exit_reason=timeout` after 44 API calls), and each cost a full parent
> salvage pass afterwards. This phase's hard parts are subtle behaviours — when a
> snapshot is *not* restorable, and what a symlink copy must not do — which is the
> worst thing to leave half-written behind a timeout. The model cap was probed
> clear before deciding (HTTP 200), so time, not quota, is the reason.
>
> **This file is still the phase's specification of record.** The convention that
> a brief exists on disk *before* its code is kept, because the ordering is the
> point: a brief written afterwards describes the code instead of fixing what the
> code must do.
>
> The attack list (`attack-list-2c.md`) is the *test* of this phase and was
> written first. **It is the authority on behaviour. If it and this brief
> disagree, the attack list wins and the disagreement gets reported.**

---

## 1. What this phase is, in one paragraph

Snapshot the environment root: make a complete, verified copy of it in a store
that lives **outside** the root. Restore from a snapshot: put the environment
back. This is the undo the design has been promising since `DESIGN.md` §3.4, and
it is the gate before anything runs on its own — *"Nothing autonomous runs before
undo exists."*

The mental model is a **save point**. And the detail that decides the whole shape
of this phase: **a save point the boss can delete is not a save point.** The
store must be outside the root, and putting it inside is a refusal, not a warning.

## 2. Who you are working for (`AGENT-RULES.md` §1, verbatim)

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

## 3. How to report (`AGENT-RULES.md` §3, verbatim)

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

## 4. The design source (`DESIGN.md` §3.4, verbatim, current text)

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
>
> **Effects sent to external apps are not recoverable.** A Figma frame deleted
> through MCP is gone. Destructive actions against external apps therefore go
> through an approval prompt instead, since there is no undo to fall back on.

**The copy-on-write branch is not being taken, and that is a recorded decision
with reasoning — `DECISIONS.md`, "Snapshots are a plain copy, not copy-on-write".**
The sentence above is conditional and names its own fallback ("otherwise a plain
copy"). The root's filesystem is 2e's decision and may be a tmpfs, which cannot
reflink at all (observed: `cp --reflink=always` on `/tmp` → `Operation not
supported`). `std` cannot issue the `FICLONE` ioctl either, so COW would mean a
registry dependency. **Do not implement COW. Do not add a dependency for it.**

## 5. The build plan (`BUILD-PLAN.md` §2c, verbatim, current)

> ## 2c. Snapshot and restore
>
> Snapshot the environment root. Copy-on-write if the filesystem supports it,
> otherwise a plain copy. Restore from a snapshot.
>
> Snapshot lands here, before anything autonomous touches files. **Nothing
> autonomous runs before undo exists.**

And from the Phase 2 verification block, the line this phase answers:

> - Snapshot, delete everything in the environment, restore, confirm it is back.

## 6. Hard rules for this subsystem (`AGENT-RULES.md` §6–7, selected verbatim)

> **The sandbox path resolver is the only way to turn a virtual path into a real
> one.** No other code constructs a real path. Not for convenience, not for a
> special case, not temporarily.

**This phase is not an exception to that rule, and this is the sentence a later
session will misread.** The resolver turns a *virtual* path into a real one. A
snapshot command has no virtual path: `--root` and `--store` are real host paths
supplied by the app, exactly as the app supplies the resolver's root. Nothing
here constructs a real path **from agent input**, which is what the rule forbids.
The tool is **not agent-facing** (attack list §H.1) and must never be reachable
from an agent tool call.

> **Never write code that reaches outside the environment root.** Host access does
> not exist in v1.

Read together with the above: this tool writes to the store, which is outside the
root — that is its entire purpose and it is the design's own requirement
(`DESIGN.md` §3.4, *"snapshots protect the environment root only"*). It reads
only the root it was given and writes only the store it was given, and **nothing
else, anywhere** — the outside-sentinel check is what enforces that.

> **If you want to do something `DECISIONS.md` rejected**, you may — but say
> explicitly which rejection you are overturning and what new evidence justifies
> it.

## 7. Stop and ask (`AGENT-RULES.md` §5, verbatim)

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

**This phase touches neither `resolver/`, `fileops/` nor `shell/`.** All three are
frozen: `resolver/` and `fileops/` are signed off hands-on, `shell/` was signed
off hands-on by Muffin on 14 Sep 2026. Not one byte in them may change.

## 8. Crate layout (decided — do not re-open)

```
snapshot/
  Cargo.toml          name = "atrium-snapshot", version 0.1.0, edition 2021
  src/lib.rs          the library
  src/main.rs         the CLI
  tests/snapshot_tests.rs
  hand-test-2c.sh     the harness (attack-list-2c.md, section by section)
  README.md
  .hermes/environment.json
```

**Zero dependencies.** Not even a path reference to `atrium-resolver` —
deliberately, and this is a *correction* to what the sibling crates do for one
specific reason: `DECISIONS.md`'s dependency entry says a new crate must not
depend on a *signed-off* crate, and `atrium-resolver` is exactly the dependency
this crate does not need. It resolves nothing: it copies a real directory and
writes to a real store. **If you find yourself wanting the resolver here, that is
the sign that agent input has crept into this tool — stop and say so.**

`src/lib.rs` is the whole implementation. Binary name `atrium-snapshot`.

## 9. Exact build spec

### 9.1 The two path types, and why this phase's differ from 2a's and 2b's

`2a` and `2b` both wrapped a *virtual* path in a constructor whose only
constructor called `resolve()`, so "forgot to resolve" was a compile error. **This
phase has no virtual paths**, so that pattern does not apply and must not be
imitated: wrapping a real host path in a type called `VirtualPath` would be a lie
in the type system.

What this phase needs instead is the *other* guarantee those types gave: that a
path which escaped the store is unrepresentable. So:

```rust
/// A snapshot's name. Guaranteed to be a single path component: no `/`,
/// no `..`, no `.`, not empty, no NUL. The only way to get one is through
/// `Label::parse`, which is what makes "a name used as a path" a compile
/// error on the write side.
pub struct Label(String);

impl Label {
    pub fn parse(raw: &str) -> Result<Label, SnapshotError>;
    pub fn as_str(&self) -> &str;
}
```

`Label::parse` is the single doorway, and it is where attack-list-2c.md §G.1–G.9
are answered. It must refuse, with a reason naming the specific problem:

- empty, `.`, `..`
- any `/` anywhere (this covers `../../x` and `/etc/evil`)
- any `\0`
- a name that is not valid UTF-8 cannot arrive through the CLI anyway (the API
  takes `&str`); state that, do not pretend it is tested (§G.4)
- **length**: refuse anything over 255 **bytes**, measured in bytes not
  characters, same rule as the resolver's `NameTooLong` (a 255-byte limit is the
  filesystem's; the label becomes a directory name)

Both `create` and `restore` go through it. Restore's `--id` **is** a `Label`.

### 9.2 The label is the snapshot's identity

No separate ID allocator. `--label base` creates `STORE/base/` (the tree copy)
and `STORE/base.snapshot` (the completed record). Restore takes `--id base`.
Duplicate label → **refused** (§B.9), never an overwrite: silently overwriting a
snapshot destroys an undo while reporting success.

### 9.3 The completed record — what makes a directory a snapshot

**A directory in the store is not a snapshot. A directory plus a completed record
is.** This is attack-list-2c.md §F, and it is the most important idea in the
phase.

`STORE/<label>/` — the copied tree.
`STORE/<label>.snapshot` — the record, written **last**, after the copy finished
and after the copy was verified against it.

`snapshot list` shows a snapshot **only** if both exist and the record ends with
its completion line. A directory with no record is what a create process that died
halfway leaves behind; it must not be listed, and restoring it must be refused.
**Do not tidy such directories away** — leave them visible (§F.4).

The record's format, byte-exact, because §J.5 (a newline in a filename) makes a
naive line-based format wrong and it is worth being precise:

```
ATRIUM-SNAPSHOT 1\n
ENTRY <kind> <mode> <size> <sha256hex> <namelen>\n
<namelen bytes of the relative path, raw, no escaping>\n
... one ENTRY block per entry, directories first then files/symlinks,
    in a deterministic order (sorted by path bytes) ...
END <count>\n
```

- `kind` is `file`, `dir` or `symlink`. For a symlink, `size` is the byte length
  of the target string and `sha256hex` is the hash of the **target string**, not
  of anything the link points at.
- `mode` is the octal permission bits (e.g. `755`, `644`, `444`).
- `namelen` makes the raw path bytes self-delimiting, so a newline, a non-UTF-8
  byte, or a space in a name changes nothing. Paths are stored **relative to the
  root** and are the *raw bytes* of the `OsStr` — never lossily converted to
  `String`, never escaped.
- The record is a snapshot **only** if the last line is `END <count>` and `count`
  matches the number of ENTRY blocks. Truncation is therefore detectable.
- **Modification times are deliberately absent** (§I.1). Do not add them: `std`
  cannot restore them, and a record that claims something the restore does not do
  is worse than one that is silent.

### 9.4 SHA-256, implemented here

`std` has no hash. The record's checksums are what make §F.2 (§F.3) work, and a
64-bit non-cryptographic hash would weaken a check that exists to notice damage.
So implement **SHA-256 properly** in `src/lib.rs`: FIPS 180-4, the standard
64-constant schedule, streaming over the file in chunks (never read a whole file
into memory — a snapshot must handle a file larger than RAM).

**Unit-test it against the published vectors** before trusting it, at minimum:
the empty string, `"abc"`, and the 56-byte and 1000000-`a` vectors. Also test the
streaming path against the one-shot path on a file larger than the chunk size, so
"chunked" is not merely asserted.

`sha2` is a registry crate and is **not** allowed here (§5: adding a dependency is
a stop-and-ask, and §8 says zero dependencies).

### 9.5 The copy

Iterative, with an explicit stack — **not** recursion. A deep tree must not
overflow the stack, and a hard limit on depth would be an arbitrary refusal the
attack list does not ask for.

For every entry, **`symlink_metadata`, never `metadata`**. That single choice is
the whole of §E. Concretely:

- **directory** → create the directory in the store, set its mode, descend.
- **regular file** → copy it by streaming (read/write in chunks), set its mode.
- **symlink** → `read_link` and re-create it with the *same target string*,
  byte-for-byte. **Never** `fs::metadata`, never `canonicalize`, never
  `read_to_string` through it, never dereference it in any way.
  A dangling symlink is a **success**, not an error (§E.4): the target does not
  exist and the link is still copied exactly.
- anything else (socket, fifo, device) → **refuse the whole snapshot** with a
  reason naming the path and what it is. Do not silently skip it; do not copy it
  "approximately". Nothing in v1 creates one, but a `.socket` in an environment
  is exactly the shape of a thing that gets dropped silently.
- **Hidden files and directories are ordinary entries.** A walk that skips
  dotfiles fails §D.1, where the environment is emptied down to the last hidden
  file. Do not special-case `.` or `..` into the walk in a way that filters real
  names.

Ordering for the record is deterministic (paths sorted by their raw bytes) so
that two snapshots of the same tree have byte-identical records — that property
is asserted, and it is what makes §A.11's "taking a snapshot changes nothing"
comparison meaningful.

**No temp files inside the root**, ever. The copy writes only into the store.

### 9.6 Restore

Order of operations, and the order is the safety property:

1. Resolve and validate `--root`, `--store`, `--id` — **refusals happen first,
   before anything is touched** (§C.9: a bad ID leaves the root untouched).
2. Read and validate the record: completion line present, count matches.
3. **Verify the snapshot against its own record** — every entry present with the
   right kind, mode, size and hash; **and no extra entries** in the snapshot
   directory that the record does not list. Any difference → refuse (§F.2, §F.3),
   root untouched, store untouched.
4. Only now: clear the root's **contents** — every child, recursively,
   symlink-aware (`remove_dir_all` only on a real directory; a symlink is removed
   with `remove_file`, never followed).
5. Copy the snapshot's tree back into the root.
6. **Verify the restored root against the record**, and report the result. If
   verification fails, say so loudly and exit non-zero. **A restore that cannot
   confirm itself has not succeeded**, and pretending otherwise is the exact
   failure this project is structured against.

**The root directory itself is never deleted or recreated** — only its contents
(§C.11: its inode must survive). Restore is idempotent (§C.12).

Cross-root restore (a snapshot taken from one root restored into a different root)
is **refused** (§C.8). Store the origin root's canonical path in the record at
create time; compare at restore. A `--force` flag is *not* required — record the
refusal and let a later phase decide the override.

### 9.7 Refusals, exit codes, and what a refusal means

- `0` success · `1` refusal (the thing was understood and declined) · `2` usage
  error (the arguments are wrong). Stated in `README.md` and tested (§J.8).
- **A refusal writes nothing anywhere.** Not the store, not the root, not the
  outside directory. The harness checks this by listing the store before and after
  (§B, `!! STORE CHANGED`).
- Every refusal **names the specific problem**, in the standard the earlier phases
  set: which directory is inside which, which label contains a separator, why the
  snapshot is not restorable. A bare `failed` is not acceptable (§G.9).
- **This tool *does* print real host paths in refusals**, unlike 2a and 2b, and
  that is deliberate — see the last paragraph of the `DECISIONS.md` entry on COW,
  and attack list §H.1. Do not "fix" it toward the sibling crates' behaviour.

### 9.8 Refusals required (each maps to a line in the attack list)

| Condition | Section |
|---|---|
| `--root` missing, not a directory, is a file, is `/`, is a symlink | §B.1–B.4 |
| `--store` inside `--root`, equal to `--root`, missing, is a file | §B.5–B.8 |
| duplicate label | §B.9 |
| missing/empty required flags | §B.10 |
| label or ID that is not a single name | §G.1–G.9 |
| snapshot not complete, or altered since creation | §F.1–F.3 |
| restoring a snapshot taken from a different root | §C.8 |
| unknown ID | §C.9 |

`--root /` deserves a sentence: it is the catastrophic typo, and the requirement
is that **the program** catches it, not that the user was careful. Refuse it
explicitly by name, before any other check, so it cannot be reached by a path that
happens to be short.

### 9.9 The CLI

```
atrium-snapshot snapshot create  --root <dir> --store <dir> --label <name>
atrium-snapshot snapshot list    --store <dir>
atrium-snapshot snapshot restore --root <dir> --store <dir> --id <name>
atrium-snapshot demo
atrium-snapshot fixtures --root <dir>
```

`fixtures` builds the attack list's tree by hand (`mkdir`, `printf`, `chmod`,
`ln`), including the symlinks, exactly as the harness needs them. `demo` runs a
short snapshot → damage → restore cycle and prints what it did, so a human can see
the whole idea in one command. Both exit 0 on success.

### 9.10 The harness — `snapshot/hand-test-2c.sh`

- `bash hand-test-2c.sh [root]`, defaults `/tmp/atrium-2c-root`; store
  `/tmp/atrium-2c-store`; outside `/tmp/atrium-2c-outside`.
- Builds the binary if missing; builds the fixtures via `fixtures --root`.
- Prints one line per check: `[section] what it did → observable`.
- **Three independent checks**, which fail the run regardless of what the program
  printed about itself — and they are the reason this harness is trustworthy:
  - `!! OUTSIDE TOUCHED` — outside directory's full listing + sentinel checksum,
    captured before the run, re-checked after **every** line.
  - `!! STORE CHANGED` — store listing captured before the run and after every
    REFUSED line.
  - `!! ROOT GONE` — the root directory must still exist after every line.
- Tally line, then the success line **only when zero failures**, then `exit 0`;
  otherwise `N line(s) FAILED`, `exit 1`.
- Ends by removing its own throwaway root, store and outside directory.

### 9.11 Tests — `snapshot/tests/snapshot_tests.rs`

Unit-level, `#[test]`, in-process (not through the CLI), because some things
cannot be expressed as arguments (§G.4) and some must be checked as bytes rather
than as printed text:

- SHA-256 published vectors; streaming vs one-shot.
- `Label::parse`: every refusal in §G, plus the 255/256-byte boundary.
- The record round-trips: write it, read it back, identical — including a path
  containing a newline and a non-UTF-8 byte.
- A damaged record (truncated, tampered count) is not a snapshot.
- Symlink copy: a link to a directory is copied as a link; a dangling link
  survives; a link pointing outside does not cause the target's contents to appear
  in the store.
- Modes: 755 and 444 survive a create/restore round trip.
- Empty directory survives. Empty file survives and is zero bytes.
- Restore removes a file that was not in the snapshot.
- The root's inode is unchanged by a restore.
- A socket/fifo inside the root refuses the snapshot with a named reason.
  (Create one with `std::os::unix::net::UnixListener::bind` in a temp dir.)

### 9.12 One project file outside the crate

`snapshot/.hermes/environment.json`, copied verbatim from `shell/.hermes/`'s,
with `name` changed to `atrium-snapshot (CLI, no server)` and an `evidence` line
saying so. `start: null`, `port: null`, `readinessPath: null` — it is a CLI with
no server, and this is what keeps `hermes verify` honest.

## 10. What NOT to do

- **Do not modify `resolver/`, `fileops/` or `shell/`.** All three are frozen and
  signed off. Their source files' modification times are the project's no-drift
  proof.
- **Do not edit `attack-list-2c.md`.** It is the test of this phase, written
  before the code. Report disagreements instead.
- **Do not implement copy-on-write.** §4.
- **Do not add any dependency**, including `libc`, `sha2`, `walkdir`, `tempfile`.
- **Do not follow a symlink**, anywhere, for any reason.
- **Do not use `metadata()`** where `symlink_metadata()` is meant.
- **Do not read a whole file into memory** to hash or copy it.
- **Do not restore a snapshot you could not verify first.**
- **Do not delete the root directory itself** during a restore.
- **Do not write temp files inside the environment root.**
- **Do not use `canonicalize()` on a path you are about to copy from** — it
  resolves symlinks, which is precisely the behaviour §E forbids. It is fine on
  `--root` and `--store` themselves, for the containment comparison, and nowhere
  else.
- **Do not preserve or claim to preserve modification times, ownership, ACLs,
  xattrs, sparse structure or hard-link identity.** §I lists them as deliberately
  not preserved; keep that list accurate rather than aspirational.
- **Do not put the store inside the root**, and do not warn-and-proceed if asked
  to: refuse.
- **Do not build the watcher (2d), any effect stream (Phase 3), any UI, or the
  cage (2e).**

## 11. Required report shape

**Observed** (with the actual command output) · **Changed** (files and what
changed) · **Not done** · **Uncertain**. Plus:

- the exact command Muffin should run by hand, and what he should expect to see;
- which of §I's non-preserved properties the implementation actually exhibits —
  measured, not asserted;
- anything the attack list demanded that you could not satisfy, quoted.

## 12. Tooling notes (real, from Phases 0, 1, 1b, 2a and 2b)

- `cargo build` is the lint arbiter. A standalone linter false-positives E0670 on
  edition 2021; ignore it, trust `cargo`.
- `cargo fmt --check` must be clean before the phase is reported.
- `hermes verify --json` in `snapshot/` must report `ok: true`.
- Long inline shell one-liners get hard-blocked; write a script to `/tmp` and run
  that.
- `find` on this host is a wrapper with reduced predicates — distrust it,
  cross-check with `ls`.
- The harness must fail when it should. **Prove it**: break one expectation, watch
  the `[FAIL]` line and the non-zero exit, then restore the file byte-exactly.
  A non-zero exit alone is **not** proof the intended failure fired — a syntax
  error also exits non-zero. This was observed in 2b and cost a wasted pass.
- Same for the escape detector: prove it fires by pointing it at something that
  does change, then restore.
- STATUS.md inserts can eat the next heading — grep heading counts after edits.
