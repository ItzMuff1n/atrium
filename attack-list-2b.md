# Attack list — Phase 2b: the real shell

**Written 14 Sep 2026, before the code exists.** That ordering is the point: this
file says what the command runner must do. Written afterwards it becomes a
description of the code instead of a test of it. Phase 1b and 2a were built this
way.

**Authority:** `BUILD-PLAN.md` §2b — "Run actual commands with the working
directory confined to the environment root. Capture stdout, stderr and exit
code." `BUILD-PLAN.md` §2b's dated correction, `DESIGN.md` §3.3 and §3.1.

**This is a working copy.** If it and `DESIGN.md` disagree, stop and say so — do
not pick one silently (`AGENT-RULES.md` §2).

---

## What is being tested, in plain words

Phase 2a built the **hands**: create, read, write, move, delete, list — and every
one of them goes through the resolver first, so the sandbox can only act on
things inside the root.

Phase 2b builds the **ability to run a real program**. That is a different kind
of thing, and it is worth being blunt about why.

A file operation is something *we* do, so we can make it check the path first.
A command is something *someone else's program* does. Once it starts, it is not
our code making the decisions any more. It is a real program on your real
machine, with your real permissions.

**So this phase has an honest, narrow promise, and it is written here so nobody
can mistake it for a bigger one:**

> 2b guarantees **where a command starts** and **what it is handed when it
> starts** — the directory, the environment, what its input is, and that its
> output and exit code come back unaltered. It does **not** confine what the
> command can then reach. A command can still `cd /` and act on your real files.

That second half is not a gap in 2b. It is `BUILD-PLAN.md` §2e, and it is why
2e exists. §G below is the section that *shows* the hole rather than leaving it
to be discovered, because a gate that quietly implies confinement would be worse
than no gate.

**What 2b can therefore be verified to do, and what this list tests:**

1. **A command starts where the runner said it would** — inside the environment
   root, never outside it.
2. **A command starts with a controlled environment**, not the user's — no
   inherited secrets, input not connected to your terminal.
3. **Output and exit code come back unaltered** — bytes are bytes, and a killed
   command does not look like a successful one.
4. **A refusal refuses: nothing starts at all.** Not "starts and fails".
5. **Our own messages never name the sandbox's real location on disk**, and the
   command's output is never rewritten to hide it.
6. **The runner cannot be hung or flooded** — a command that never exits, or
   floods output, does not take the runner with it.

Point 6 is the one place this list adds something the plan does not literally
say. Reasoning, stated plainly so it can be overruled: a runner that a command
can hang forever cannot be verified (the gate itself would hang) and cannot be
safely used. The stop is a **safety stop with a default limit**, not the kill
switch — the kill switch is `DESIGN.md` §7.1, Phase 4, and is user-initiated with
a UI. See §F and §K.

---

## The vocabulary of a result

This phase is mostly *doing*, not *refusing*, so the words differ from 2a's.

- **RUNS** — the command must actually run, and the stated observable must be
  true: the output bytes, the exit code, or where a file ended up on disk.
- **REFUSED** — the runner must not start anything. Checked by looking for the
  marker file the command would have created; a refusal is only a pass if
  nothing ran anywhere.
- **HOLE** — the line demonstrates a boundary 2b does **not** have. It is
  expected to succeed at reaching outside, and it is marked so it can never be
  read as a pass. These belong to 2e.
- **NOT HERE** — belongs to another phase; listed so the gap is not silent.

Two independent checks run alongside the lines, either of which fails the run
regardless of what the program printed about itself:

- **`!! OUTSIDE TOUCHED`** — the harness keeps a real directory outside the root
  holding a sentinel file, records its full listing and the sentinel's checksum
  before the run, and re-checks after every line. Any change is an escape. This
  is the check that does not depend on the program telling the truth.
- **`!! MARKER APPEARED`** — several REFUSED lines use a command that would
  create a file in a place the harness watches. If that file exists, the runner
  started something it said it refused.

A line reading `ok` and a line reading `FAIL` differ by one printed word, so a
run cannot be checked by eye for correctness — only for the absence of that
word. Reading the script is what establishes that the right lines ran. Same
caveat as Phases 1, 1b and 2a; stated again because it keeps being worth stating.

---

## Fixtures

The harness builds these itself, by hand, with `mkdir` and `printf` — it does
**not** call another crate's fixture mode, so 2b's gate does not depend on 2a's
binary still behaving.

Inside a throwaway root (`/tmp/atrium-2b-root` by default):

```
home/documents/notes.txt        ("hi")
home/documents/sub/             (empty dir)
home/work/                      (empty dir, the cwd for most lines)
trap/escape -> <outside>        (a link out, used by §G only)
afile.txt                       (a plain file, used as a bad cwd)
```

Outside it (`/tmp/atrium-2b-outside`), created by the harness:

```
sentinel.txt
sub/keep.txt
```

**`HOME` and `TMPDIR` point at the root** (§D), so a line that writes
`$HOME/...` or `$TMPDIR/...` must land inside the root. The harness checks that
by looking inside the root and inside the outside directory — both directions,
because "it did not escape" alone is passed by a command that did nothing.

---

## A. Where the command starts — the whole of the phase's promise

Every line here is `RUNS`, and each is checked on disk, not by reading output.

| # | What it must do |
|---|---|
| A.1 | `sh -c 'pwd'` with cwd `/home/work` prints the **real** root's `home/work` directory. The runner sets the child's working directory to exactly what the virtual path resolves to. |
| A.2 | `sh -c 'touch relative.txt'` with cwd `/home/work` creates `<root>/home/work/relative.txt`. A relative path in a command lands inside the root, because the command started inside it. |
| A.3 | `sh -c 'mkdir -p a/b && touch a/b/deep.txt'` with cwd `/home/work` creates the whole chain inside the root. |
| A.4 | `sh -c 'pwd'` with cwd `/` prints the **root itself** — the virtual `/` is the environment root, not the host root. |
| A.5 | `sh -c 'touch ../sibling.txt'` with cwd `/home/work` creates `<root>/home/sibling.txt` — `..` from inside the root is still inside the root. |
| A.6 | A cwd deeper than one level (`/home/documents/sub`) works the same way. |
| A.7 | `sh -c 'pwd'` with cwd `/home/documents` prints the root's `home/documents`, **not** the host's. |

**A.8 — the disclosure this phase cannot prevent, recorded.** The output of A.1
and A.4 contains the **real on-disk path of the root**. That is the command's own
output and 2b passes it through unaltered (§H.4). Consequence, stated so it is
not a surprise: **under 2b a command can discover where the sandbox lives, and so
can the agent driving it.** Under 2e the command's view of the filesystem has the
root at `/`, so there is no real path for it to print. This is a requirement on
2e, not a defect in 2b.

---

## B. Refusals — nothing starts at all

Every line here is `REFUSED`. Each command is written to create a marker file, so
"it refused" is verified by the marker's absence, not by the runner's word.

A refusal must **name the virtual path and the specific thing that was wrong** —
the `..` step, the symlink, the missing directory, the file-where-a-directory-was
-expected — and must not name the real path (§H.1).

| # | cwd given | Must refuse because |
|---|---|---|
| B.1 | `/home/../../work` | the `..` step climbs above the environment root |
| B.2 | `/nonexistent/work` | the directory does not exist — a command cannot start somewhere that is not there |
| B.3 | `/afile.txt` | it is a file, not a directory |
| B.4 | `/home/documents/notes.txt` | same, one level deeper |
| B.5 | *(empty)* | an empty path is not a place |
| B.6 | `home/work` | a relative path as a cwd — the runner does not silently root it |
| B.7 | `/outside-link` | the link's target is outside the environment root |
| B.8 | `/link-to-nothing-out/..` | the degenerate-tail case Phase 1b ruled on: the target is outside, so it rejects |

**B.9** — every refusal above must leave the filesystem **byte-identical**: no
marker file inside the root, none outside, and the root's listing unchanged from
before the line. A runner that starts the process and lets it fail is a failure.

**B.10** — `REFUSED` for a bad cwd must not be reachable by the command's own
exit code. A command that would exit non-zero for its own reasons must still be
`RUNS`, not confused with a refusal.

---

## C. Output and exit code come back unaltered

| # | What it must do |
|---|---|
| C.1 | stdout only: `echo hi` → stdout is exactly `hi\n`, stderr empty, exit 0. |
| C.2 | stderr only: `sh -c 'echo oops >&2'` → stderr `oops\n`, stdout empty, exit 0. |
| C.3 | both, distinctly: stdout has one line, stderr has another, **and they are not merged** — each stream keeps its own bytes and order. |
| C.4 | exit code preserved: `exit 0`, `exit 1`, `exit 42`, `exit 255` all reported exactly. |
| C.5 | `exit 127` (command not found) is reported as the exit code, not as a runner error. |
| C.6 | **killed by a signal is not exit 0.** `sh -c 'kill -9 $$'` must be reported as killed by signal 9, distinctly from any exit code. A runner that reports 0 here would call a crash a success. |
| C.7 | empty output: a command writing nothing gives zero bytes on both streams, not an error. |
| C.8 | no trailing newline: `printf x` → stdout is exactly one byte `x`. |
| C.9 | **binary output is not mangled**: a command emitting all 256 byte values (including NUL and bytes that are not valid UTF-8) comes back as exactly those bytes, same length. Bytes are bytes. |
| C.10 | arguments survive intact: a command given arguments **containing spaces, quotes and a `$`** receives them unchanged — the runner is not a quoting layer that rewrites argv. |
| C.11 | ordering within a stream: a command printing 100 numbered lines returns them in order. |

---

## D. What the command is handed — the environment

**D.1 — the user's environment is not passed to the command.** The harness
exports a canary variable (`ATRIUM_2B_CANARY=<value>`) into its own environment
before running the runner, so a leaked environment is detectable by name. Inside
the command, `env` must **not** contain it. This is the line that would catch the
real defect: this machine's own `.env` holds API keys, and an inherited
environment hands every one of them to any command.

| # | What it must do |
|---|---|
| D.1 | the canary is absent from the command's environment |
| D.2 | `env` shows only the runner's own small, fixed set — nothing else from the user's session |
| D.3 | `PATH` is set and usable: `sh -c 'ls /'` and plain `ls` both resolve and run |
| D.4 | `HOME` is set and **points inside the root**: `sh -c 'touch "$HOME/homefile"'` creates a file inside the root and nothing outside |
| D.5 | `TMPDIR` likewise points inside the root: a command writing `"$TMPDIR/tmpfile"` lands inside |
| D.6 | the environment is *fixed*, not inherited: running the same command twice from differently-set outer environments gives the same child environment |
| D.7 | the runner's own settings are visible and named (a line the harness can read), not secret — so "which environment did it get" is answerable by looking |

---

## E. What the command's input is

| # | What it must do |
|---|---|
| E.1 | a command that reads stdin (`sh -c 'cat'`) **returns immediately** with EOF and does not hang. The child's input is not connected to the user's terminal. |
| E.2 | a command reading stdin gets no bytes, not garbage: `cat` produces empty output on both streams. |
| E.3 | a command that would *wait* for input (`sh -c 'read x; echo got:$x'`) does not hang the runner. |

---

## F. The runner cannot be hung or flooded

| # | What it must do |
|---|---|
| F.1 | a command that never exits is stopped at the runner's limit, reported as **timed out**, distinctly from an exit code — never reported as exit 0, never left hanging. |
| F.2 | after F.1 the child is **gone**: no process from that line is still running when the harness checks, and no orphan is left behind. |
| F.3 | a command flooding stdout with far more than the capture limit is **capped**, and the cap is **reported** — the result says it was truncated. Silent truncation is a failure, because it would look like a small command. |
| F.4 | flooding **stderr** is handled the same way — a cap on one stream does not rely on the other being small. |
| F.5 | flooding both at once does not deadlock: the runner must not stop reading one pipe while it waits on the other. This is the classic failure of a naive implementation and it looks exactly like a hang. |
| F.6 | a command that writes a lot and *then* exits normally is still reported with its correct exit code. |

---

## G. The hole 2b does not close — marked, for Muffin's eyes

Every line here is `HOLE`. It is **expected to succeed at reaching outside the
root**, because 2b has no cage. It is checked and printed so that a passing 2b
run cannot be misread as confinement. These are the lines 2e must invert.

| # | What it shows |
|---|---|
| G.1 | `sh -c 'cd / && pwd'` prints the **host's** `/`. The command left the environment root by changing directory. |
| G.2 | `sh -c 'ls /'` lists the **host's** root directory. |
| G.3 | `sh -c 'cat /etc/passwd \| head -1'` reads a real host file. Nothing stops it. |
| G.4 | `sh -c 'touch /tmp/atrium-2b-hole-marker'` writes to the real `/tmp`. The harness creates this marker's path, **confirms it appears**, deletes it, and treats its appearance as the expected result of the line — not as an escape, because it is not the outside sentinel directory. |
| G.5 | a command run with cwd `/home/work` can still reach outside by absolute path in the same breath: `sh -c 'touch /tmp/atrium-2b-hole-marker2; touch inside.txt'` creates both, one inside and one on the host. |

**G.6** — the outside sentinel directory must remain untouched **even by §G**.
These lines are pointed at a separate, disposable `/tmp` path precisely so that
the escape detector stays meaningful: if the detector were tripped by the
deliberate hole, it would stop distinguishing a designed hole from an accident.

---

## H. Our messages, and the command's output — two different things

**H.1** — every refusal names the **virtual** path and the failing step. It never
prints the environment root's real location, and never prints the outside
directory's location. This is 2a's §H discipline, re-applied: it is about *the
runner's own sentences*.

**H.2** — no line of the harness output may contain either real path, **except
A.1/A.4 and §G**, where the real path is the command's own output and is the
subject of the line. The harness marks those lines so this is not a silent
exemption.

**H.3** — a refusal's reason must be a sentence a person can judge without a
security background — the same requirement Phase 1's gate rests on.

**H.4 — the runner does not rewrite command output.** A command whose output
contains the root's real path has that output returned **unaltered**. Redacting
it would corrupt data (a build tool's output, a checksum, a diff) and would be
the runner lying about what happened. Stated as a requirement, not an oversight:
the leak here is real and it is 2e's to close by removing the path from the
command's *view*, not by editing its words.

**H.5** — a debug flag may reveal real paths for the runner's own debugging, and
it is **off by default** (same pattern as 2a).

---

## I. How the harness checks — independence

**I.1** — the escape check looks at the disk (`ls -laR` of the outside directory
plus the sentinel's checksum) before the run, after every line, and at the end. It
never consults anything the runner says about itself.

**I.2** — every `RUNS`-with-an-effect line is confirmed **positively**: the file
it should have created exists inside the root, at its real location. "Nothing
escaped" is not sufficient — a runner that does nothing also escapes nothing.

**I.3** — every `REFUSED` line is confirmed by the **absence** of the marker the
command would have created, checked inside the root and outside it.

**I.4** — exit codes are compared against what the shell itself produces for the
same command, run by the harness directly, not against a number the test author
remembered.

**I.5** — the harness itself must be **proven able to fail**: break one
expectation, watch it report `FAIL` and exit non-zero, revert. A harness that has
never failed is not evidence.

---

## J. The boring half — ordinary commands must simply work

| # | What it must do |
|---|---|
| J.1 | `echo hi` runs. |
| J.2 | `ls` of a real directory inside the root lists the right entries. |
| J.3 | `mkdir newdir` then `pwd` shows the command is still where it started. |
| J.4 | a pipeline works: `printf 'a\nb\n' \| wc -l` gives `2`. |
| J.5 | a command that exits non-zero is not special-cased: `exit 3` is `RUNS` with exit 3. |
| J.6 | `sh -c` with a compound command (`cd sub && touch here.txt`) works, and the file lands in `sub`. |
| J.7 | a command touching a path inside the root that does not exist yet still works — 2b does not re-implement the resolver's checks on the command's own arguments (`AGENT-RULES.md` §6). |

---

## K. Open items this list raises

Written down, not built (`AGENT-RULES.md` §4).

1. **The safety stop is not the kill switch.** §F's limit is a default guard so
   the runner cannot be hung; the user-facing kill switch is `DESIGN.md` §7.1 /
   Phase 4. No decision here about what the limit *is* — a value, revisitable,
   and it should not be mistaken for control over a running task.
2. **The environment is a channel the path model cannot see.** §D closes the
   obvious half (do not inherit the user's environment). What the command *should*
   be able to see — network, other processes, the clock, device files — is 2e's,
   together with the filesystem cage. Recorded here so it is not lost.
3. **`HOME`/`TMPDIR` pointing at the root is a v1 simplification.** It is correct
   for containment and needs no directory to be created. Whether a real `/tmp`
   inside the environment should be its own directory is a product decision
   (Phase 6+), not 2b's.
4. **Which shell a terminal eventually uses.** `run` takes a program and
   arguments; a shell line is `sh -c '<line>'`. v1's terminal will probably want
   the user's own shell, which is a one-line change in one place. Not decided
   here, and not 2b's to decide.
5. **A command can learn the sandbox's real path** (§A.8, §H.4). 2e's to close.
6. **A command's own children.** Whether a daemonised process that outlives the
   command is killed or left running is the implementer's call; what is **not**
   optional is writing down which it is (`README.md` + report). §N.9. A survivor
   per task is a leak that only shows up as a slow machine.
7. **What a non-UTF-8 argument even means.** If the API takes `String`, such a
   value cannot be expressed and the question is answered by the type; if it takes
   `OsString`, it can, and the runner must not panic. §N.17. State which, do not
   leave it as "untested".

---

## N. Lines the blind list added — the mechanisms this list had missed

**Added 14 Sep 2026, after the code was dispatched.** The blind second list
(`blind-attack-list-2b.md`, 62 items, one tool call, no project files read) was
compared against this file **by mechanism, not by text** — a literal diff is
useless here, because two lists describing the same attack with different fixture
names look entirely different, and two lists using the same words can be testing
different things. This is the rule that was learned on Phase 1b's blind list.

**Read this section as the agent's extension, not as part of the phase's original
gate.** A–K were written before the code; N was written while the code was being
built. Same status as Phase 1b's §M and Phase 2a's §N.

**What the blind list did not find:** it did not find a hole of the hard-link
class (the thing a first list is *for*). Everything it contributed is a
mechanism-level gap in the same promise, not a new promise. That is the honest
result, and it is recorded as such.

### N.1 The program itself can fail to start — a different refusal from a bad cwd

Every `REFUSED` line in §B is about the **directory**. Nothing in A–K tests the
**program**. Each of these must produce its own truthful error, and the three
must be distinguishable from each other and from a child that actually ran and
exited 127.

| # | What it must do |
|---|---|
| N.1 | a program that does not exist → refused, named as not found, and nothing ran (checked by marker absence) |
| N.2 | a program that exists but is not executable → its own refusal, naming that, not "not found" |
| N.3 | a directory given as the program → its own refusal, naming that |
| N.4 | none of the above is reported as exit code 127 — that code means a *child* ran and could not find something |

### N.2 The time limit must hold against a child that refuses to die

§F tests that a *cooperative* sleeper is stopped. These test the adversarial case.

| # | What it must do |
|---|---|
| N.5 | a child that ignores the polite signal (`trap '' TERM; sleep 999`) is still **stopped at the limit**, reported timed out, and gone afterwards. If the runner sends only one signal and waits, this line hangs — which is exactly why it is here. |
| N.6 | the limit is measured in time, not in activity: a child emitting one byte per second past the limit is still stopped at the limit. Output must not extend the deadline. |
| N.7 | a child that exits successfully in under a millisecond is reported as exit 0 — never as a timeout or a spawn failure. The race is real and it is invisible when it does not fire. |

### N.3 A child's own children — the runner must not be held open by them

This is the mechanism A–K missed entirely, and it is the most likely way this
phase produces a hang in ordinary use (a build tool, a daemon, a background job).

| # | What it must do |
|---|---|
| N.8 | a command that backgrounds a process which inherits its output pipes (`sh -c '(sleep 30) &'`) must be reported **when the command itself exits** — the runner must not sit reading pipes until the grandchild closes them, and must not hang past the limit. This is the classic pipe-EOF trap. |
| N.9 | a command that daemonises and exits 0 leaves a process behind that the runner does not own. Whether the runner kills the group or leaves it running is **the implementer's call, but it must be written down in `README.md` and in the report** — an undocumented survivor is a leak that accumulates with every task. (§K.6.) |

### N.4 The cap and the streams at their edges

| # | What it must do |
|---|---|
| N.10 | the cap is exact at its boundary: a command producing `cap-1`, `cap` and `cap+1` bytes reports the right truncation flag each time. No off-by-one in either direction. |
| N.11 | a command that closes its **own** stdout (`exec 1>&-`) or stderr before writing → the runner does not hang and does not misreport; the other stream's bytes are still captured intact. |
| N.12 | **a command's bytes must not be able to forge the runner's or the harness's own status words.** A command that prints text identical to a passing line, a `FAIL`, or a result marker must appear as **its output**, structurally distinct from the runner's own report. This matters here more than anywhere: the gate is a person reading lines, and a command that can print a believable `ok` line attacks the gate itself. The runner must keep status and captured bytes in separate places, and the harness must never let captured bytes sit where a verdict is read from. |
| N.13 | `isatty` on the child's stdin is **false** — confirmed by the command itself, not inferred from §E. A terminal here would mean the child can talk to the user's keyboard. |

### N.5 Arguments: what reaches the program, exactly

| # | What it must do |
|---|---|
| N.14 | an argument beginning with `-`, including the runner's own flag names (e.g. `--timeout`), reaches the **program** and is not swallowed by the runner's argument parsing. Requirement 8 of §9.3 is easy to satisfy for ordinary arguments and easy to break for these. |
| N.15 | zero arguments, and an argument that is the empty string, arrive as exactly that — checked by argv count from a small script, not from output formatting. |
| N.16 | a program name containing a path (`./script.sh`, `/home/bin/tool`, `../tool`) is passed to the operating system as written and interpreted **relative to the child's cwd** — it is not resolved, rewritten or checked by us (it is the program, not a path of ours). `../tool` must not become anything else. |
| N.17 | a non-UTF-8 byte in an argument, and in the program name, does not panic the runner. Whether such a value is even **representable** depends on the API's string type — see §K.7. If it is not representable, that is the answer, and it must be stated rather than left as "untested". |

### N.6 The cwd spelled differently

| # | What it must do |
|---|---|
| N.18 | cwd `/home//work/./` and cwd `/home/work/` resolve to the **same** directory as `/home/work`, and the command starts there. The resolver handles the normalisation; this line proves the runner passes the resolved path rather than the raw string to the operating system. |

### N.7 Recorded, not gate lines — the classes this phase does not close

Written down so they are not discovered later as surprises. None blocks 2b.

- **N.19 — TOCTOU.** The cwd is resolved to a path and then used to start a
  process; between those two moments it can be replaced. Not closable with the
  current resolver, and the same class Phase 2a recorded. Belongs with 2e or a
  resolution-hardening phase — not to be quietly fixed here.
- **N.20 — process-group interference.** Another command on this machine, same
  user, can kill or outlive a command here (observed behaviour class, not a
  defect). The runner's only obligation is a truthful status; containment is
  2e's.
- **N.21 — resource exhaustion.** A command can consume the machine's CPU, memory
  or file descriptors. Not addressed in 2b; 2e's boundary.
- **N.22 — concurrency semantics.** Two runners writing the same file, whether the
  output cap is per-run or shared, and whether identical input gives identical
  output are all **undecided**. No requirement exists; recorded rather than
  invented.
- **N.23 — a command can write anywhere in the environment root, including
  wherever the app itself keeps state.** Nothing in 2b separates the sandbox's
  content from the app's own files. Needs deciding before anything keeps durable
  state inside the root. (Not a 2b job — Phase 2c keeps state and is the first
  phase exposed to it.)

---

## Independence

`attack-list-2b.md` was written by the head that also wrote the build brief and
will verify the result, so the two share assumptions. Before the phase is
accepted, a **blind second list** must come from a subagent that has seen **none**
of: the `shell/` source, the brief, this file, `attack-list.md`,
`attack-list-1b.md` or `attack-list-2a.md`. It gets only a plain description of
what running commands must guarantee and is asked what it would try. Every novel
line it produces is run against the built code. The blind lists for Phases 1, 1b
and 2a each found lines the first list had missed — for 2a it found the hard-link
hole, which the first list had missed entirely. This is a gate, not a courtesy.

---

## How this list was derived

From `BUILD-PLAN.md` §2b and its correction, `DESIGN.md` §3.3 (the shell is real;
"working directory confined" is not confinement) and §3.1 (the agent never learns
the real path — which a real command can simply print), and from what Phases 1,
1b and 2a actually taught: that a path check cannot see a hard link, that a
refusal is only a refusal if nothing happened, that a passing harness line and a
failing one differ by one word, and that the strongest check looks at the disk
rather than at the program's own report.
