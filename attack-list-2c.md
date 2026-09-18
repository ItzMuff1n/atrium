# Attack list — Phase 2c: snapshot and restore

**Written 14 Sep 2026, before the code exists.** That ordering is the point: this
file says what snapshot and restore must do. Written afterwards it becomes a
description of the code instead of a test of it. Phases 1, 1b, 2a and 2b were all
built this way, and 2b's blind list found a real defect precisely because the
list existed independently of the code.

**Authority:** `BUILD-PLAN.md` §2c — *"Snapshot the environment root. Copy-on-write
if the filesystem supports it, otherwise a plain copy. Restore from a snapshot.
Snapshot lands here, before anything autonomous touches files. **Nothing
autonomous runs before undo exists.**"* Also `DESIGN.md` §3.4, `DESIGN.md` §3.1
(the environment root), and the plan's Phase 2 verification step *"Snapshot,
delete everything in the environment, restore, confirm it is back."*

**This is a working copy.** If it and `DESIGN.md` disagree, stop and say so — do
not pick one silently (`AGENT-RULES.md` §2).

---

## What is being tested, in plain words

Everything built so far can only *do* things. Phase 1 decides which paths are
allowed. Phase 2a acts on those paths. Phase 2b runs real programs that can do
anything at all inside the environment.

None of them can **undo**. If something is deleted, damaged or overwritten
inside the environment, it is gone. That is the state this phase ends.

The mental model is a **save point**, the way a game saves before a boss fight.
Before a task begins, you take a complete copy of the environment. If the task
goes badly, you put the copy back and the environment is exactly as it was.

One detail of that model decides the whole shape of this phase. **A save point
the boss can delete is not a save point.** If the copy lived inside the
environment, then the very thing you are protecting against — a command that
deletes things, or a program that damages the environment — would also be able
to delete the copy. So the copy must live **outside** the environment, and this
is why `--store` is a separate location and why putting the store inside the
root is a **refusal** rather than a warning (§B, §G).

**Why this phase is the gate before anything autonomous runs.** `BUILD-PLAN.md`
says it in one line: *"Nothing autonomous runs before undo exists."* The gate
before the agent loop is not the gate mechanism (Phase 4) and not the cage
(2e) — it is the ability to put the environment back. Everything after this
point assumes it can be undone.

**What 2c can therefore be verified to do, and what this list tests:**

1. **A snapshot captures the whole environment root, and nothing outside it** —
   every file, every directory, every symlink, every name, at every depth.
2. **Taking a snapshot changes nothing.** The environment is untouched by being
   photographed.
3. **Restore puts it back** — and the put-back is *exact* for the things that
   matter, which this list enumerates rather than assumes.
4. **A refusal refuses: nothing is written anywhere**, including the store.
5. **Symlinks are copied as symlinks and never followed** — this is the
   dangerous one, and §E is devoted to it.
6. **A snapshot that is incomplete or has been altered since it was taken is
   never restored.** A half-written save point is worse than none, because it
   looks like one.
7. **Nothing about the store's layout, an ID or a label can be used to write
   outside the store** — the same bug class as the path resolver, in a new place.
8. **What is deliberately *not* preserved is written down**, not discovered later
   (§I).

Points 6 and 8 are where this list adds requirements the plan does not literally
state. Reasoning, stated plainly so it can be overruled:

- **Point 6** — the plan's sentence is *"restore from a snapshot"*. A snapshot is
  a copy made over some period of time; if the process dies halfway, there is a
  directory that looks like a snapshot and is not one. Restoring it would delete
  the environment and put back half of it. So a snapshot is not "a directory" —
  it is **a directory plus a completed record**, and restore requires both
  (§F).
- **Point 8** — a copy mechanism that silently drops parts of what it copied is
  the exact failure mode this project is structured against: nothing looks wrong.
  The honest thing is to state the boundary in the artifact the user reads, so
  the boundary is a decision rather than a surprise.

---

## The vocabulary of a result

- **SNAPSHOT** — the command must run, and the stated observable must be true on
  disk afterwards: a file's checksum, a mode, a symlink's target, a count.
- **RESTORED** — the command must run, and the environment must afterwards match
  the stated observable exactly.
- **REFUSED** — the command must not run to completion and must write **nothing**
  anywhere — not into the root, not into the store, not outside either. Checked
  by comparing the store's full listing and a sentinel before and after; a
  refusal is only a pass if nothing appeared.
- **NOT PRESERVED** — a printed statement, not a gate line: a property this phase
  deliberately does not carry across a restore (§I). Printed so the boundary is
  visible in the run rather than only in a document.
- **NOT HERE** — belongs to another phase; listed so the gap is not silent.

Three independent checks run alongside the lines. Any one of them fails the run
regardless of what the program printed about itself:

- **`!! OUTSIDE TOUCHED`** — the harness keeps a real directory outside both the
  root and the store, holding a sentinel file, and records its full listing and
  the sentinel's checksum before the run. It re-checks after every line. Any
  change is an escape. This is the check that does not depend on the program
  telling the truth. It is also the check that proves §E: the root contains a
  symlink pointing into that outside directory, so if the copy mechanism follows
  links, the outside sentinel's *contents* end up inside the store and this check
  fires.
- **`!! STORE CHANGED`** — the store's listing is recorded before the run and
  after every REFUSED line. A refusal that quietly creates a snapshot directory
  is a failure.
- **`!! ROOT GONE`** — the root directory itself must still exist after every
  line. Restore replaces the root's *contents*; it must never remove the root.

A line reading `ok` and a line reading `FAIL` differ by one printed word, so a run
cannot be checked by eye for correctness — only for the absence of that word.
Reading the script is what establishes that the right lines ran. Same caveat as
Phases 1, 1b, 2a and 2b; stated again because it keeps being worth stating.

---

## Fixtures

The harness builds these itself, by hand, with `mkdir`, `printf`, `chmod` and
`ln` — it does **not** call another crate's fixture mode, so 2c's gate does not
depend on 2a's or 2b's binary still behaving.

Inside a throwaway root (`/tmp/atrium-2c-root` by default):

```
home/documents/notes.txt          ("hi")
home/documents/sub/               (empty dir)
home/documents/sub/deep.txt       ("deep")
home/work/                        (empty dir)
weird/with space.txt              ("spaces survive")
weird/café.txt                    ("unicode survives")
weird/mode-755.sh                 (chmod 755, "#!/bin/sh\necho ok\n")
weird/mode-444.txt                (chmod 444, "read only")
weird/empty-file.txt              (zero bytes)
deep/a/b/c/d/e/leaf.txt           ("bottom")
link-docs -> home/documents       (relative, points INSIDE the root)
link-escape -> <outside>          (absolute, points OUTSIDE the root)
link-broken -> nothing-here.txt   (dangling, target does not exist)
hard/one.txt, hard/two.txt        (hard link pair, same inode)
```

Outside both the root and the store (`/tmp/atrium-2c-outside`), created by the
harness:

```
sentinel.txt
sub/keep.txt
```

The store is a third throwaway directory (`/tmp/atrium-2c-store`).

**Why `link-escape` is pointed at a directory this harness owns and not at
`/etc`.** Pointing it at `/etc` would make the "followed a symlink" failure mode
*writable* against the real machine, and the check for it — "did the outside
sentinel's contents end up in the snapshot" — would be unobservable. Pointing it
at a harness-owned directory makes the failure mode both harmless and **visible**:
if the copy follows the link, `sub/keep.txt` appears inside the store. The same
mechanism that would have copied `/etc/passwd` is what this line catches.

---

## A. Capture — the whole root, and nothing else

A.1 `snapshot create --root ROOT --store STORE --label base` — SNAPSHOT: exits 0
and a snapshot is visible in `snapshot list --store STORE`.

A.2 The store now contains **exactly one** snapshot, and its listed ID is the one
`create` reported.

A.3 **The snapshot's file count equals the root's file count**, measured
independently by the harness before the run (`find`-equivalent walk, counting
files, directories and symlinks separately). A copy that silently skipped a
subtree fails here.

A.4 Every fixture file's content inside the snapshot is **byte-identical** to the
root's, checked by checksum, file by file — not by size.

A.5 The deepest file (`deep/a/b/c/d/e/leaf.txt`) is present at the same relative
depth. Depth is not flattened or truncated.

A.6 Both empty directories (`home/documents/sub/`, `home/work/`) are present as
**directories** in the snapshot. A copy that only records files loses them.

A.7 `weird/mode-755.sh` is mode 755 and `weird/mode-444.txt` is mode 444 **inside
the snapshot**, and both are still readable/copyable afterwards. A copy that
loses the executable bit breaks every build tool that needs it.

A.8 `weird/empty-file.txt` is present and is **zero bytes** — present and empty is
different from absent.

A.9 The unicode and space names are present **with the same bytes in the name**,
not renamed, escaped or stripped.

A.10 `link-broken` is present and is still a symlink whose target is
`nothing-here.txt` — a dangling link survives as a dangling link.

A.11 **Taking a snapshot changes nothing in the root.** The root's full tree
listing and every file's checksum, captured before and after the run, are
identical. Photographs do not touch the subject.

A.12 The store is **not** inside the root, and the snapshot contains **no path
beginning with the root's own name** — the copy is of the root's *contents*, so
the tree shape matches, not the path.

---

## B. Refusals — nothing is written at all

Every line in this section also checks `!! STORE CHANGED` and
`!! OUTSIDE TOUCHED`. A refusal that creates a directory, or touches the host, is
a failure regardless of the exit code.

B.1 `--root` does not exist — REFUSED, and the store gained nothing.
B.2 `--root` is a plain file, not a directory — REFUSED.
B.3 `--root /` — REFUSED. The catastrophic typo must be caught by the program,
not by luck.
B.4 `--root` is a **symlink** to a real directory — REFUSED. What is being
snapshotted must be the directory the user means, not whatever it points at now.
B.5 `--store` is **inside** `--root` — REFUSED, with the reason naming which
directory is inside which (§G).
B.6 `--store` **is** `--root` — REFUSED.
B.7 `--store` does not exist — REFUSED. The store is never created silently: a
mistyped store path must fail, not become a new empty store that then "succeeds".
B.8 `--store` is a plain file — REFUSED.
B.9 Duplicate label — SNAPSHOT the first time, then creating a second snapshot
with the **same label** is REFUSED. Silently overwriting an existing snapshot
destroys the undo while reporting success.
B.10 Missing `--root` or `--store` — REFUSED as a usage error, writes nothing.

---

## C. Restore — exactly back

C.1 Modify `home/documents/notes.txt`, then restore — RESTORED: the checksum
matches the snapshot's, not the modified content's.

C.2 Delete a file, then restore — RESTORED: the file is back with the same
content.

C.3 Create a file that was **not** in the snapshot, then restore — RESTORED: the
extra file is **gone**. A restore that only adds things back is not a restore.

C.4 Delete an entire directory, then restore — RESTORED: the directory and its
contents are back, including its empty subdirectory.

C.5 Change a file's mode from 444 to 666, then restore — RESTORED: the mode is
444 again. A restore that brings back content but not mode is a partial restore.

C.6 Replace `link-docs` with a real directory containing a file, then restore —
RESTORED: it is a symlink again, pointing at `home/documents`.

C.7 Restore reports the snapshot ID it used, and it is the ID requested.

C.8 **Restore into a different root than the snapshot was taken from** — REFUSED
unless explicitly forced. Restoring one environment's contents into another
environment's directory is a plausible disaster and not a silent default.

C.9 `--id` that does not exist — REFUSED, and the root is **untouched**: the
refusal happens before anything is deleted. This is checked by comparing the
root's tree before and after.

C.10 `--id` containing `..` or `/` — REFUSED (§G).

C.11 **The root directory itself survives a restore**: its own inode (the
directory's identity on disk) is the same before and after. The contents are
replaced; the environment itself is not deleted and recreated. If anything else
on the host holds a reference to that directory, it keeps working.

C.12 Restore is **idempotent**: restoring the same snapshot twice leaves the same
tree, and the second restore reports success rather than failing on
already-correct content.

---

## D. The plan's own verification step

This section is `BUILD-PLAN.md`'s Phase 2 line, performed by the harness so that
the gate command and the plan say the same thing. Muffin's own pass should
reproduce it by hand; this is the harness's version of it.

D.1 Record the environment's full tree checksum. Take a snapshot. **Delete
everything in the environment** — every file and directory in the root,
including the hidden ones. Confirm the root is now empty (and that the snapshot
**still exists**: it lives outside, so deleting the environment must not touch
it). Restore. Confirm the tree checksum **matches the recorded one exactly**.

D.2 After D.1's restore, the snapshot is still listed and still restorable a
second time — the restore did not consume or corrupt its own save point.

D.3 The `!! OUTSIDE TOUCHED` check holds across D.1 — deleting the environment
touches the environment, and nothing else.

---

## E. Symlinks are copied, never followed

This section exists because a naive recursive copy — including the obvious
one-liner in most languages — **follows** symlinks. Following them has two
consequences: the sandbox's link structure is destroyed (a link becomes a copy of
its target), and a link pointing outside the root causes host data to be read and
written into the store. Both are silent.

E.1 `link-escape` is present **in the snapshot** as a symlink whose target string
is **exactly** the outside directory's path — byte-for-byte, not resolved.

E.2 **The outside directory's contents are not in the snapshot.** Specifically:
`sub/keep.txt` does not exist anywhere under the store, and the outside
sentinel's checksum appears nowhere under the store. This is the line that fires
if the copy followed the link.

E.3 After a restore, `link-escape` is still a symlink with the same target string
— the restore did not follow it either.

E.4 `link-broken` survives both directions as a dangling symlink. Neither create
nor restore errors out on it.

E.5 `link-docs` is still a symlink after restore, and reading *through* it still
lands on `home/documents/notes.txt` with the right content — the link was
preserved **and** still works.

E.6 A symlink to a directory is copied as a link, not as a directory: the
snapshot's entry for `link-docs` is a symlink, and the snapshot does **not**
contain a second full copy of `home/documents` under the link's name.

---

## F. A snapshot that is incomplete or altered is never restorable

F.1 A directory in the store **with no completed record** is not a snapshot:
`list` does not show it, and restoring by that ID is REFUSED. (The harness
constructs this state directly — a directory and nothing else — because that is
exactly what a create process that died halfway leaves behind.)

F.2 A snapshot whose contents were **altered after it was taken** (harness
modifies one file inside the store) is REFUSED on restore, and the root is
**untouched**. This is the line that proves the completed record is load-bearing
rather than decorative: if it were only checked for existence, this line would
pass a restore of damaged data over a working environment.

F.3 A snapshot with a file **deleted** from it after creation is REFUSED on
restore, and the root is untouched.

F.4 A refusal in this section deletes nothing from the store: the damaged
snapshot is left visible so it can be investigated rather than silently tidied
away.

**Added from the blind list's mechanisms (§N.2a–c).** These are the three store-side
states the original list missed, and all three were probed before being written down.

F.12 A **symlink sitting at the snapshot's own name** in the store, placed there
before `create` runs: `create` must REFUSE rather than write through it. Checked
both ways — nothing appears at the link's target, and the store listing is
unchanged.

F.13 The snapshot's tree **replaced wholesale by a symlink to a decoy directory**,
with the record left intact. This is the sharpest form: the record is genuine and
the symlink makes `is_dir()` succeed, so a check that only confirms the tree exists
would restore *someone else's files*. REFUSED, the reason naming a file from the
decoy that the record does not list, and the decoy directory is never copied into
the environment.

F.14 An **extra file** inside the snapshot's tree, added after it was taken.
REFUSED, naming the extra file. §F.2 covers an altered file and §F.3 a missing one;
an added one is how a decoy or a smuggled payload arrives.

---

## G. The store, the IDs and the labels — no path escapes

This is the same bug class as the path resolver, in a new place: a **name** that
was assumed to be a name turns out to be a path. Every one of these must be
REFUSED, and for each, the check that matters is that **nothing was created
outside the store**.

G.1 `--label ../../outside-evil` — REFUSED.
G.2 `--label /etc/evil` — REFUSED (an absolute label).
G.3 `--label .` and `--label ..` — REFUSED.
G.4 `--label` containing a NUL byte — not representable on a command line, and
refused by the API's type; recorded, covered by in-process tests
(`attack-list-2a.md` records the same limitation — argv is NUL-terminated).
G.5 `--id ../../outside-evil` on restore — REFUSED.
G.6 `--id` with a leading `/` — REFUSED.
G.7 A label that is a **valid name but an existing file** in the store — REFUSED,
not overwritten.
G.8 After every line in this section, the **store's listing is unchanged** and
nothing exists at the outside paths the labels were aiming at.
G.9 The reason given for a refusal in this section **names the specific problem**
(the label contains a path separator / the ID escapes the store), matching the
standard Phases 1, 1b, 2a and 2b set. A bare "failed" is not acceptable.

---

## H. What this tool is, and what it is not

Printed by the harness, plainly, so these cannot be mistaken for gate results.

H.1 **This tool is not reachable by the agent.** It is the app's own undo
machinery, driven by the app and by the user — the same class as the gate. The
agent never calls it. That is why it takes a real host path (`--root`) and is
**not** routed through the path resolver: the resolver turns *virtual* paths into
real ones, and there is no virtual path here. `AGENT-RULES.md` §6's rule — the
resolver is the only way to turn a virtual path into a real one — is not broken
by this, and this line is here so a later session does not have to re-argue it.
*(This closes open item §N.23 from `attack-list-2b.md`: the app's own state must
not live inside the root, and 2c is the first phase exposed to that. The store is
outside by requirement, §B.5.)*

H.2 **A snapshot does not protect the store.** A shell command under 2b can reach
anything the user can reach, including the store and the outside directory.
Snapshotting is not a cage. Closing that is 2e, and the honest statement of it
belongs in this run rather than in a footnote.

H.3 **Snapshots are not automatic yet.** §2c delivers the mechanism; "before
every task" is the caller's job, and the caller (the agent loop) does not exist
until Phase 5. Recorded so an empty store is not read as a broken feature.

H.4 **A snapshot is not an integrity guarantee against a hostile writer.** The
completed record detects alteration, but it is not cryptographic and the store is
inside the user's own account. Against accidents and agent mistakes — the things
this phase is for — it holds. Against an attacker with the same account, nothing
here claims to.

---

## I. What is deliberately not preserved

Printed, not gate lines. These are decisions, not oversights, and they are
printed in the run so the boundary is visible where the result is read.

I.1 **File modification times are not restored.** `std` cannot set a timestamp on
a file; doing so needs a crate or an external program, and this project's standing
rule is `std`-only plus path references to its own crates. Consequence: after a
restore, files look freshly written. Contents, names, structure, symlinks and
modes are preserved; the clock is not. A later phase may revisit this with a
deliberate, recorded dependency.

I.2 **Hard links are restored as separate files with identical contents.** The
data survives; the fact that two names shared one file does not. Consequence: a
build tool that relies on inode identity to detect "unchanged" will redo work
after a restore. (The *escape* channel that hard links represent is a different
problem and is `DESIGN.md` §3.1's — the root being its own mount point.)

I.3 **Extended attributes, ACLs and sparse-file structure are not preserved.**
Nothing in v1 depends on them. Recorded so a later phase that does (for example,
a file with a capability set) knows to extend this list rather than assume.

I.4 **Ownership is not restored.** Every file is owned by whoever ran the
restore. Within one user's own environment this is invisible; across users it
would not be.

I.5 **Directory modification times are not preserved** (same reason as I.1).

---

## J. The boring half — ordinary use must simply work

J.1 Snapshot a root with a single file in it. Restore it. The file is there. The
simplest case works before the clever cases are attempted.

J.2 Snapshot twice with different labels, restore the **first** one after
modifying things — the older snapshot is intact and restores its own state, not
the newer one's.

J.3 `snapshot list` shows IDs in a stable, stated order, and shows nothing that
is not a snapshot.

J.4 Snapshot, restore, snapshot again — a second snapshot taken after a restore
works normally. The mechanism does not poison itself.

J.5 A file with a **newline in its name** is captured and restored with the name
intact. Names are bytes; anything that treats output as line-delimited will get
this wrong, and it is worth one line to prove which way this implementation went.

J.6 An empty root (a directory with nothing in it) can be snapshotted and
restored — RESTORED: still empty, no error. An undo mechanism that crashes on
"nothing to undo" is broken at the edge where it is most likely to be called.

J.7 `snapshot list` on an empty store reports no snapshots and exits 0 — not an
error. Same reasoning as J.6.

J.8 Every ordinary command's **exit code is meaningful**: 0 for success, 1 for a
refusal, 2 for a usage error — stated and tested, not assumed.

---

## K. Open items this list raises

Written down, not built (`AGENT-RULES.md` §4).

1. **Copy-on-write is not implemented, and this is a recorded deviation from
   `DESIGN.md` §3.4's wording.** The design says *"copy-on-write if the filesystem
   supports it, otherwise a plain copy"*. See `DECISIONS.md` for the reasoning;
   in short, the root's filesystem is 2e's decision and may be a tmpfs, which
   cannot reflink at all (observed: `cp --reflink=always` on `/tmp` fails with
   `Operation not supported`), so building COW now would be building against an
   undecided mechanism. Cost is the only thing COW changes, and cost is not a
   requirement of this phase. The measurement that would justify adding it later,
   and the reason it is safe to add later (the completed record verifies the copy
   whatever produced it), are in `DECISIONS.md`.

2. **Restore is not itself undoable.** If a restore goes wrong midway, the
   environment is in a mixed state. This is *mitigated* by refusing to start a
   restore that will not verify (§F), which removes the main way that happens.
   A snapshot taken automatically before each restore would close it completely
   and is not built here — it doubles what a restore writes and the plan does not
   ask for it. Recorded rather than invented.

3. **There is no retention policy.** Snapshots accumulate forever, one per task,
   each roughly the size of the environment. Nothing prunes them, nothing states
   a maximum, and disk exhaustion is a real failure mode for a long-running
   environment. Not 2c's to decide (it is a product question), but 2c is where
   the cost starts. Recorded.

4. **Concurrency is undecided**, as in 2b (§N.22): two snapshots at once, or a
   snapshot during a restore, have no stated semantics. The lock manager is
   `DESIGN.md` §10. Recorded, not invented.

5. **Nothing verifies the *root's own* filesystem** is a mount point, which
   `DESIGN.md` §3.1 requires and 2e owes. A snapshot of a root that is a plain
   directory inside a larger volume is still correct here; it is the hard-link
   channel that stays open until 2e.

6. **Where the app should keep its own state** is now decided in principle
   (§H.1: never inside the root) but no location is chosen. The store is one such
   location; the app will need others (notes, task data, the effect log). Not 2c's
   to site, but the principle is 2c's to state.

7. **Access times are not addressed.** *(Added 14 Sep 2026, from the blind list —
   `blind-attack-list-2c.md` item 19. It is a genuinely new item: nothing in §I or
   here covered it.)* Reading a file to hash it can update its **access time**, and
   §A.11's promise is that taking a snapshot changes nothing in the root. **The
   honest position:** §A.11 compares contents, names, kinds, modes and structure —
   atime is not compared there, and the harness's fingerprint deliberately
   normalises mtimes because §I.1 says they are not preserved. So the promise this
   phase actually makes is about *what is in the environment*, not about the
   kernel's bookkeeping. Closing it would mean `O_NOATIME`, which requires owning
   the file or holding `CAP_FOWNER`; it is not available in general and would be a
   silent failure on someone else's file. A filesystem mounted `noatime` or
   `relatime` makes it moot; that is a mount option, not this phase's. **Recorded
   rather than claimed**, because "taking a snapshot changes nothing" is the kind
   of sentence a later session could read as covering more than it does.

---

## L. Lines the blind list added — gate lines for its novel mechanisms

Added 14 Sep 2026 after `blind-attack-list-2c.md` was compared by mechanism
(§N.2). Each was probed against the built tool before being written down here;
these lines are the harness's permanent versions of those probes.

L.1 A **named pipe (FIFO)** inside the environment: `create` REFUSES by name and
returns promptly. The specific danger this line exists for is that a naive copy
routine *opens* the fifo to read it, and opening a fifo for reading blocks until a
writer appears — so the snapshot never finishes and the gate itself hangs. Probed
under a 10-second timeout: refused in milliseconds, no hang.

L.2 **Snapshot → restore → snapshot again, and compare the two records**: they must
be byte-identical apart from the recorded origin root. Stronger than §C.12, which
compares the tree, because it also proves the *symlink and mode* handling is
identical on a second pass — a first capture and a re-capture disagreeing is how
drift hides.

L.3 **Locale independence**: `create` under `LC_ALL=C`, `restore` under a UTF-8
locale. Nothing may be lost or merged by locale-dependent name collation. Uses
names that sort differently under the two collations (accented, non-Latin).

L.4 **Restore into an environment holding a read-only directory**: it must succeed,
not fail part-way. *This is the line for the defect the blind list found* (§N.1).
A directory whose contents cannot be deleted makes the delete step fail — and if
that failure happens after other deletions began, the environment is left
half-cleared, which is worse than not trying. The line asserts the restore
succeeds and a file modified elsewhere in the environment is genuinely restored.

---

## Independence

This list was written by the head that also wrote the build brief and will verify
the result, so the two share assumptions. Before the phase is accepted, a **blind
second list** must come from a subagent that has seen **none** of: the `snapshot/`
source, the brief, this file, `attack-list.md`, `attack-list-1b.md`,
`attack-list-2a.md` or `attack-list-2b.md`. It gets only a plain-words description
of what a snapshot-and-restore tool is for, in a sandboxed environment, and is
asked what it would try.

The blind list is **a gate, not an extra** (`HANDOFF.md` §4). Its value is
mechanism-level: Phase 2b's blind list produced §N, one line of which found a real
defect. Lines the blind list contributes are appended to this file as a new
section and never folded into the existing ones — so it stays visible which head
raised what.

## How this list was derived

Not invented from nothing. Its sources, in order:

1. `BUILD-PLAN.md` §2c and its Phase 2 verification block, quoted above.
2. `DESIGN.md` §3.4 (snapshots before every task; copy-on-write or a plain copy)
   and §3.1 (the root is a closed environment; host access off).
3. `DECISIONS.md` — the stack, the environment namespace, and the standing rule
   that a signed-off crate is not borrowed sideways from.
4. The open items the previous phases wrote down and handed forward — §N.23
   (app state inside the root, first exposed here), §N.22 (concurrency), L.4
   (the root as its own mount point). Each is either closed by a line above or
   explicitly carried forward in §K, not left to be rediscovered.
5. The shape of Phases 1, 1b, 2a and 2b: fixtures, sections, an independent
   outside sentinel, refusals checked by absence rather than by exit code, and a
   blind second list before acceptance.

**The ordering claim this file rests on:** §F, §E and §G are in this list *before*
any code exists, because they are the three ways this phase fails silently — a
half-written snapshot restored as complete, a symlink followed, and a name used
as a path. Every one of them produces something that looks like success.

---

## N. Lines the blind list added — and the defect one of them found

**`blind-attack-list-2c.md`** was written by a subagent given a plain-words
description of snapshot-and-restore and nothing else — no source, no brief, no
access to any project file, and in fact no tool call but the one that wrote its
answer (observed: 2 API calls, 76 s, one `write_file`). It produced ~70 items
across ten sections. It is preserved verbatim and **not** merged into §A–§K, so it
stays visible which head raised what.

Compared **by mechanism, not by text**: a literal diff reports a large overlap
that is an artifact of both lists covering the same subject. What follows is the
mechanism-level result, honestly stated — most of the blind list's ground was
already covered here, and the value is in the items that were not.

### N.1 — THE DEFECT IT FOUND (blind list item 26, `restore_empties_a_read_only_directory_instead_of_failing`)

**A read-only directory in the environment made `restore` fail, and fail after
destroying data.** `clear_contents` needs write+execute on a directory to remove
its children. A directory recorded as mode 555 therefore stopped the deletion —
but only *after* the loop had already deleted everything beside it. Observed
before the fix: `restore` exited 1 with `Permission denied (os error 13)`, the
file that had been modified was **gone rather than restored**, and only the
read-only directory survived.

This is the worst class of defect in this phase: the tool exists so that nothing
is lost by accident, and this loss was caused *by the undo mechanism itself*. The
fix is a pre-flight check (`ensure_contents_removable`) that runs before the first
deletion and makes directories writable as needed, so a tree it cannot empty is
refused **with nothing changed**.

**This was found after every other check in this file was green** — 97/97 on the
harness, 40/40 unit tests, three negative proofs holding. Nothing here would have
caught it. That is the case for the blind list being a gate rather than an extra,
and it is the second time one has paid for itself (2b's §N.8 was the first).

### N.2 — genuinely novel mechanisms, now gate lines

- **N.2a — a symlink planted at the snapshot's own name in the store.**
  (Blind list items 18, 36, 54.) The list had §G for names-as-paths but never for
  a *store entry that is already a symlink* when create arrives. Probed: `create`
  refuses on `LabelExists`, because the existence check is `tree.exists()`, which
  reports a symlink as present. Correct. **Added as a line: §F.12.**
- **N.2b — a snapshot tree replaced wholesale by a symlink to a decoy directory,
  with the record left intact.** The sharper form of the above: the record is
  genuine, `tree.is_dir()` is true because it follows the symlink, and the tree is
  *someone else's*. Probed: refused — `verify_snapshot_integrity` walks the decoy
  and the entry set does not match, naming `secret.txt … does not match … legit.txt`.
  The outside directory is never read. **Added as a line: §F.13.**
- **N.2c — an EXTRA file inside the store's snapshot tree.** (Item 42.) §F covered
  a *missing* file and an *altered* one, not an added one. Probed: refused, naming
  the extra file. **Added as a line: §F.14.**
- **N.2d — a FIFO in the environment.** (Items 7, 8.) The sharpest form: a naive
  copy routine opens the FIFO and **blocks forever**, because opening a fifo for
  reading waits for a writer. Probed under a 10-second timeout: refused by name in
  milliseconds, no hang — the `classify` refusal fires before any open. **Added as
  a line: §L.1.**
- **N.2e — restore, then re-snapshot, and compare the two records.** (Item 65.) An
  idempotence check at the *record* level, stronger than §C.12's tree comparison
  because it also proves symlink handling is identical on both passes. Observed:
  byte-identical records apart from the origin-root line. **Added as a line: §L.2.**
- **N.2f — locale independence.** (Item 68.) Create under `LC_ALL=C`, restore
  under a UTF-8 locale. Nothing may be lost to locale-dependent name collation.
  Observed: identical. **Added as a line: §L.3.**

### N.3 — mechanisms raised that this design already excludes, with the reason

Recorded so a later session does not have to re-argue them.

- **A hard link recreated where none existed** (item 64). Cannot happen: §I.2
  records that hard links are *not* preserved, and the restore writes plain files.
  Probed (§J of the harness) and asserted in `hard_links_are_restored_as_separate_files`.
- **Restore-by-"latest" driven by store mtimes** (item 67). Cannot happen: there is
  no "latest" — restore takes an explicit `--id` (§9.2), so store mtimes decide
  nothing. The mechanism is not merely refused; it does not exist.
- **Environment renamed between snapshot and restore** (item 32). §C.8 refuses a
  cross-root restore on the canonical path, so a rename is a refusal rather than a
  silent restore into the wrong tree. That is the conservative answer the blind
  list asks for.
- **Two labels differing only by case** (item 51). Fine by construction: a label is
  a directory name, and Linux is case-sensitive, so `Snap` and `snap` are two
  snapshots. Asserted in the label tests.
- **Same-label collision** (item 52). Refused, never a merged state (§B.9), and
  asserted non-destructively in `a_duplicate_label_is_refused_and_does_not_overwrite`.
- **`touch` on a stored file to defeat a size/mtime-based integrity check**
  (item 45). Cannot slip through: integrity is content-hashed (§9.3), and §F.2's
  same-length tamper test proves the hash path fires where a size check cannot.
- **A planted symlink drawing a *restore* outside the environment** (items 24, 25).
  This was the blind list's own "single most dangerous restore bug", and it was
  probed in its exact form — post-snapshot symlink where the snapshot holds a file,
  pointing at a sacrificial outside file, then restore. The outside file's checksum
  was unchanged and the file's original content came back. The mechanism that
  protects it is §E's rule applied on the *delete* side too: `clear_contents` uses
  `symlink_metadata`, so a planted link is removed with `remove_file` and the
  restore never writes through it.

### N.4 — recorded, not built (same standing as §K)

- **Atomicity of restore under interruption** (items 43, 59, 61): a power cut or
  ENOSPC midway leaves a half-restored environment. §K.2 records the mitigation
  (nothing is deleted until the snapshot verifies) and declines the full fix
  (stage-beside-and-swap) as beyond what the plan asks for. The blind list agrees
  it is "genuinely hard"; it is now recorded in two places instead of one.
- **A consistent point-in-time copy of a tree being written to** (item 55): §K.4.
  The mitigation added above — comparing the walk's hashes against the copy's
  hashes — is a *detection* of churn, not a freeze. A file that changes during the
  copy is now refused rather than recorded torn.
- **atime** (item 19): §K raises it? It does not — this is a genuinely new item.
  Reading a file in the environment can update its access time, which "taking a
  snapshot changes nothing" arguably forbids. **Recorded as §K.7**, with the honest
  position: the promise in this list is about the environment's *contents* and
  *structure*, atime is not compared anywhere, and the fix (`O_NOATIME`) has an
  ownership precondition. Not silently claimed as covered.
- **Store integrity against a same-user attacker** (item 70): §H.4 already states
  it, and the blind list's sharper phrasing — that the agent runs as the same user
  and can rewrite store *and* record if it can reach them — is worth keeping. See
  §K.5 and §H.4 together.

### N.5 — the blind list's own honest section

Its final section lists five things it believes are hard or impossible to
guarantee: a truly consistent live-tree copy; crash-safe restore; environment
identity across a rename; nothing-outside-touched under TOCTOU; atime neutrality;
and store integrity against the same user. **Every one is recorded above rather
than dismissed**, and three of them (§K.2, §K.4, §H.4) were already in this list
before the blind list was asked. That agreement from an independent head is worth
something: it is evidence that the boundaries marked here are real limits rather
than convenient omissions.
