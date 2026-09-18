# Delegation brief — Phase 2d: the filesystem watcher

> **Built in-session by the parent (the Hermes orchestrator session), 15 Sep 2026,
> not delegated — and this is a change of method with an observed reason.** Both
> 14 Sep build children died at the 900-second cap with no report (2a's child on an
> HTTP 429 quota wall, 2b's child on `exit_reason=timeout` after 44 API calls), and
> each cost a full parent salvage pass. Phase 2c was built in-session for the same
> reason and produced a verified crate with no salvage. The model cap was probed
> clear before deciding (HTTP 200 on a one-token `kimi-k3` request, 0.84 s), so the
> reason is time, not quota. **This is a change of method, not of standard:**
> nothing in the phase below is relayed from a child, because there is none.
>
> **The independent blind list is still dispatched to a subagent** (`§10`), because
> that check is worthless if it comes from the head that wrote the code
> (`HANDOFF.md` §4). It is a gate, not an extra.
>
> This file is the phase's specification of record, written **before** the code.
> The attack list (`attack-list-2d.md`) is the *test* of this phase and was written
> first. **It is the authority on behaviour. If it and this brief disagree, the
> attack list wins and the disagreement gets reported.**

---

## 1. What this phase is, in one paragraph

Watch the environment root and **report what changed inside it**. `DESIGN.md` §3.3
states the need in one sentence: because `run commands` executes real programs,
those programs change files *outside* Atrium's own file-operation layer, so the
explorer would go stale. A watcher on the environment root emits the resulting
changes. The `ran` effect covers the command and its output; **the watcher covers
what the command did.** Phase 2d produces the notifications; **Phase 3 gives them
somewhere to go** (`BUILD-PLAN.md` §2d: *"They have nowhere to go yet — Phase 3
gives them one."*).

The mental model is a **security camera on the environment**. It does not decide
anything, does not gate anything, and does not stop anything. Its only job is to
notice and report, accurately, and to be honest about what it cannot see.

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

## 4. The design source (`DESIGN.md` §3.3, verbatim, current text)

> It is **not** a reimplemented fake shell. Reimplementing coreutils, git, python
> and npm is years of work that would never be 1:1, and a fake shell could never
> run real builds — which the long-term goal requires.
>
> Consequence: commands change files outside Atrium's own file layer, so the
> explorer would go stale. A **filesystem watcher** on the environment root emits
> the resulting effects. The `ran` effect covers the command and its output; the
> watcher covers what the command did.

And from `DESIGN.md` §6.2, the effect this feeds:

> `ran` is the odd one — its real consequences are invisible, since a command can
> create, change and delete at once. It covers the execution and output; the
> filesystem watcher emits the rest.

And `DESIGN.md` §3.1, the boundary that constrains every path this tool prints:

> A **closed environment with its own filesystem**. One directory on the host is
> the environment root. Everything an agent sees lives inside it. Paths are
> translated: the agent asks for `/home/documents`, the app resolves it to
> `<root>/home/documents`. **The agent never learns the real path exists.**

## 5. The build plan (`BUILD-PLAN.md` §2d, verbatim, current)

> ## 2d. Filesystem watcher
>
> Watch the environment root for changes made outside the file-operation layer
> (i.e. by shell commands). Emit change notifications. They have nowhere to go yet
> — Phase 3 gives them one.

And from the Phase 2 verification block, the line this phase answers:

> - Run a shell command that creates a file. Confirm the watcher noticed.

## 6. Hard rules for this subsystem (`AGENT-RULES.md` §6–7, selected verbatim)

> **The sandbox path resolver is the only way to turn a virtual path into a real
> one.** No other code constructs a real path. Not for convenience, not for a
> special case, not temporarily.

**This phase is not an exception to that rule, and this is the sentence a later
session will misread.** The resolver turns a *virtual* path into a real one. The
watcher does the **opposite** direction: it is given a real root by the app (as
the resolver is) and reports what happened inside it. It constructs no real path
from agent input, and it is **not agent-facing**. The rule is untouched.

> **Never write code that reaches outside the environment root.** Host access does
> not exist in v1. If a task seems to require it, the task is wrong — stop and ask.

**This is the rule that does the most work in this phase**, and it is why §9.4
exists: a watcher that places a watch through a symlink reports changes made
*outside* the root, which is host activity leaking into the sandbox's view.

## 7. Stop and ask (`AGENT-RULES.md` §5, verbatim, selected)

> - The design document does not cover the case you have hit.
> - You are about to add a dependency.
> - You would need to weaken a gate rule, a permission, or a lock to make
>   something work.
> - You have tried the same fix twice and it has not held.

## 8. Crate layout (decided — do not re-open)

```
watcher/
  Cargo.toml            # package atrium-watcher, edition 2021, NO dependencies
  src/lib.rs            # the watcher: Watch::start, next changes, change records
  src/main.rs           # CLI: `watch` and `demo`
  tests/watcher_tests.rs
  hand-test-2d.sh       # the hands-on harness (the gate)
  README.md
  .hermes/environment.json
```

A fifth crate, separate like the other four, for the same reason: a signed-off
thing cannot gain unverified code (`STATUS.md`, "Four crates now").

**No dependency of any kind.** Not `notify`, not `inotify`, not `libc`, not
`walkdir`, not `tempfile`. See §9.1 — the mechanism chosen needs none.

## 9. Exact build spec

### 9.1 The mechanism, and why it adds no dependency

**Mechanism: Linux `inotify`, called through `extern "C"` declarations of the
libc symbols.** Rust already links libc on `x86_64-unknown-linux-gnu`, so
`inotify_init1`, `inotify_add_watch`, `inotify_rm_watch` and `read` are declared
in our own source and resolved at link time. **`Cargo.toml` gains nothing** — no
registry crate is fetched, so this is not the stop-and-ask dependency
`AGENT-RULES.md` §5 guards. This is recorded as its own decision in
`DECISIONS.md` with the alternatives, because a later session must not read it as
a dependency slipped in.

*The alternative rejected:* calling `syscall(SYS_inotify_init1, ...)` directly.
`std` exposes no raw syscall, and the `libc` crate is a registry dependency = a
stop-and-ask. Declaring the three libc wrappers we need is smaller than either.

*The other alternative, and why it is not the mechanism:* **polling** (walk the
root on a timer, compare). It needs no FFI and works on any filesystem, but it
cannot see a file that was created and deleted between two passes, and it cannot
report *which* change happened — only "something differs". This phase's whole
value is knowing what a command *did*. Polling is recorded in `DECISIONS.md` as
the rejected fallback and in the README as a known limitation, not implemented.

**Linux only.** `DESIGN.md` targets this machine; there is no cross-platform
requirement in v1. State it in the README rather than pretending otherwise.

### 9.2 What is watched, and how

- **One watch per directory**, placed by a recursive walk from the root at start.
  inotify is **not recursive** — observed 14 Sep 2026: a file created inside a
  subdirectory produced no event on the parent's watch. The walk is the only way
  to cover a tree.
- The walk uses **`symlink_metadata`**, never `metadata`. A directory reached
  through a symlink is **not** descended into. (`DESIGN.md` §3.1; the same rule
  2a and 2c hold.)
- Every `inotify_add_watch` is made with **`IN_DONT_FOLLOW`**, so a path that is a
  symlink to a directory does **not** get a watch placed through it. Observed 15
  Sep 2026, both halves: **without** the flag the kernel follows the link and
  places a watch on the *target* — a write inside the outside target then reported
  as an event (`CREATE created-after-watch.txt`), which is host activity leaking
  into the sandbox's view; **with** `IN_DONT_FOLLOW` on a link to a directory the
  call is refused.
- `IN_ONLYDIR` is also passed, so a race that replaces a directory with a file
  between the walk and the `add` fails the add rather than watching the wrong
  object.
- Mask per directory: `IN_CREATE | IN_DELETE | IN_MODIFY | IN_CLOSE_WRITE |
  IN_MOVED_FROM | IN_MOVED_TO | IN_ATTRIB | IN_DELETE_SELF | IN_MOVE_SELF`.
  **`IN_ACCESS` is deliberately NOT included** — a read is not a change, and
  taking a snapshot or listing a directory would otherwise flood the stream. Say
  so in the README (`DESIGN.md` §6.2's `read` effect comes from the file-operation
  layer, which knows it read something; the watcher cannot tell a read from a
  stat).

### 9.3 A `wd` is not a path — the mapping, and how it is kept right

`inotify_add_watch` returns a **watch descriptor** (`wd`), a small integer. Every
event arrives tagged with a `wd`, and a change to a file is reported as
`(wd, name)` — **the directory's descriptor plus the entry's name.** The path is
reconstructed as `path_of(wd) + "/" + name`.

`wd` is assigned **per inode**, and the kernel never tells you the path. Three
things follow, all **observed** 15 Sep 2026, and each is a way this goes silently
wrong:

1. **A rename re-keys nothing.** After `mv root/old root/new`, the events arrive
   as `MOVED_FROM old` + `MOVED_TO new` on the **parent's** wd, and a `MOVE_SELF`
   on the renamed directory's **own** wd. A write inside the renamed directory
   still arrives **on the old wd number**. So the `wd → path` table must be
   updated from the event stream: on `MOVED_FROM`/`MOVED_TO` sharing a cookie,
   re-key; on `MOVE_SELF`, update that wd's path.
   **Observed after the rename: `wd=2 CREATE name="after.txt"` — still wd 2.**
2. **Cookies pair a move's two halves, and the pair can be split across two
   different `wd`s.** `mv d1/f.txt d2/f.txt` produced `wd=1(d1) MOVED_FROM
   cookie=123063` and `wd=2(d2) MOVED_TO cookie=123063`. Pairing must be **by
   cookie**, not per-watch, or a cross-directory move is reported as two unrelated
   events. A move **out of** the root has a `MOVED_FROM` with no matching
   `MOVED_TO`; a move **in** has the `MOVED_TO` alone. Both are real and must be
   reported as they are.
3. **A watched directory can die.** If the root directory itself is replaced by a
   rename, the kernel sends `ATTRIB|ISDIR`, `DELETE_SELF`, `IGNORED` on that wd
   **and then nothing**: observed, a later write into the new directory produced
   **zero events**. The watcher is blind from that moment. `DELETE_SELF`/`IGNORED`
   must therefore be **reported**, and the directory marked dead — silence here
   would look exactly like "nothing happened".

`IN_IGNORED` is **always** sent when the kernel drops a watch, including after an
explicit `inotify_rm_watch` (observed). It is the only signal that a watch ended.

### 9.4 What must never be reported — the disclosure boundary

**A reported change must concern something inside the environment root, and its
path must be a virtual path.**

- The default output names **virtual** paths only — `/home/documents/x.txt`, never
  `<root>/home/documents/x.txt`. `DESIGN.md` §3.1: *the agent never learns the real
  path exists.* This is the same rule 2a and 2b hold, by the same re-wording
  pattern: **strip the canonical root prefix; never print the root.**
- A `--show-real` flag prints real paths, for the human reading the tool's output
  — the same exception `shell/` makes, and 2c's opposite-by-design case
  (`DECISIONS.md`). It is off by default.
- **Any event whose reconstructed path does not sit under the canonical root is
  reported as an error, never as a change.** With `IN_DONT_FOLLOW` and
  `symlink_metadata`, an event outside the root should be impossible; if one
  arrives, the correct behaviour is to say so loudly, because it means the
  watcher is watching something it was not asked to watch.

### 9.5 Overflow — the one case where events are *lost*, and what to do about it

**Kernel fact, observed 15 Sep 2026.** The queue holds
`/proc/sys/fs/inotify/max_queued_events` = **16384** events (this machine). When
it fills, the kernel **drops events** and delivers a single record with
`mask = IN_Q_OVERFLOW` and **`wd = -1`**.

Observed precisely: 300,000 file creations, never drained, produced **16385**
drained records — 8192 `CREATE`s, then the overflow record, then nothing. **Events
after that point are gone with no record of which.**

Two wrong designs to avoid:

- **Silently dropping the overflow record.** Then a watcher that missed 290,000
  changes reports the 8,192 it saw and looks complete. That is the failure this
  phase is most likely to be trusted for and least likely to survive.
- **Inventing a resync.** Guessing "probably these files" is inventing events.

**Required behaviour:** the overflow is **reported as its own record**, and every
consumer downstream can see that a gap exists. The watcher may also say *how many*
events it had already drained before the gap, and no more than that. A resync pass
is **not** built here; it is recorded as an open item (§11). This is the same
principle as `DESIGN.md` §6.5: *"Generic is vague but never lies"* — a gap is
reported as a gap.

### 9.6 Names are bytes, not strings

A filename on Linux is an arbitrary byte string that is not guaranteed to be valid
UTF-8. Observed 15 Sep 2026: a file named `bad-\xff\xfe-name.txt` produced
`CREATE`, `MODIFY`, `CLOSE_WRITE` with the name arriving as **those exact bytes**;
the lossy string is `bad-<replacement><replacement>-name.txt`.

So: **carry names as `OsString`/`Vec<u8>` from the kernel read to the point of
printing, and never convert lossily.** The CLI prints a non-UTF-8 name with the
offending bytes escaped (`\xFF`) rather than replaced, so the report is reversible
and no two distinct files can print identically. This is the same reasoning as
2c's record format (raw bytes, `namelen`-delimited), and it is a §J line in the
attack list.

### 9.7 Change records

The notification itself. Fields, fixed here because Phase 3 consumes them:

```rust
pub enum Change {
    Created { path: OsString, is_dir: bool },
    Modified { path: OsString },
    Deleted { path: OsString, is_dir: bool },
    MovedFrom { path: OsString, cookie: u32 },
    MovedTo { path: OsString, cookie: u32, is_dir: bool },
    Attributed { path: OsString },
    DirectoryGone { dir: OsString },    // DELETE_SELF / MOVE_SELF: a watch died
    Overflow { dropped_since: u64 },    // events were lost; a gap exists
    Error { detail: String },           // watched something outside the root, etc.
}
```

- `Modified` is emitted once per file **when the writer closes it**
  (`IN_CLOSE_WRITE`), not on every `IN_MODIFY`. Observed: three writes to one file
  produced `MODIFY, CLOSE_WRITE, MODIFY, CLOSE_WRITE, MODIFY, CLOSE_WRITE` — seven
  records for one logical edit. Reporting each `MODIFY` would triple the stream
  for no information; `CLOSE_WRITE` is the same signal the file-operation layer
  would emit. **`IN_MODIFY` is therefore consumed and not reported on its own.**
  State this in the README; it is a deliberate reduction, and §F of the attack
  list tests it.
- `MovedFrom`/`MovedTo` are kept as separate variants rather than one `Moved`,
  because the two halves can arrive with a `MOVED_FROM` whose partner never comes
  (moved out of the root), and a consumer must be able to tell that from a
  completed move.
- Every record carries **no timestamp field**. `std` cannot read a monotonic
  clock portably... it can, but Phase 3 owns record identity and ordering; adding
  a timestamp here would put a second source of truth for ordering in the project.
  The CLI may print elapsed milliseconds for the human. Recorded as deliberate.

### 9.8 The tree walk, and what it does when it cannot watch something

- A **directory it cannot watch is reported, not skipped in silence.** Observed:
  a directory with mode `000` refuses `inotify_add_watch` with `EACCES`; a
  directory whose permissions change later cannot be re-watched either. The
  watcher emits `Error` (or a named `Unwatchable`) naming the virtual path, and
  **keeps going** — one unreadable directory must not blind the whole root.
- A **dangling symlink** is not an error: observed, the walk adds no watch for it
  and reports no failure (it is not descended into, by design).
- A **newly created subdirectory** gets a watch added as soon as its `CREATE|ISDIR`
  event arrives. **There is an unavoidable race:** observed, a file written into a
  brand-new directory *before* the watch is attached produces **no event**. The
  window is one event-loop turn. It is recorded in the README and in §11 as a
  known limit; it is **not** papered over by inventing events for the new
  directory's contents.
- **Watch limits, observed:** `max_user_watches` = **300000**, `max_user_instances`
  = **2048** on this machine. Cost measured: **2461 directories → 2461 watches in
  4.82 ms**, so the walk is not a performance concern at realistic sizes. If a
  limit is hit, that is an `Error` naming the virtual path, not a silent gap.

### 9.9 What it deliberately does not do

- **It does not cage anything.** No bubblewrap, no namespaces, no Landlock. 2e.
- **It does not gate or block anything.** No rule, no permission, no kill switch.
  Phase 4.
- **It does not emit effects.** No `kind`, no `count`, no `label`, no SQLite, no
  text log — that is Phase 3 and `DESIGN.md` §6.3's field list. This phase emits
  **change notifications** and nothing else.
- **It cannot tell who made a change.** A change from the file-operation layer and
  a change from a shell command are indistinguishable at the kernel. `2d` reports
  **all** changes inside the root; de-duplicating them against the file layer's
  own effects is **Phase 3's job**. Stated in the README so nobody reads an
  authority into this phase that it does not have.
- **It does not watch outside the root**, and cannot be asked to.
- **It is not agent-facing** and is not routed through the resolver (it takes
  `--root`, a real host path, exactly as `snapshot/` does).

### 9.10 The CLI

```
atrium-watcher watch --root <dir> [--duration-ms N] [--show-real] [--settle-ms N]
atrium-watcher demo
```

- `watch` places the watches, then prints one line per change until
  `--duration-ms` elapses (default: run until interrupted). Exit 0 when it ran to
  its limit; **exit 1 if any `Error`/`Overflow` was reported** — a run that lost
  events is not a clean run, and the harness depends on that.
- `--settle-ms N` (default **150**) is how long after a change arrives it keeps
  draining before printing the batch. It exists because the kernel coalesces
  nothing (observed: 500 creates → 500 `CREATE` records, all reported), so a wait
  is not needed for completeness — it is there so a batch can be printed as a
  batch. **It must not be presented as a completeness mechanism; it is not.**
- Line format, one change per line, stable and greppable:

```
CREATE   /home/documents/notes.txt
MODIF    /home/documents/notes.txt
DELETE   /home/documents/old/
MOVEFROM /home/documents/a.txt cookie=123009
MOVETO   /home/documents/b.txt cookie=123009 dir
ATTRIB   /home/documents/sub/
GONE     /home/documents/sub/
OVERFLOW dropped-since=8192
ERROR    <plain-words reason>
```

- `demo` runs a self-contained scenario against a temp root and prints what it
  saw. Read-only outside its own temp directory.
- Refusals: one line, `REFUSE <why>`, exit code 1 — **naming the real path is
  permitted here, and only here**, because the person reading a refusal is the
  user, who needs to know which directory was refused (the 2c precedent, recorded
  in `DECISIONS.md`). A refusal never prints a path in the *change stream*.

### 9.11 The harness — `watcher/hand-test-2d.sh`

- `bash hand-test-2d.sh [root]`, default root `/tmp/atrium-2d-root`, outside dir
  `/tmp/atrium-2d-outside`.
- Builds the binary if missing or if any source is newer, and prints a build line
  when it does.
- Prints one line per check: `[section] what it did → observable`.
- **Independent checks that fail the run regardless of what the program printed**,
  the same three 2c used plus a fourth this phase needs:
  - `!! OUTSIDE TOUCHED` — outside directory's listing + sentinel checksum,
    captured before, re-checked after every line.
  - `!! HOST TOUCHED` — `/etc/passwd` checksum and mtime, same discipline.
  - `!! ROOT GONE` — the root still exists.
  - **`!! REAL PATH LEAKED` — no line of the default output may contain the
    root's real path.** This is the check that makes §9.4 load-bearing rather than
    aspirational, and it is checked as bytes, not by eye.
- Tally, then the success line **only when zero failures**, then `exit 0`;
  otherwise `N line(s) FAILED` and `exit 1`.
- **Prints two lines that are statements, not checks** — what this watcher cannot
  see — so the boundary is visible where the result is read (the 2c precedent).
- Removes its own throwaway directories at the end.

### 9.12 Tests — `watcher/tests/watcher_tests.rs`

In-process where possible, because some things cannot be expressed as CLI
arguments (a non-UTF-8 name, a rename pair's cookie, an overflow):

- The tree walk adds one watch per directory and **does not descend a symlink to a
  directory**; a symlink to an outside directory yields no watch and no event when
  the outside target is written to (the §9.4 boundary, as a test).
- A created file → `Created`; a written file → exactly one `Modified` at close;
  a deleted file → `Deleted`.
- A directory rename re-keys the `wd → path` table: a write inside the renamed
  directory afterwards reports the **new** virtual path.
- A cross-directory move pairs by cookie.
- A move out of the root reports `MovedFrom` with no partner.
- The root deleted → `DirectoryGone`, not silence.
- A non-UTF-8 name survives to the record as raw bytes.
- An unreadable subdirectory is reported and does not stop the other watches.
- Virtual-path mapping: a reported path never contains the root's real path.

### 9.13 One project file outside the crate

`watcher/.hermes/environment.json`, copied from `snapshot/.hermes/`'s pattern with
`name` changed to `atrium-watcher (CLI, no server)`. `start: null`, `port: null`,
`readinessPath: null` — a CLI with no server, which is what keeps `hermes verify`
honest (`HANDOFF.md` §5: `hermes verify` invents port 8000 otherwise).

## 10. The independent blind list (a gate, not an extra)

Dispelled before this phase is accepted, to a subagent that has seen **none** of:
`watcher/`'s source, this brief, `attack-list-2d.md`, any other attack list, or any
project file. It gets a **plain-words description of a filesystem watcher in a
sandboxed environment** and is asked what it would try. One tool call, no reads.
Its answer is preserved verbatim as `blind-attack-list-2d.md` and compared **by
mechanism, not by text**. Everything genuinely novel becomes a gate line in a new
section of `attack-list-2d.md`; **which head raised what stays visible.**

## 11. Open items this phase hands forward (written down, not built)

- The **create-then-write race** in a brand-new directory (§9.8). Closing it
  properly means watching the root with a mechanism that reports the child's
  contents, or accepting a resync pass — Phase 3's to weigh, since it owns
  re-reading `DESIGN.md` §8 for a re-scrape.
- **Resync after overflow** (§9.5). A deliberate, recorded decision to report the
  gap rather than guess.
- **Hard links** — observed 15 Sep 2026: a change made to an in-root inode
  **through a name outside the root** produced **no event at all** in the root.
  This is the 2a/§N hard-link channel appearing in the watcher: the root must be
  its own mount (2e) for it to be unreachable. Recorded, not fixed.
- **De-duplication against the file-operation layer** (§9.9). Phase 3.
- **`DESIGN.md` §7.3's watchdog** is a different thing entirely (stuck/looping
  agents). Not this.
- **inotify is Linux-only** (§9.1). Cross-platform is not a v1 requirement.

## 12. What NOT to do

- **Do not modify `resolver/`, `fileops/`, `shell/` or `snapshot/`.** All four are
  frozen and signed off; their source mtimes are the project's no-drift proof.
- **Do not edit `attack-list-2d.md`.** It is the test of this phase, written before
  the code. Report disagreements instead.
- **Do not add any dependency**, including `libc`, `notify`, `inotify`, `walkdir`,
  `tempfile`.
- **Do not follow a symlink**, anywhere, for any reason — not in the walk, not in
  `add_watch` (use `IN_DONT_FOLLOW`), not in a `metadata` call.
- **Do not report a change as if it were complete when events were lost.**
- **Do not invent a resync** after an overflow.
- **Do not report a real host path in the change stream.** `--show-real` only, and
  off by default.
- **Do not print a filename lossily.** Bytes in, bytes out.
- **Do not emit effects or write to SQLite** — Phase 3.
- **Do not build the cage (2e), the gate (Phase 4), the agent loop (Phase 5), any
  UI, or the effect stream (Phase 3).**

## 13. Required report shape

**Observed** (with the actual command output) · **Changed** (files and what
changed) · **Not done** · **Uncertain**. Never "fixed", "works" or "done" for
something not executed. Write the long form to a file
(`phase-2d-evidence.txt`) and keep the chat reply short.
