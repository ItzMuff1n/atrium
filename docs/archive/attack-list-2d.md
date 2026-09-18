# Attack list — Phase 2d: the filesystem watcher

**Written 15 Sep 2026, before the code exists.** That ordering is the point: this
file says what the watcher must do. Written afterwards it becomes a description of
the code instead of a test of it. Phases 1, 1b, 2a, 2b and 2c were all built this
way, and 2b's and 2c's blind lists each found a real defect precisely because the
list existed independently of the code.

**Authority:** `BUILD-PLAN.md` §2d — *"Watch the environment root for changes made
outside the file-operation layer (i.e. by shell commands). Emit change
notifications. They have nowhere to go yet — Phase 3 gives them one."* Also
`DESIGN.md` §3.3 (commands change files outside our own layer, so the explorer
would go stale; the watcher emits the resulting effects), §3.1 (the agent never
learns the real path exists), §6.2 (`ran` covers the command and its output; the
watcher covers what the command did), and the plan's Phase 2 verification step
*"Run a shell command that creates a file. Confirm the watcher noticed."*

**This is a working copy.** If it and `DESIGN.md` disagree, stop and say so — do
not pick one silently (`AGENT-RULES.md` §2).

---

## What is being tested, in plain words

Everything built so far can *do* things. Phase 1 decides which paths are allowed.
2a acts on them. 2b runs real programs. 2c can put the environment back.

**2d is the first thing that observes.** A real command run through 2b is a black
box: the `ran` effect records that something executed and what it printed, but not
one word about what it changed. 2a's own operations report themselves; a shell
command does not. So a command can delete a folder, rewrite a file and move a tree
around, and Atrium learns none of it.

The mental model is a **security camera on the environment**. It records what
changed. It does not decide, block, or act — it notices and reports.

Three questions decide whether it is any good:

1. **Does it notice the right things, once each, with the right name?**
2. **Does it admit what it cannot see?** A camera with a blind spot is fine if the
   blind spot is labelled; the failure mode is a camera with a blind spot that
   reports the room as empty.
3. **Does it stay inside the environment?** A watcher on the root that reports a
   change made *outside* the root is a hole in `DESIGN.md` §3.1 — the sandbox
   leaking the host.

**Point 2 is where this phase is most likely to be wrong.** Every other phase fails
by doing something it should not. A watcher fails by *not* reporting something and
looking fine: an event dropped, a directory it never watched, a queue that
overflowed, a watch that silently died. Almost every section below exists to make
one of those failure modes observable.

---

## The vocabulary of a result

Same discipline as the previous lists. A line is one of:

- **REPORTS** — the watcher must emit a specific change for this action.
- **NOT-REPORTS** — the watcher must emit nothing for this action, and the line is
  as important as a REPORTS line.
- **REPORTS-ONCE** — a burst that must not be turned into many records.
- **REFUSES** — a request the watcher must decline, with a reason naming the
  specific problem.
- **STATES** — a printed statement of a limit. Not a check; it exists so the
  boundary is visible where the result is read.

**A passing line and a failing line differ by one printed word.** A run therefore
cannot be checked by eye for correctness, only for the absence of `[FAIL]`. Reading
this file is what establishes that the right lines ran. Said plainly here because
it is true of every list in this project and gets re-derived each time.

## Fixtures

Created at the start of every run by the harness, and thrown away at the end.

```
/tmp/atrium-2d-root/            the environment root (a throwaway)
  home/documents/               a nested directory
    notes.txt                   an ordinary file with known contents
    sub/deep/                   a directory three levels down
  link-to-outside -> /tmp/atrium-2d-outside   a symlink out of the root (dangling
                                 if the target is removed)
  dangling -> /nowhere/at/all   a symlink to nothing
/tmp/atrium-2d-outside/         an outside directory, with sentinel.txt
```

Outside and host state is captured **before the run** and re-checked after **every
line**: the outside directory's full listing and the checksum of `sentinel.txt`,
and `/etc/passwd`'s checksum and mtime. Any change fails the run regardless of what
the watcher printed about itself.

---

## A. The plain case — the plan's own verification step

The line this whole phase exists to answer, first, because everything else is a
refinement of it.

| # | Action | Must | Note |
|---|---|---|---|
| A.1 | `sh -c 'echo hi > <root>/home/documents/by-command.txt'` | REPORTS `CREATE` then `MODIFY` (at close) for `/home/documents/by-command.txt` | *"Run a shell command that creates a file. Confirm the watcher noticed."* |
| A.2 | the same, via `mkdir -p` | REPORTS `CREATE` for the directory | |
| A.3 | `mv` a file, same directory | REPORTS `MOVEFROM` + `MOVETO`, **same cookie** | |
| A.4 | `rm` a file | REPORTS `DELETE` | |
| A.5 | `rm -r` a directory with contents | REPORTS `DELETE` for each child, then for the directory | Observed order: children first, then the parent |
| A.6 | `chmod 700` a file | REPORTS `ATTRIB` | |
| A.7 | a command that changes nothing (`true`) | NOT-REPORTS | |

## B. Once each, and once only

The reduction that keeps the stream readable. **Measured on this kernel 15 Sep
2026** (probe 7), because the first draft of this section had it wrong:

- One file opened, **written in 10 chunks**, closed once → 3 raw records
  (`CREATE`, `MODIFY`, `CLOSE_WRITE`). The kernel coalesces the ten chunks into
  **one** `MODIFY` itself.
- `dd` of 8 MiB in one open/write/close → 3 raw records, **one** `MODIFY`.
- Three separate `open`/`write`/`close` cycles on the same path → **6** raw
  records: three `MODIFY` and three `CLOSE_WRITE`.

> **Correction, 15 Sep 2026 — this section's first draft was wrong, and the reason
> matters.** It said "write one file three times → REPORTS-ONCE `Modified`". That
> was derived from an earlier probe that counted `CREATE, MODIFY, CLOSE_WRITE`
> three times over and was read as "one edit reported three times". It is not: it
> is **three separate edits**, each an `open`/`write`/`close` cycle. Three
> completed edits are three changes, and reporting one would be the watcher
> **losing** two. The reduction this phase actually makes is **one record per
> completed edit**, which is what the lines below now say. Corrected rather than
> quietly rewritten, because an expectation that passes for the wrong reason is the
> failure this list exists to catch.

So the rule is: **one `Modified` per completed edit** (one open→write→close), and
the number of `write` calls *inside* one open is invisible — the kernel coalesces
those, not us.

| # | Action | Must | Note |
|---|---|---|---|
| B.1 | write one file three times in **one open**, then close | REPORTS-ONCE `Modified` | measured: 10 chunks in one open → one `MODIFY`. The reduction is below us |
| B.2 | the same file written in **three separate** open/close cycles | REPORTS **three** `Modified` | measured: three edits are three changes; one record would be two changes lost |
| B.3 | `dd` 8 MiB in one go | REPORTS-ONCE `Modified` | measured: one `MODIFY` for 8 MiB |
| B.4 | 500 files created in a burst | REPORTS 500 `Created` | measured: the kernel does **not** coalesce across entries |
| B.5 | read a file (`cat`), never writing | NOT-REPORTS | no `IN_ACCESS` watch; a read is not a change |
| B.6 | `stat` a file | NOT-REPORTS | |
| B.7 | `ls` a directory | NOT-REPORTS | listing is not a change |
| B.8 | create then delete a file, both before a drain | REPORTS both, in order | measured: not coalesced into "nothing happened" |

## C. The name is right — including names that are not text

| # | Action | Must | Note |
|---|---|---|---|
| C.1 | create `notes.txt` | REPORTS the path `/home/documents/notes.txt` | virtual, not real |
| C.2 | create a file whose name contains a space | REPORTS the name exactly | |
| C.3 | create a file whose name contains a newline | REPORTS the name exactly, **not split across two lines** | |
| C.4 | create a file whose name contains a non-UTF-8 byte (`\xff`) | REPORTS the name with the byte escaped (`\xFF`), **not** as a replacement character | the lossy form would print `bad-??-name.txt` |
| C.5 | two files whose names differ only in the non-UTF-8 byte | REPORTS two **distinct** lines | a lossy conversion would make them identical |
| C.6 | create a file whose name is 255 bytes | REPORTS it | the filesystem's limit, not ours |
| C.7 | a name containing `%00` as four literal characters | REPORTS them literally | not a NUL byte |
| C.8 | every reported path | **must not contain the root's real on-disk path** | checked as bytes, unconditionally — see §H |
| C.9 | the same, with `--show-real` | prints real paths | the documented exception, off by default |

## D. The tree — what is watched, and what is not

inotify is **not recursive** (observed: a write inside a subdirectory produced no
event on the parent's watch). One watch per directory is therefore the mechanism,
and where those watches are placed is the whole of what this phase can see.

| # | Action | Must | Note |
|---|---|---|---|
| D.1 | write a file in `sub/deep/` | REPORTS it | the walk reached three levels |
| D.2 | write a file in a directory created **after** the watcher started, some time later | REPORTS it | the new directory must get a watch when its `CREATE` arrives |
| D.3 | write a file in a brand-new directory **immediately** | STATES a known race | observed: no event — the window is one loop turn. Must be **stated**, not hidden |
| D.4 | `link-to-outside` exists and the **outside target is written to** | NOT-REPORTS | with `IN_DONT_FOLLOW` no watch is placed through the link. Without it, observed: the outside write **is** reported — a host-activity leak |
| D.5 | a dangling symlink in the tree | NOT-REPORTS, no error | not descended into; not an error either |
| D.6 | a symlink to a directory **inside** the root | NOT-REPORTS changes made via the target's real path | only the real path is watched; a change is one change, once |
| D.7 | a directory with mode `000` in the tree at start | STATES/REPORTS an unwatchable directory, **and keeps watching everything else** | observed: `EACCES`. One bad directory must not blind the root |
| D.8 | a directory whose mode becomes `000` after the watcher started | NOT-REPORTS an error and NOT blind elsewhere | |
| D.9 | the tree walk on a large root | STATES the cost | observed: 2461 dirs → 2461 watches in 4.82 ms |

## E. Moves — the cookie is the pairing, not the watch

Observed: `mv d1/f.txt d2/f.txt` produced `MOVED_FROM` on `d1`'s watch and
`MOVED_TO` on `d2`'s watch, **sharing one cookie**. Pairing per-watch would report
one move as two unrelated events.

| # | Action | Must | Note |
|---|---|---|---|
| E.1 | move within one directory | REPORTS a matched pair, same cookie | |
| E.2 | move between two directories, both watched | REPORTS a matched pair, same cookie, from **two different watches** | |
| E.3 | move a file **out of** the root | REPORTS `MOVEFROM` with no partner | must not invent a `MOVETO` |
| E.4 | move a file **into** the root from outside | REPORTS `MOVETO` alone | must not invent a `MOVEFROM` |
| E.5 | rename a **directory** inside the root | REPORTS the pair, and afterwards a change inside it is reported under the **new** path | observed: events keep arriving on the old `wd`, so the table must be re-keyed |
| E.6 | rename a directory so a nested file's path changes | REPORTS the file's events under the new path | the `wd` is per-inode and stable; the *path* is what changed |
| E.7 | move a file onto an existing file (overwrite) | REPORTS both halves | the old content's loss is a change |

## F. The reduction, stated so it is not read as an omission

| # | Action | Must | Note |
|---|---|---|---|
| F.1 | `IN_MODIFY` mid-write | NOT-REPORTS on its own | the stream reports at close; stated in the README |
| F.2 | `IN_ACCESS` | NOT-REPORTS, ever | the mask does not include it; stated |
| F.3 | `IN_ATTRIB` on a file | REPORTS `ATTRIB` | a permission change is a change |

## G. Refusals — the watcher declines, with a reason

| # | Action | Must | Note |
|---|---|---|---|
| G.1 | `--root` that does not exist | REFUSES, names the problem | |
| G.2 | `--root` that is a file, not a directory | REFUSES | |
| G.3 | `--root` given as a relative path | REFUSES | the 2c precedent: a tool taking host paths should not guess |
| G.4 | no `--root` at all | usage error, exit 2 | |
| G.5 | `--settle-ms` not a number | usage error | |
| G.6 | a root the process cannot read | REFUSES, names it | |

## H. The disclosure boundary — the check that makes §C.8 real

The one thing here that a later session is most likely to "tidy" and break.

| # | Action | Must | Note |
|---|---|---|---|
| H.1 | a full default-mode run | **no line contains the root's real path** | checked by the harness as bytes, after every line, unconditionally |
| H.2 | a refusal naming the root | prints the real path | permitted **here only** — the reader is the user |
| H.3 | the change stream, with a refusal having happened earlier in the same run | still contains no real path | the boundary is per-line, not per-run |
| H.4 | `--show-real` | prints real paths throughout | and is stated as off by default |
| H.5 | any event whose path is not under the root | REPORTS `ERROR`, **not** a change record | should be impossible; if it happens, say so loudly |

## I. What the watcher is, and is not

Statements printed in every run, so the boundary is visible **where the result is
read** — the 2c precedent.

| # | Statement |
|---|---|
| I.1 | It reports changes **inside the root only**; it cannot be asked to watch outside, and host access is off in v1. |
| I.2 | It **cannot tell who made a change** — the file-operation layer and a shell command look identical at the kernel. De-duplication is Phase 3's. |
| I.3 | It **emits no effects**: no kind, count, label, SQLite, or log. Phase 3 owns the effect stream. |
| I.4 | It **gates nothing and blocks nothing.** Phase 4. |
| I.5 | It **reports all changes inside the root**, including ones Atrium's own file operations made — by design, and stated rather than filtered. |
| I.6 | **Linux only**, via inotify. Not a v1 requirement to be portable. |
| I.7 | A **read is not watched.** No `IN_ACCESS`. |
| I.8 | A change made to an in-root file **through a hard link outside the root is invisible** (observed: no event). Closed by the root being its own mount, which is 2e's. |

## J. The quiet failure modes — loss, blindness, death

The reason this section exists at all. Every line here is a way the watcher stops
reporting things and continues looking healthy.

| # | Action | Must | Note |
|---|---|---|---|
| J.1 | overflow the kernel queue (> 16384 undrained events) | REPORTS `OVERFLOW`, naming that a gap exists | observed: 16385 drained for 300,000 creations, one `Q_OVERFLOW` record with `wd = -1` |
| J.2 | the same | **must not** report the run as complete | a lost-events run is a failure; the CLI exits 1 |
| J.3 | the same | **must not** invent events to fill the gap | no guessed resync |
| J.4 | delete the watched root directory | REPORTS `GONE` for it | observed: `DELETE_SELF` + `IGNORED` |
| J.5 | replace the root by a rename | REPORTS `GONE`, **and afterwards states it is blind to that path** | observed: after the swap, later writes produced **zero** events — silence would look like "nothing happened" |
| J.6 | an explicit watch removal | REPORTS the watch ended | observed: `IGNORED` |
| J.7 | a `MOVEFROM` whose partner never arrives within the settle window | REPORTS it as one half, not as an error | a move out of the root is not a fault |
| J.8 | the watcher is asked to keep running with zero watches left | STATES that it can see nothing, and **does not exit 0 quietly** | |

## K. The boring half — ordinary use must simply work

| # | Action | Must | Note |
|---|---|---|---|
| K.1 | start on a normal root | exits 0 for a `--duration-ms` run | |
| K.2 | `--duration-ms 0` | runs, prints the watches placed, exits 0 | |
| K.3 | an empty root | no changes, exit 0 | |
| K.4 | a root with 5,000 files | walk completes, watch count reported | |
| K.5 | `demo` | runs its own scenario and reports what it saw | |
| K.6 | the harness run twice in a row | identical output | determinism, as every phase's harness has been |
| K.7 | a run that reports any `ERROR` or `OVERFLOW` | exit 1 | a clean run is the only green |

## L. Open items this list raises

Written down, not built (`AGENT-RULES.md` §4).

1. **The create-then-write race** (§D.3). One event-loop turn of blindness in a
   brand-new directory. Needs a resync pass or a different mechanism; Phase 3's to
   weigh, since it owns re-reading `DESIGN.md` §8.
2. **Resync after overflow** (§J.1–J.3). Decided here: report the gap, do not
   guess. A real resync is a separate, verifiable piece of work.
3. **Hard links** (§I.8). Observed in this phase: a write to an in-root inode via a
   name outside the root produced no event. The 2a/§N channel reappearing in the
   watcher; closed by the root's own mount, which is 2e.
4. **De-duplication** (§I.2/§I.5) against the file-operation layer's own effects.
   Phase 3.
5. **The `IN_MODIFY` reduction** (§F.1). Chosen for signal-to-noise; if a consumer
   ever needs mid-write progress, that is a new decision with its own reasoning,
   not a quiet change to the mask.
6. **inotify is Linux-only** (§I.6). Not a v1 requirement.
7. **A mount point inside the root, or an overlay/FUSE filesystem under it** (§M.3).
   inotify watches the filesystem object beneath a mount, so writes inside a mounted
   filesystem are not reported, and some stacked filesystems have their own
   event-fidelity quirks. v1's root is one plain filesystem; **2e** requires it to be
   its own mount, which is the phase that owns this. Raised by the blind list (§1.5),
   not by this list.
8. **A close that straddles startup is reported** (§5 of the evidence file). Measured:
   an open/write landing before the walk and a close landing after it delivers
   `IN_CLOSE_WRITE` alone. It is reported deliberately — suppressing it would drop a
   real change — and test `j9` pins the boundary. What was *fixed* is the kernel
   queue already in flight when the watches were placed, which is drained at start.

---

## Independence

This list was written by the head that also wrote the build brief and will verify
the result, so the two share assumptions. Before the phase is accepted, a **blind
second list** must come from a subagent that has seen **none** of: the `watcher/`
source, the brief, this file, or any other attack list in this project. It gets
only a plain-words description of a filesystem watcher in a sandboxed environment
and is asked what it would try.

The blind list is **a gate, not an extra** (`HANDOFF.md` §4). Its value is
mechanism-level: 2b's found a real hang, 2c's found a restore that destroyed data.
Lines it contributes are appended as a new section and **never folded into the
existing ones** — so it stays visible which head raised what.

## M. Lines the blind list added — and the defect one of them found

**`blind-attack-list-2d.md`** was written by a subagent given a plain-words
description and nothing else — no source, no brief, no access to any project file
(observed: 2 API calls, 79.75 s, one `write_file`, no reads). It produced ~30 items
across eight sections. It is preserved verbatim and **not** merged into §A–§L, so it
stays visible which head raised what.

Compared **by mechanism, not by text**. Result, honestly stated: most of its ground
was already covered here or already excluded by design. **It found two things worth
acting on, and one of them was a real defect.**

### M.1 — THE DEFECT IT FOUND (its §1.3; tests `blind_1_3`, `d3` in the harness)

**A populated subtree moved *into* the root was watched only at its top level.**
`mv bigdir/ root/` delivers a single `MOVED_TO` for the top directory; the tree
inside it produces no events anywhere. The code attached one watch to the top and
nothing below, so **every file inside was invisible forever** — and nothing ever
re-walks the tree, so no later event would correct it either. A viewer shows a
populated folder as one empty directory, confidently and permanently.

Observed before the fix: after writing at three levels of the moved-in tree, the
only record was `CREATE /home/incoming/top2.txt`. Nothing at all for `lvl2/` or
`lvl2/lvl3/`.

Fixed by walking the moved-in subtree rather than adding one watch. **Proven
load-bearing** by reverting the fix and watching the test fail with exactly that
gap. A harness line (`D.3`) now covers it, because a test the agent wrote shares the
agent's assumptions and this is precisely the class of thing an independent head
sees and the author does not.

### M.2 — a prompt, not a find: a new directory's contents (its §1.2)

The same shape one level down. The race was already recorded here (§D.3) as a known
window; the blind list stating it independently is what prompted closing it further
by **walking** on `CREATE` as well as on `MOVED_TO`, rather than adding a single
watch. Recorded as a prompt rather than a discovery, because the window was already
known and documented — claiming it as a find would be overstating the gate.

### M.3 — mechanisms it raised that this design already excludes, with the reason

Kept here rather than argued away in a chat window, because a rejection with no
record is a rejection that gets re-proposed.

- **§1.4 — mmap writes and delayed writeback.** `IN_MODIFY`/`CLOSE_WRITE` timing is
  the kernel's, and this phase reports the close. It cannot and does not claim to
  report an mmap write at the instant the bytes reach memory. That is the kernel's
  semantics, not a defect here, and inventing a poll to catch it is the mechanism
  this phase rejected (§9.1 of the brief). **Recorded as a real limit**, and it is
  why "modified at close" is the wording everywhere.
- **§1.5 — a mount point inside the root, or a filesystem beneath it.** *Real, and
  an input to 2e.* inotify watches the filesystem object beneath the mount, so a
  write inside the mounted filesystem is not reported. v1's root is one filesystem;
  2e requires it to be **its own mount** (`DESIGN.md` §3.1), which is exactly the
  phase that owns this. Carried in §L.
- **§4.3 — descriptor reuse cross-labelling.** Tested (`blind_4_3`): the table is
  keyed **by inode**, not by position, so a recycled `wd` cannot carry a stale path
  onto a new directory, and a retired `wd`'s stragglers are dropped rather than
  reported as anomalies. Passes. The concern is legitimate; it is answered by the
  design rather than by luck.
- **§4.1/§6.2 — descriptor and instance limits.** Measured rather than assumed
  (§1 of the evidence file): `max_user_watches` 300000, `max_user_instances` 2048,
  and 2461 directories cost 4.82 ms. A limit hit is an `ERROR` naming the virtual
  path, never a silent half-covered tree. Test `blind_6_2` covers 600 directories.
- **§2.6/§6.4 — coalescing.** Measured, and this list's §B had it **wrong** first
  (see the correction there). The kernel does not coalesce across entries; it
  coalesces *within* one open. The reduction this phase makes is therefore one
  record per completed edit, and the `IN_MODIFY` bit is what is consumed.
- **§7.x — rescan/identity/bypass mechanisms** (mtime forgery vs a rescan, replace
  shown as create, debounce net-state, inode reuse). None applies: this phase
  performs **no rescan, no debounce and no identity tracking**, and reports what
  happened rather than a computed net state. That is a reason to keep it that way —
  §L.2 declines a resync partly because a resync is where all four of these become
  reachable.
- **§5.6 — special files.** Tested (`blind_5_6`): the watcher never opens anything —
  it only hashes nothing and reads no file — so a FIFO or device node cannot wedge
  it. A FIFO's creation is reported as an ordinary entry.

### M.4 — the blind list's own honest section

Eight items it could not settle without a machine. Two this phase then answered by
measurement (the queue threshold; whether mmap-visible events arrive at all). The
rest are open and recorded: the mount stack, whether device nodes are creatable in
the sandbox, and whether the filesystem is a plain volume or an overlay/FUSE layer
(overlayfs and FUSE have event-fidelity quirks that would each be their own entry).
Preserved rather than resolved, because an unresolved question written down is worth
more than a confident guess.

### M.5 — what the second half of the phase was run against

`tests/probe8_blind_list.rs` — ten of the blind list's mechanisms as tests. Its
results are in `phase-2d-evidence.txt` §7, including the two that failed on first
run and why each was right to fail.

## How this list was derived

Not invented from nothing. Its sources, in order:

1. `BUILD-PLAN.md` §2d and the Phase 2 verification step, quoted above.
2. `DESIGN.md` §3.3 (why the watcher exists), §3.1 (the real path is never
   disclosed), §6.2 (`ran` vs the watcher; `read` vs `reported`).
3. **Direct measurement on this machine, 15 Sep 2026** — five probe programs, no
   project file touched. Everything marked *observed* above is from those runs:
   the queue limit and its overflow record, `wd` stability across a rename, cookie
   pairing across two watches, the death of a watch when its directory is replaced,
   non-recursiveness, non-UTF-8 name bytes, `EACCES` on a mode-000 directory,
   `IN_DONT_FOLLOW`'s two behaviours, hard-link invisibility, and the watch-placement
   cost. **This is the first attack list in the project whose kernel facts were
   measured rather than reasoned** — 2c's equivalent facts were read from docs and
   from failures.
4. The open items previous phases handed forward: §N.23 (app state in the root),
   §N.22 (concurrency), L.4 (the root as its own mount), §K (the symlink-object
   limitation). Each is either closed by a line here or carried forward in §L.
5. The shape of Phases 1, 1b, 2a, 2b and 2c: fixtures, sections, an independent
   outside sentinel, refusals checked by absence, and a blind second list before
   acceptance.

**The ordering claim this file rests on:** §C, §D and §J are in this list *before*
any code exists, because they are the three ways this phase fails **silently** — a
name reported wrong, a part of the tree never watched, and a change lost with the
run still looking clean. Every one of them produces something that looks like
success. §B and §F are here because the fourth silent failure is reporting *too
much*, which buries the signal and is indistinguishable from a busy environment.
