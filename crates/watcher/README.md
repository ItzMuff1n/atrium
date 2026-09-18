# atrium-watcher

Phase 2d of Atrium — **the filesystem watcher** (`BUILD-PLAN.md` §2d). It watches
the environment root and reports what changed inside it.

```
atrium-watcher watch --root <dir> [--duration-ms N] [--settle-ms N] [--show-real]
atrium-watcher demo
atrium-watcher fixtures --root <dir> [--outside <dir>]
```

## Read this first: what it is, and what it is not

**It is a watcher, not a gate.** It observes and reports. It decides nothing,
blocks nothing, gates nothing, and writes nothing. `DESIGN.md` §3.3 says why it
exists: `run commands` (Phase 2b) executes real programs, so those programs change
files *outside* Atrium's own file-operation layer. The `ran` effect covers the
command and its output; **the watcher covers what the command did.**

**It is what tells the human the tree is not stale.** Without it, a command can
delete a folder, rewrite a file and move a tree, and the live view keeps showing
the old state with no indication anything is wrong.

## The four ways a watcher fails *silently*

Every other phase fails by doing something it should not. A watcher fails by **not
reporting something and looking fine.** These are what most of this crate is about.

| Failure | How it is answered |
|---|---|
| Events are lost and the run looks clean | The kernel's queue is finite. When it overflows it drops events and sends one record with `wd = -1`; that becomes `OVERFLOW`, **the run exits 1**, and nothing guesses what was missed. |
| Part of the tree is never watched | One watch per directory, by a walk. A directory that cannot be watched is **reported**, not skipped: `ERROR … changes in it will NOT be reported`. |
| A watch dies and nobody is told | `DELETE_SELF` / `IGNORED` become `GONE`. A dead watch reports exactly as much as an idle one — nothing. |
| The watcher watches something it was not asked to watch | Every watch placement passes `IN_DONT_FOLLOW`, the walk uses `symlink_metadata`, and every reconstructed path is checked against the canonical root before it can become a record. |

**A run that produced any `ERROR`, `OVERFLOW` or `GONE` exits non-zero.** A green
exit cannot be produced by a watcher that quietly missed things.

## The boundary, stated precisely

- **The report names virtual paths** — `/home/documents/x.txt`, never the root's
  real location. `DESIGN.md` §3.1: the agent never learns the real path exists.
  `--show-real` prints real paths and exists for the person reading the tool, the
  same exception `shell/` makes.
- **A refusal names the real path on purpose**, because the reader is the user who
  must know which directory was refused — the `snapshot/` precedent
  (`DECISIONS.md`). The prohibition is on the change stream, not on a refusal.
- **It reports changes to the whole tree, including ones Atrium's own file
  operations made.** At the kernel a shell command and our own file layer are
  indistinguishable, so it cannot filter them — and de-duplication is Phase 3's
  job, not this phase's. Stating that here so nobody reads an authority into this
  phase that it does not have.

## The reduction it makes, measured

- **One `Modified` per completed edit** (one open → write → close). Measured:
  a file written in ten chunks inside one open produces a single `MODIFY`; `dd` of
  8 MiB produces one; **three separate open/close cycles produce three** — because
  three completed edits *are* three changes, and collapsing them would be the
  watcher losing two.
- **`IN_MODIFY` is consumed, not reported.** Reporting it would triple the stream
  for one logical edit.
- **`IN_ACCESS` is not watched at all.** A read is not a change; watching it would
  make `ls`, `stat` and taking a snapshot flood the stream.

## Known limits — recorded, not hidden

- **Linux only**, via inotify. v1 has no portability requirement and this crate
  does not pretend to be portable.
- **A change made to an in-root file *through a hard link outside the root* is
  invisible** (measured: no event at all). This is the Phase 2a/§N hard-link
  channel appearing in the watcher; it is closed by the environment root being its
  own mount, which is **2e's** work. Test `i8` asserts the current behaviour so a
  later phase closing it will see that test change.
- **A file written into a brand-new directory before its watch is attached** can be
  missed. The window is one event-loop turn. A walk runs on the directory's
  `CREATE`, which closes most of it; the remainder is a real race and is not
  papered over by inventing events.
- **After an overflow, events are gone.** The gap is reported; nothing is guessed.
  A resync is deliberately *not* built here (`attack-list-2d.md` §L).
- **A close that straddles startup is reported.** Measured: an open/write that
  lands before the walk and whose *close* lands after it delivers `CLOSE_WRITE`
  alone — no `IN_CREATE`, no `IN_MODIFY`. The bytes were written before the watcher
  existed, but the event happened after it, and the two are indistinguishable.
  Since this phase's whole purpose is not losing changes, it is **reported**. The
  alternative would be dropping a real change. Boundary pinned by test `j9`.
- **It cannot tell who made a change.** Phase 3 de-duplicates.
- **This tool is not agent-facing** and is not routed through the resolver: it
  takes `--root`, a real host path, exactly as `snapshot/` does.

## No dependency, deliberately

The inotify entry points are declared as `extern "C"` symbols and resolved against
the libc Rust already links for `x86_64-unknown-linux-gnu`. **Nothing is fetched
from a registry**, so this is not the stop-and-ask dependency `AGENT-RULES.md` §5
guards. The alternatives and why they were rejected (`libc`; `syscall()` directly;
polling instead of inotify) are recorded in `DECISIONS.md`.

## The line format

One record per line, greppable by its tag column.

```
CREATE   /home/documents/notes.txt
MODIF    /home/documents/notes.txt
DELETE   /home/documents/old/ dir
MOVEFROM /home/documents/a.txt cookie=123009
MOVETO   /home/documents/b.txt cookie=123009
ATTRIB   /home/documents/sub/
GONE     /home/documents/sub/ (deleted)
OVERFLOW dropped-before=8192
ERROR    /home/documents/sealed cannot be watched (…); will NOT be reported
```

A **filename is bytes, not text.** A name that is not valid UTF-8 is printed with
the offending bytes escaped (`\xFF`), not replaced — a lossy conversion would print
`bad-??-name.txt` and two different files could print identically. A newline in a
name is escaped so one record is always one line.

## Files

- `src/lib.rs` — the watcher: `Watcher::start`, `drain`, `Change`, `Options`.
- `src/main.rs` — the `watch`, `demo` and `fixtures` modes.
- `tests/watcher_tests.rs` — the attack list, line by line (32 tests).
- `tests/probe8_blind_list.rs` — the independent blind list's mechanisms as tests.
- `hand-test-2d.sh` — the hands-on harness (the gate).
- `negative-proofs-2d.sh` — proves the harness's own checks can fail.
- `attack-list-2d.md` (project root) — the spec, written **before** this code.
- `blind-attack-list-2d.md` (project root) — the independent second list.

## Exit codes

`0` clean run · `1` refusal, **or a run that lost events / hit an error** · `2`
usage error.
