# Atrium — Status

**The only file that says where this project actually is.**

Two sections, and the distinction between them is the point.

- **Verified hands-on** — Muffin ran it and saw it work. Trustworthy.
- **Agent-reported, unverified** — an agent said it worked. Not evidence.

**Agents append to the second section only.** Only Muffin moves an item up. An
agent must never move an entry, including its own from a previous session.

---

## Current phase

**Phase 2c — snapshot and restore. VERIFIED HANDS-ON by Muffin, 14 Sep 2026.
Signed off.** See "Verified hands-on" below for what he ran and what he saw.
**111 lines, zero failures, 0 holes** — with the outside directory, host
`/etc/passwd` and the store all checked unchanged after every line. It is the
undo: a snapshot copied to a store **outside** the environment, restorable, and
refused if altered. **What it does not do: cage a command (2e), and protect the
store from a same-user command** — that boundary is 2e's too. **Nothing
autonomous runs before undo exists, and the undo now exists.**

**Phase 2b — the real shell. VERIFIED HANDS-ON by Muffin, 14 Sep 2026. Signed
off.** See "Verified hands-on" below for what he ran and what he saw. **78 lines,
zero failures, 5 holes demonstrated on purpose** (the command can still reach the
host — that is 2e). It runs a real command with its working directory resolved
through the resolver, a fixed non-inherited environment, stdin not connected to a
terminal, a time limit, an output cap and a three-way status. **Signing 2b off
means the promise it makes holds — where a command starts and what it is handed —
not that a command is confined.**

**Phase 2a — file operations. VERIFIED HANDS-ON by Muffin, 14 Sep 2026. Signed
off.** See "Verified hands-on" below for what he ran and what he saw. 64 lines,
zero failures, nothing outside the environment root touched, no host path
disclosed. The file-operation layer holds: create, read, write, move, delete and
list now act on the environment through the Phase 1/1b resolver.

**Phase 2e — strong confinement for `run commands` — is a required phase, after
2d and before Phase 5 (the agent loop).** Found while starting 2b: "working
directory confined" confines only the starting directory, so a real command can
still reach the host. The plan's own Phase 2 verification block already demanded
"confirm it cannot reach the host", so the cage is not optional — see the 2e
entry under "Agent-reported", and `DECISIONS.md`. **Nothing autonomous may
exercise the shell until 2e passes.**

**Phase 1b — Non-existent-path resolution. VERIFIED HANDS-ON by Muffin, 13 Sep
2026. Signed off.** See "Verified hands-on" below for what he ran and what he
saw. This is the phase that lets anything be created at all: Phase 1 could only
resolve paths that already existed.

**Phase 1 — sandbox path resolution is signed off** (13 Sep 2026), and Phase 1b
extends it. Together they are the whole of the resolver as it stands.

**Phase 0 is CLOSED.** 0a and 0b confirmed by user in chat. **0c verified
hands-on by Muffin, 13 Sep 2026** — see "Verified hands-on" below.

**Next: Phase 2e — strong confinement for `run commands`.** Phase 2d (the watcher)
was built 15 Sep 2026 and awaits Muffin's hands-on gate; see "Agent-reported" below
and `phase-2d-evidence.txt`. **2e is not optional and is the last gate before Phase
5** — nothing autonomous runs through a shell that can reach the host, and nothing
autonomous may exercise the shell until 2e passes.

**0c blocks Phase 9 only — not Phase 2, not 2b.** That is BUILD-PLAN rule 2's
worked example, and it has been re-argued from scratch twice; read it there
rather than re-deriving it.

---

## Verified hands-on

### Phase 0a and 0b — bridge and local MCP spikes (Muffin, confirmed in chat)

**Moved here 13 Sep 2026 at Muffin's explicit instruction.** These two were
confirmed by chat message, not by a re-run: 0a in the session before 11 Sep
2026, 0b by a "go" reply after he read the 0b report. That is a weaker form of
evidence than the other entries in this section, which record something he ran
and watched — recorded plainly rather than dressed up as a hands-on pass.

They sat in "Agent-reported" for two days after he had already confirmed them,
because an agent may not move an entry (`AGENT-RULES.md` §10) and nobody asked
him to. Phase 0 was recorded as closed while two of its three items were still
below the line.

### Phase 0c — MCP over remote transport (Muffin, 13 Sep 2026)

**This is the 0c sign-off. Phase 0 is closed.**

**What Muffin ran:** `cargo run` in `spikes/0c-mcp-http/`, with a fine-grained
GitHub PAT (read-only, public repos, 30-day expiry) exported in his shell.

**What he saw:** initialize handshake succeeded against
`https://api.githubcopilot.com/mcp/readonly`; **27 read-only tools listed**;
`call_tool(get_me)` returned his account JSON; `is_error: None`; exit 0.

**One bug found and fixed on the way, by Claude.** The first run failed with
HTTP 400, `bad request: Authorization header is badly formatted`. Cause:
`src/main.rs` set `config.auth_header = Some(format!("Bearer {}", token))`,
but rmcp 3.3.0 adds the prefix itself — its doc comment on
`StreamableHttpClientTransportConfig::auth_header` reads "A bearer token
without the `Bearer ` prefix". The wire header was therefore
`Bearer Bearer <token>`. Fixed to `config.auth_header = Some(token)`, with the
reason recorded in a comment at the line. The token itself was verified clean
first (93 chars, no non-alphanumeric characters) so the fault was isolated to
our code before anything was changed.

**What this retires:** the last open item in Phase 0, and the only remaining
unknown in 0c. Remote transport was already proven; auth is now proven too.

**Follow-up, not blocking:** `spikes/0c-mcp-http/src/bin/probe_noauth.rs` was a
throwaway no-auth probe and can be deleted. The PAT has no further use in this
project and can be revoked.

### Phase 1 — sandbox path resolution (Muffin, 13 Sep 2026)

**This is the Phase 1 sign-off. The gate is passed.**

**What Muffin ran:** `crates/resolver/hand-test.sh` — the full attack list, sections
A–I, 91 lines against a throwaway environment root. His result: **24 accepts,
67 rejects, zero failures.**

**What he then checked himself,** going beyond the script:

- **His own paths,** not only the list's.
- **A symlink-plus-escape combination** — a shortcut pointing outside the
  sealed area, with an escape appended.
- **A path that climbs out of the root and back in.**

**All behaved correctly, and every rejection named the specific step that
failed** — which was the requirement: a rejection returns a reason, not a bare
failure.

**Not covered by this pass, and cannot be:** the **three NUL-byte paths**
(sections F and H). A NUL byte is a zero byte, and a command-line argument is
terminated by a zero byte by definition, so no terminal can pass one through.
These are covered by the automated test suite only —
`cargo test`, `section_f_unicode_and_lookalikes` and
`section_h_nasty_combinations`. Recorded in `attack-list.md` §F rather than
left implicit.

**Recorded at Muffin's instruction.** Agents may not move entries into this
section (`AGENT-RULES.md` §10); this entry was written because he directed the
move in chat on 13 Sep 2026, not on an agent's judgement.

**Consequence:** the sandbox holds. Phase 2 assumes it. **One escape would have
stopped the project** — there was none.

### Phase 1b — non-existent-path resolution (Muffin, 13 Sep 2026)

**This is the Phase 1b sign-off. The gate is passed.**

**What Muffin ran:** `crates/resolver/hand-test-1b.sh` — the full Phase 1b attack list,
sections A–M, 92 lines against a throwaway environment root. His result, quoted
from chat: **`accepts: 43   rejects: 49`** and
**`hand-test-1b: every line behaved as required`** (exit 0). The final line only
prints when zero lines failed and no ACCEPT landed outside the root — an escape
is flagged by the script independently of what the resolver reported.

**Why this phase exists:** nothing can ever be created if an absent path cannot
be resolved. Absent-and-inside now accepts; every escape still rejects with a
reason naming the specific step.

**What the passing 92 lines do and do not prove.** A passing line and a failing
line differ by one printed word, so a run cannot be checked by eye for
correctness — only for the absence of that word. Reading the script, not just
its output, is what establishes that the right lines ran. The attack list was
written **before** the code, as the phase requires. It was extended by this
session (§M, the blind list's lines that no existing section covered) — that
extension is the agent's, not part of the phase's original gate. Both stated
plainly rather than presented as a clean hand-off.

**On NUL bytes — one thing the agent got wrong, corrected here.** The NUL lines
in the attack lists can never be passed as a command-line argument (a zero byte
terminates it), so they run in the test suite only. The agent first recorded
that Phase 1b's list carries the same gap as Phase 1's; it does not —
`attack-list-1b.md` has no NUL lines at all. What 1b added instead is a fix that
makes the gap smaller: `resolve()` rejects any path containing a NUL byte with
its own named reason (`NullByte`, naming NUL) **before it touches the filesystem
and before either mode of the walk**, so the verdict is the same whether the path
exists or not, and an OS error can never be what rejects it. `%00` in `§M.2` is
three literal characters, not a NUL byte.

**Recorded at Muffin's report of his own run** (`AGENT-RULES.md` §10 — agents
may not move entries; the gate is his hands-on pass).

**Consequence:** resolution holds for paths that do not exist yet, so Phase 2 may
now create files. **Phase 2 is unblocked and is the next build.**

### Phase 2a — file operations (Muffin, 14 Sep 2026)

**This is the Phase 2a sign-off. The gate is passed.**

**What Muffin ran:** `crates/fileops/hand-test-2a.sh` — the Phase 2a attack list, 64
lines against a throwaway environment root and a separate outside directory. His
result, quoted from chat: **`lines run: 64   failures: 0`** and
**`hand-test-2a: every line behaved as required`** (exit 0).

He also pasted the two integrity prints at the end of the run, verbatim:

```
out sentinel after:   3783831074 9 /tmp/atrium-2a-outside/sentinel.txt
/etc/passwd after:    539412550 2943 /etc/passwd 1783778225
```

**What those two lines prove, and how.** They are prints, not verdicts — the
script captures both values *before* the run and fails the run if either changed,
so `failures: 0` is what asserts they are unchanged. Checked parent-side after
his paste: the host `/etc/passwd` right now is still `539412550 2943` with mtime
`1783778225`, identical to his line, and `3783831074 9` is exactly the cksum of
`printf 'sentinel\n'` — i.e. the outside sentinel was still present and unmodified
while the harness was running its own cleanup, so the `MISSING` branch never fired
(nothing outside the root was ever deleted). His run is consistent with everything
else on disk: the code he ran carries mtimes all earlier than his run, so the
binary he watched is the binary on disk now.

**Why this phase exists:** the hands. Every create, read, write, move, delete and
list goes through the Phase 1/1b resolver, so the sandbox is the thing that can
act on the environment rather than only name paths inside it.

**What the passing 64 lines do and do not prove.** A passing line and a failing
line differ by one printed word, so a run cannot be checked by eye for
correctness — only for the absence of that word. Reading the script, not just its
output, is what establishes that the right lines ran.

**Built by a subagent that was cut off — stated plainly, not presented as a clean
hand-off.** The Phase 2a build child died mid-run on an account-level usage limit
with no report; the parent session finished, corrected and re-ran everything, and
nothing the child claimed was treated as evidence. Two things the parent session
changed after the child stopped are worth naming, because both are the parent's
work rather than the phase's original shape:

1. **A disclosure defect the child's code shipped.** Refusal reasons passed the
   resolver's own text through verbatim, which printed the environment root's
   **real path on disk** in 13 of the harness's lines — the sandbox's location,
   which `DESIGN.md` §3.1 says the sandbox must never learn. Fixed by rewording
   from the resolver's structured error fields, with **no change to the resolver**.
   A regression test guards it and was proven non-vacuous by reintroducing the
   defect and watching the test fail.
2. **An error in the attack list itself** (`§C.12`), written before the code: it
   demanded that writing beneath a dangling symlink succeed, which contradicts
   write-never-creates-parents. Probed, split into `C.12` (must refuse, ordinary
   missing-parent reason) and `C.13` (writing *through* the link must work — the
   case Phase 1b exists for), and the harness corrected.

The attack list was written **before** the code, as the phase requires. The list
was also extended by this session — `§N` hard links, `§M` blind-list lines — and
that extension is the agent's, not part of the phase's original gate. Both stated
rather than smoothed over.

**The containment promise 2a actually keeps, and what it does not.** Observed
result of the run: **zero escapes, zero outside files touched, no host path
disclosed.** But the honest statement of the guarantee is narrow: *Atrium's own
file operations cannot leave the root* — **not** "nothing can". Two limits are
recorded, not hidden:

- **Hard links (§N).** A hard link placed inside the root pointing at a file
  outside it let the sandbox read *and overwrite* that file. No path check can
  see this — a hard link is a second name for one file, not a path. This is not a
  2a bug and 2a cannot fix it; it is closed by requiring the environment root to
  be its own mount (a deployment constraint), which `DESIGN.md` §3.1 now states.
- **The symlink-object limitation (§K).** A symlink is always followed, so a link
  whose target is outside cannot be addressed as an object at all; deleting an
  inside link removes the target and leaves the link dangling. Safe — nothing
  outside is touched — but wrong. Deferred deliberately: fixing it needs a
  non-following resolver variant, which is a stop-and-ask item with its own
  verification, not a 2a change.

**Recorded at Muffin's report of his own run** (`AGENT-RULES.md` §10 — agents may
not move entries; the gate is his hands-on pass).

**Consequence:** the file-operation layer holds, so Phase 2b (the real shell) is
the next build. 2b is also where the shell's own boundary turned out not to exist
— see the 2e entry under "Agent-reported"; **strong confinement is its own phase
and gates the agent loop.**

> *(14 Sep 2026, later the same day: 2b was then built and awaits its hands-on
> gate — see the 2b entry under "Agent-reported". The paragraph above is left as
> written because it was the state when Muffin signed 2a off.)

### Phase 2b — the real shell (Muffin, 14 Sep 2026)

**This is the Phase 2b sign-off. The gate is passed.**

**What Muffin ran:** `crates/shell/hand-test-2b.sh` — the Phase 2b attack list, 78 lines
against a throwaway environment root and a separate outside directory. His
result, quoted from chat, verbatim:

```
lines run: 78   failures: 0   holes demonstrated (expected, 2e's): 5
hand-test-2b: every line behaved as required (the 5 HOLE lines reached the host by design)
```

**Those are the harness's own last two prints, and they were checked before being
recorded here — not taken on trust.** Parent-side, after his paste: (a) the
exit-0 line is **unreachable unless `FAILS=0`**, and `FAILS` also counts every
change to the outside directory and to host `/etc/passwd` — so a green line here
means nothing outside the root moved; (b) the "78" was traced at runtime with
`bash -x`: `OKS` reaches **exactly 78**, so the count is real checks and not a
padded number; (c) running the same harness here produced output **byte-identical**
to his paste, which is expected — the output is deterministic for a given build;
(d) **no escape**: the outside sentinel (`3783831074 9`), `sub/keep.txt`
(`704589907 5`) and host `/etc/passwd` (`539412550 2943`, mtime `1783778225`) were
all intact. His run is consistent with everything on disk.

**One thing was not verified, stated rather than smoothed over.** The same
159-byte paste appears **twice** in the stored transcript (19:30:32 and 19:40:30),
and the first arrived in the same second as a replayed delegation notification. A
re-delivery could not be told apart from a second send. The content itself is
consistent with a genuine clean run, and with the tail-paste Muffin used for the
Phase 1b and 2a gates. The gate rests on his report; this is that report as it
reached the session.

**What the passing 78 lines do and do not prove.** A passing line and a failing
line differ by one printed word, so a run cannot be checked by eye for
correctness — only for the absence of that word. Reading the script is what
establishes that the right lines ran. §A–§K were written before the code; §N was
written **while** the code was being built, so the build child never saw it.

**Why this phase exists:** the ability to run actual commands — `ls`,
`git status`, `python3` — inside the environment. Every later phase needs it.

**The hole, demonstrated on purpose.** 5 of the 78 lines are marked `HOLE` and
reach the host by design: `cd /`, `ls /`, reading a line of the host `/etc/passwd`,
and creating files in the real `/tmp`. **2b guarantees WHERE a command starts and
WHAT it is handed; it does not confine what the command can reach.** Closing that
is Phase 2e, required before the agent loop. The harness shows the hole rather
than letting a green run imply a sandbox.

**A real defect the independent blind list found, and the fix.** A command that
backgrounds a child inheriting its output pipes made the runner block until the
*survivor* ended — past its own limit (observed: 12002 ms against a 2000 ms limit,
blocked for the whole outer guard). Fixed; observed after the fix: **222 ms,
`status=exit 0`**. The survivor is deliberately not killed (that is containment,
2e's), and `README.md` records it. Details and the three proofs in
`phase-2b-evidence.txt`.

**Recorded at Muffin's report of his own run** (`AGENT-RULES.md` §10 — agents may
not move entries; the gate is his hands-on pass).

**Consequence:** the shell holds for the promise it makes, so **Phase 2c
(snapshot and restore) is the next build.** Snapshot/restore gates anything
autonomous — nothing runs on its own before undo exists.

> *(14 Sep 2026, later the same day: 2c was then built and awaits its hands-on
> gate — see the 2c entry under "Agent-reported". The paragraph above is left as
> written because it was the state when Muffin signed 2b off.)*

### Phase 2c — snapshot and restore (Muffin, 14 Sep 2026)

**This is the Phase 2c sign-off. The gate is passed.**

**What Muffin ran:** `crates/snapshot/hand-test-2c.sh` — the Phase 2c attack list, 111
lines against a throwaway environment root, a store outside it, and a separate
outside directory. His result, quoted from chat, verbatim:

```
lines run: 111   failures: 0   holes demonstrated (expected, 2e's): 0
hand-test-2c: every line behaved as required
```

**Those are the harness's own last two prints, and they were checked before being
recorded here — not taken on trust.** Parent-side, after his paste: (a) the
success line is **unreachable unless `FAILS=0`**, and `FAILS` also counts every
change to the outside directory, to host `/etc/passwd`, and to the store during a
refusal, and the disappearance of the root — so a green line means nothing outside
the root moved and no refusal wrote anything; (b) `check_outside "end of run"` runs
**immediately before** the tally, so the final integrity check cannot be skipped by
the line that reports it; (c) `/etc/passwd` right now is still `539412550 2943`
with mtime `1783778225` — byte-identical to the 2a baseline from the day before, so
nothing in the project has touched it; (d) running the same harness here produces
output matching his paste, which is expected — the output is deterministic for a
given build; (e) `=== cleanup ===` and `throwaway directories removed` are harness
lines 623 and 626, so the paste is real script output rather than a retyped summary.

**One thing the paste does not show, stated plainly.** The harness **rebuilds the
binary if any source file is newer than it** (`hand-test-2c.sh` line 42 ff.), and
prints a build line when it does. Neither his paste nor my run showed one, which
means **no rebuild was triggered** — the guard was not exercised in either pass. It
was observed firing earlier in the session, by touching the source and watching the
line print. So the guard is proven by that observation, not by this run.

**What this phase is.** The undo. `snapshot create` copies the whole environment
root — files, directories, links, names, at every depth — into a **store outside
the root**; `snapshot restore` puts the root back and removes anything created
since; `snapshot list` shows what is restorable. A save point taken before a task,
so a task that goes badly can be undone. This is the phase that makes "nothing
autonomous runs before undo exists" true.

**The boundary he has now confirmed, in one line each.** A snapshot is a directory
plus a **completed record**; a create killed halfway leaves a directory that is not
restorable and is **left on disk as evidence**, not tidied away. The record holds
every entry's kind, mode, size and SHA-256, so a snapshot altered after it was
taken is refused **before the environment is touched**, and a same-length tamper is
caught by the checksum where a size check could not. Links are copied as links and
never dereferenced in either direction. The store **must** be outside the root, and
a store inside is refused, not warned about.

**What he signed off, and what he did not.** He signed off the promises this phase
makes. Two things sit outside them and are recorded here so a later session does
not read them into 2c:

- **The store is not protected from a same-user command.** The record detects
  accidental alteration; it is not cryptographic, and the store lives in the same
  user's account. The design works only if the store is genuinely out of the
  agent's reach — **that boundary is 2e's**, not this phase's.
- **Snapshots are not automatic.** "Before every task" is the caller's job in
  Phase 5; 2c provides the ability, not the habit.

Also deliberately not preserved, printed in every run so the limit is visible where
the result is read: modification times, hard links (return as separate files),
ownership, ACLs, extended attributes, sparse layout, directory mtimes. **Access
times are neither preserved nor defended against** — reading a file to hash it can
update its atime; recorded as a limit rather than claimed as neutral (§K.7).

**A real defect the independent blind list found, and the fix — the reason this
round earned its place.** After the harness was 97/97 green and the tests 40/40
green, the blind list (`blind-attack-list-2c.md`, ~70 items, one tool call, no
project files read, sha256 `6b2ad920ac…599d03c` verbatim) supplied six new
mechanisms. One exposed a genuine defect: **a directory recorded as mode 555 made
`restore` fail *after* it had already deleted everything beside it** — the modified
file was gone rather than restored. The undo mechanism caused the loss it exists to
prevent; the worst class of defect in this phase. Fixed with an
`ensure_contents_removable` pre-flight that runs **before the first deletion**, so
a tree that cannot be emptied is refused with nothing changed. Observed after:
exit 0, the file restored, the read-only directory's contents restored. Permanent
regression test `restore_empties_a_read_only_directory_instead_of_failing`;
harness line L.4.

**A second defect, found by measurement rather than by any list.** A 5,000-file
snapshot took 5,873 ms, of which 5,592 ms was re-reading files it had just written
(first read of a fresh file ~1.119 ms against ~0.012 ms for later reads — a
per-file cost, not per-byte). Fixed by hashing each file's bytes as they stream out
and recording what was actually written, which removes the re-read **and**
strengthens the guarantee: the record now attests to the bytes written rather than
to a re-read of them. The churn check it could have lost was kept and gained its
own error, `ChangedWhileCopying`. Observed after: **277 ms**. 20x on the shape
("many small files") a real project has. Full detail in `phase-2c-evidence.txt` §6
and §7.

**What the passing 111 lines do and do not prove.** A passing line and a failing
line differ by one printed word, so a run cannot be checked by eye for correctness
— only for the absence of that word. Reading the script is what establishes that
the right lines ran. §A–§K were written before the code; §L and the §F additions
were written **after** the blind list arrived, each probed by hand before being
written down.

**Recorded at Muffin's report of his own run** (`AGENT-RULES.md` §10 — agents may
not move entries; the gate is his hands-on pass).

**Consequence:** the undo holds, so nothing autonomous now waits on it. **Phase 2d
(the watcher) is the next build**, and **2e — strong confinement — is still
required before Phase 5**, because 2b deliberately leaves a command able to reach
the host. See the 2e entry below.

### Pre-project research

**Flutter renders correctly on the target hardware.** *(Sep 2026)*
Release build, i7-14700KF / RTX 3080 / Nobara / KDE Plasma 6 / Wayland /
2560x1440.

| Load | FPS | Avg frame |
|---|---|---|
| 200 animated squares | 372 | 2.69 ms |
| 1000 animated squares (clean) | ~154–158 | ~6.4 ms |

60fps budget is 16.7 ms — roughly 3x headroom. Native Wayland, no XWayland.
Impeller on OpenGL ES. Zero rendering errors, no flicker, no blank windows, no
resize crash. Separately confirmed that `--headless-metrics` only controlled
stdout metric export and did not disable presentation, so the numbers are valid.

**Excalidraw MCP is browser-dependent and non-persistent.** *(Sep 2026)*
`yctimlin/mcp_excalidraw`, run hands-on:

- Screenshots, PNG/SVG export, viewport control and mermaid conversion all fail
  with `No frontend client connected` unless a browser tab is open. No CLI path
  around it. Headless rendering is only "planned".
- Nothing persists across restart — created an element, killed the server,
  restarted, canvas empty. Snapshots are in-memory too.
- Element CRUD, queries and `.excalidraw` JSON export work headless.
- 26 MCP tools exposed over stdio; canvas server is separate, on
  `127.0.0.1:3000`, REST + WebSocket on the same port.

Consequence: Excalidraw is a **generic** surface in v1, not native.

### Toolchain

**Hermes delegation to kimi-k3 works.** *(Sep 2026)*
Config: `model.default: deepseek-v4.1-flash (delegation.model: kimi-k3)`, on ollama-cloud,
`child_timeout_seconds: 900`.

**This is the single home for live config values** (HANDOFF §6, "one fact, one
home"). Verified live 12 Sep 2026 with `hermes config get model.default` →
`deepseek-v4.1-flash` and `hermes config get delegation.model` → `kimi-k3`.
Note `hermes-recon.md` §1–2 holds older values from 10 Sep 2026 and is marked
stale; do not read a model id from there.

A test subagent completed in 4.22s over 2 API calls, ran `date -Iseconds`
independently (three distinct timestamps confirmed the child executed it, not the
parent), exit code 0, no errors, and no `subagent-timeout-*.log` was created.

**Caveat, RESOLVED:** the installed `delegate_task` (Hermes v0.20.0, 2026.8.3)
exposes six parameters only — `background` (deprecated), `context`, `goal`
(required), `output_schema`, `role`, `tasks`. **There is no `toolsets` and no
`max_iterations`.** The reference-tree docs describe a different version.

Consequences:

- Subagents receive the default toolset. Toolset restriction is not enforceable
  per call. **The gate built in Phase 4 is the real boundary, not the delegation
  harness.**
- `max_iterations` comes from config only (currently 50, for every subagent).
- Delegations run in the **background automatically**; `background` is
  deprecated because it cannot be opted in or out of. Results re-enter the
  conversation when finished. Interrupting the parent is therefore not the
  work-losing hazard the older docs describe.
- `output_schema` is available — use it to force structured subagent reports
  rather than prose.
- Nesting is off (`max_spawn_depth: 1`); `role="orchestrator"` is silently
  forced to `leaf`.

**The post_tool_call build hook does not fire.** *(Sep 2026, audited)*
Registered in config, allowlisted, script correct and working when invoked
directly — and a no-op in practice. Three independent causes: the TUI process
never calls `register_from_config`; `_emit_post_tool_call_hook` discards the
hook's return value; `{"context": ...}` is the `pre_llm_call` payload shape,
not `post_tool_call`'s. `hermes hooks doctor` reports it healthy because it
checks config, allowlist and a synthetic payload, never the live registry.
Removed from config Sep 2026. Full evidence in `pipeline-check.md` §5.

---

## Agent-reported, unverified

### Phase 2d — the filesystem watcher: built in-session by the parent, because the last three build children died at the cap (15 Sep 2026, Hermes session)

**Read this entry as evidence of what was RUN, not as a pass.** Phase 2d is NOT
signed off. The gate below is Muffin's to run. Full command-by-command evidence is
in `phase-2d-evidence.txt`.

**What it is.** A new crate `crates/watcher/` (binary `atrium-watcher`). It watches the
environment root and reports what changed inside it, so the live view cannot go
stale when a real command changes files behind Atrium's own file-operation layer
(`DESIGN.md` §3.3). `watch` prints one record per change; `demo` runs a
self-contained scenario; `fixtures` builds the harness's tree. The spec of record is
`attack-list-2d.md`, written *before* the code, as every phase's has been.

**Built in-session, not delegated — with the observed reason.** The last three build
children died at the 900 s cap with no report: 2a's on an HTTP 429 quota wall, 2b's
on `exit_reason=timeout` after 44 API calls, and 2c was built in-session for the same
reason and needed no salvage pass. The model cap was probed clear before deciding
(`kimi-k3`, HTTP 200, 0.84 s), so the reason is time, not quota. **A change of
method, not of standard:** nothing below is relayed from a child, because there was
none.

**What it does, and the honest scope of it.** It observes and reports. It does not
gate, block, decide or act. Notifications are `Change` records: created, modified,
deleted, moved-from, moved-to, attributed, directory-gone, **overflow**, error. One
watch per directory (the kernel is not recursive — measured). It reports **changes
from the moment `start` returns**; a close that straddles startup is reported
deliberately, and that boundary is pinned by a test rather than left to drift.

**Parent verification, all observed 15 Sep 2026** (real output, not summary):

- `cargo fmt --check` → clean. `cargo build` → exit 0, no warnings.
- `cargo test` → **42 passed, 0 failed** (32 attack-list tests + 10 blind-list
  mechanisms).
- `bash crates/watcher/hand-test-2d.sh` → **rc 0, 16 lines, 0 failures, 0 outside touches,
  0 root disappearances, 0 real-path leaks.**
- **20 consecutive full-suite runs, 0 failures.** Before the fix in the paragraph
  below the suite failed **3 times in 22 runs** — so the flake was real, and its
  disappearance is measured rather than assumed.
- The harness's **rebuild guard was exercised on purpose** (`touch src/lib.rs`, then
  run: it printed its build line and recompiled). 2c recorded that its own guard had
  **never** been exercised in either pass; this one was watched firing.
- **Three negative proofs hold**, each asserting the specific marker rather than a
  non-zero exit (2b lost a pass to exactly that trap): a broken expectation produces
  `[FAIL] A.1` and rc 1; a real change to the outside directory produces
  `!! OUTSIDE TOUCHED` and rc 1; a real path in the change stream produces
  `!! REAL PATH LEAKED` and rc 1.

**Three defects, each found by running something, each fix proven load-bearing by
reintroducing the defect and watching a specific check fail.**

1. **A watched subtree moved *out of the root* kept reporting, naming an in-root
   path — a fabricated change.** Observed: after moving a watched directory outside,
   writes made out there arrived on the old watch descriptors, and a watcher trusting
   its table would report `CREATE /home/documents/sub/deep/host-side.txt` for a file
   living entirely outside the environment. Fixed by re-deriving every path from the
   tree by inode, and removing any watch whose directory is no longer in the root.
   Proven by disabling the re-derivation and watching the test fail with exactly that
   invented record.
2. **A populated subtree moved *into* the root was watched only at its top level.**
   **Found by the independent blind list.** Everything inside was invisible
   **forever** — no later event corrects it, so the view shows a populated folder as
   one empty directory, permanently. Fixed by walking the moved-in subtree. Proven by
   reverting and watching the test fail.
3. **The header line printed the root's real on-disk path** — caught by this phase's
   own harness on its first run as `!! REAL PATH LEAKED`. `DESIGN.md` §3.1 forbids it.
   Fixed: the default output shows `/`; the real path appears only under `--show-real`.

**A flake, chased to the kernel, and the honest limit that remains.** A test failed
**once in 8** suite runs, then **3 in 22**, reporting a file modified in a window
where nothing wrote it. Measured, not assumed: the file's content, length, inode and
**mtime to the nanosecond** were all unchanged; no fixture directory was shared; the
reconstructed path was right. The full event mask gave it away — the spurious record
arrived as **`mask = 0x8` alone** (`IN_CLOSE_WRITE`, no `IN_CREATE`, no `IN_MODIFY`),
where legitimate writes always arrive as `CREATE`→`MODIFY`→`CLOSE_WRITE`. A probe
then reproduced the mechanism: **a handle opened and written before the watch, closed
after it, delivers a bare close-write.** The fix flushes the kernel queue at `start`,
discarding events already in flight when the watches were placed. **What is not
closed, stated rather than smoothed over:** a close that lands *after* `start`
returns is still reported — the bytes were written before the watcher existed and the
event happened after it, and the two are indistinguishable. Suppressing it would be
dropping a real change, which is what this phase exists to prevent. No recurrence in
20 suite runs and 25 dedicated rounds, which is consistent with the fix and does not
prove the window is closed.

**The independent blind list earned its place again — third time running.**
`deleg_577c62bd` (leaf, `kimi-k3`, 79.75 s, 2 API calls) was given a plain-words
description and nothing else. Observed from the live transcript: **exactly one
`write_file` call, no reads, no search, no skill, no project-tree access.** Preserved
verbatim as `blind-attack-list-2d.md` (27,046 bytes, sha256
`fadac2fac005310e4b822ec7d415b4de60e5c89eef30bdb787e93d968033b005`, byte-identical
to the child's file). Compared **by mechanism, not by text**. Honestly stated: most
of its ~30 items were already covered here or already excluded by design;
**it found one real defect** (defect 2 above) and prompted a second fix. Its
mechanisms that this design already excludes are kept in `attack-list-2d.md` §M with
the reason, **including one it raised that is a genuine input to 2e**: a mount point
inside the root, or an overlay/FUSE filesystem under it, produces no events, which
2e's own-mount requirement is the phase that owns. Unverifiable limit, stated: the
contamination check proves the child read no project file; it does not prove the
model had no prior knowledge of Atrium.

**This list is the first in the project whose kernel facts were measured before it
was written** — nine probes, no project file touched: non-recursiveness,
`IN_DONT_FOLLOW`'s two behaviours, the queue limit and its overflow record, `wd`
stability across a rename, cookie pairing across two watches, a watch's death when
its directory is replaced, non-UTF-8 name bytes, `EACCES` on a mode-000 directory,
hard-link invisibility, the watch-placement cost (2461 dirs → 4.82 ms), and the
actual reduction (one `MODIFY` per completed edit). Every one is quoted in
`phase-2d-evidence.txt` §1. **One line of the attack list was wrong and is corrected
in place rather than quietly rewritten** (§B): its first draft said three writes give
one record; measurement showed three separate open/close cycles are three edits and
three records, and collapsing them would lose two.

**Honest caveats for the reader, none of them hidden:**

- **It cannot tell who made a change.** A shell command and Atrium's own file
  operations are identical at the kernel. De-duplication is **Phase 3's**.
- **It reports changes the file-operation layer made too**, by design. Stated in the
  README so nobody reads an authority into this phase that it does not have.
- **A change made to an in-root file through a hard link outside the root is
  invisible** (measured: no event). The 2a/§N channel in the watcher; closed by the
  root being its own mount, which is **2e's**. Test `i8` asserts the current
  behaviour so 2e's change is visible when it lands.
- **A file written into a brand-new directory before its watch is attached** can be
  missed; one event-loop turn. Reduced by walking on `CREATE`, not closed.
- **Overflow loses events.** The gap is reported; nothing is guessed, and a resync is
  deliberately **not** built (`attack-list-2d.md` §L.2).
- **Linux only**, via inotify. No portability requirement in v1.
- **The tool is not agent-facing**, and is deliberately not routed through the
  resolver: it takes real host paths, as `crates/snapshot/` does.

**Not done, on purpose:** no cage (2e), no gate (Phase 4), no effects / SQLite / log
(Phase 3), no UI, no agent loop (Phase 5). Nothing touched outside `crates/watcher/` plus
this STATUS append, `attack-list-2d.md`, `blind-attack-list-2d.md`,
`delegation-briefs/phase-2d-watcher.md`, `phase-2d-evidence.txt`, `BUILD-PLAN.md`
and `DECISIONS.md`.

**One thing Muffin needs to do, and it is not a test.** `crates/watcher/.hermes/
environment.json` **was not written** — the write was blocked by the
agent-instruction-file guard and the approval prompt timed out, which is not consent,
so it was not retried or routed around. Consequence: `hermes verify` needs
`--skip-start` for this crate instead of returning `ok: true` bare. The fix is one
file, copied from `crates/snapshot/.hermes/environment.json` with the name changed. It is
cosmetic for a CLI with no server (`HANDOFF.md` §5), and it is listed here rather
than worked around.

**Muffin's gate — the only thing that signs this off:**

```
cd "/home/muffin/VibeCodeProjects/atrium/crates/watcher" && bash hand-test-2d.sh
```

Expect the last two lines `lines run: 16   failures: 0`, `outside touches: 0   root
disappearances: 0   real-path leaks: 0`, then
`hand-test-2d: every line behaved as required`, exit 0. Any `[FAIL]` line, any `!!`
line, or a non-zero exit means it is not holding. §I of the harness prints statements
(what this watcher cannot see) rather than checking anything.

### Phase 2c — snapshot and restore: built in-session by the parent, because the last two build children died at the cap (14 Sep 2026, Hermes session)

> **Superseded, later on 14 Sep 2026: Muffin ran `crates/snapshot/hand-test-2c.sh`
> himself and Phase 2c is SIGNED OFF — see "Verified hands-on" above. This entry is
> left as written because it was the state when it was written. Its "NOT signed
> off" line is no longer current.**

**Read this entry as evidence of what was RUN, not as a pass.** Phase 2c is NOT
signed off. The gate below is Muffin's to run. Full command-by-command evidence is
in `phase-2c-evidence.txt`.

**What it is.** A new crate `crates/snapshot/` (binary `atrium-snapshot`). `snapshot
create` copies the whole environment root into a **store outside it**; `snapshot
restore` puts the root back and removes anything created since; `snapshot list`
shows what is restorable. The undo `DESIGN.md` §3.4 promised. The spec of record is
`attack-list-2c.md`, written *before* the code, as every phase's has been.

**Built in-session, not delegated — with the observed reason.** The last two build
children both died with no report: 2a's child on an HTTP 429 quota wall mid-edit,
2b's child on `exit_reason=timeout` at the 900 s cap after 44 API calls, and each
needed a full parent salvage pass afterwards. The model's headroom was probed clear
(HTTP 200) before deciding, so the reason is time, not quota. Recorded in the
brief header. **This is a change of method, not of standard**: nothing below is
relayed from a child, because there was none.

**Parent verification, all observed 14 Sep 2026** (real output, not summary):

- `cargo fmt --check` → clean. `cargo build` → exit 0. `cargo test` → **41 passed,
  0 failed**. `hermes verify --json` → `ok: true` (readiness null, correct for a
  CLI).
- SHA-256 is implemented here rather than pulled from a crate, so it is checked
  against the published NIST vectors first: empty, `abc`, the 448-bit and 896-bit
  padding boundaries, and one million `a`, plus **70 combinations** proving the
  streaming path agrees with the one-shot path. All pass.
- `bash crates/snapshot/hand-test-2c.sh` → **rc 0, 111 lines, 0 failures**, with three
  independent checks running alongside the lines and able to fail the run
  regardless of what the program printed about itself: the outside directory and
  the host `/etc/passwd` unchanged, the store unchanged by any refusal, and the
  root still present.
- **Three negative proofs hold**, each verified by the intended failure appearing
  in the output, not merely by a non-zero exit code (2b lost a pass to exactly that
  trap): a broken expectation produces `[FAIL]` and rc 1; a real change to the
  outside directory produces 98 `!! OUTSIDE TOUCHED` lines; a refusal made to write
  into the store produces `!! STORE CHANGED`.
- Two guards proven **load-bearing** by disabling them and watching a specific test
  fail — the same-length tamper check (below) and the read-only-directory
  pre-flight — then restoring the source byte-exact.

**Two real defects, both found after the phase was already green.** Both are
recorded in full in `phase-2c-evidence.txt` §7.

1. **A 20x cost that also weakened a guarantee.** Of a 5,873 ms snapshot, 5,592 ms
   was re-reading every file just written. Measured cause: the first read of a file
   after writing it costs ~1.119 ms against ~0.012 ms for later reads — about 90x,
   a per-file constant, not per-byte. Fixed by hashing each file's bytes **as they
   stream out** and recording what was actually written. That both removes the
   re-read and strengthens the guarantee, because the record now attests to the
   bytes written rather than to a re-read of them. The check it could have lost —
   catching a file changed *during* the snapshot — was kept (both hash sets are
   already in memory) and gained its own error variant, `ChangedWhileCopying`.
   Observed after: **277 ms** create, 294 ms restore on the same 5,000 files.
2. **A refusal that destroyed data — the worst class of defect in this phase.**
   Found by the **blind list**, after 97/97 harness green and 40/40 tests green.
   `clear_contents` needs write permission on a directory to delete its children, so
   a directory recorded as mode 555 stopped the deletion — **after everything
   beside it had already been deleted.** Observed: `restore` exited 1 with
   `Permission denied (os error 13)`, the modified file was **gone rather than
   restored**, and only the read-only directory survived. The undo mechanism caused
   the loss it exists to prevent. Fixed with `ensure_contents_removable`: a
   pre-flight pass over every directory about to be cleared, **before the first
   deletion**, so a tree that cannot be emptied is refused with nothing changed.
   Observed after: exit 0, the modified file restored, the read-only directory's
   contents restored. Permanent regression test
   `restore_empties_a_read_only_directory_instead_of_failing`; harness line L.4.

**What it promises, and the boundaries of it.** A snapshot is a directory **plus a
completed record** (a `.snapshot` file written last); `list` and `restore` only
accept a pair, so a create process killed halfway leaves a directory that is not
restorable and **left on disk as evidence** rather than tidied away. The record
holds every entry's kind, mode, size and SHA-256, hashed in flight, so a snapshot
altered after it was taken is refused **before the environment is touched**, and a
same-length tamper is caught by the checksum where a size check could not be.
Every metadata call is `symlink_metadata`; links are copied as links and never
dereferenced in either direction, so a dangling link is a success, not an error.

**The store must be outside the root, and a store inside is refused, not warned
about.** Reason, in the design's own words: a save point the boss can delete is not
a save point. This also closes pending item **§N.23** from `attack-list-2b.md` —
"a command can write anywhere in the environment root, including wherever the app
keeps state". The undo state is the first durable thing the design would otherwise
put inside a sandbox's blast radius.

**Deliberately not preserved (recorded, not oversights; printed in every harness
run so the boundary is visible where results are read):** modification times
(`std` cannot set one), hard links (come back as separate files with identical
bytes), ownership, ACLs, extended attributes, sparse layout, directory mtimes.
**Access times are neither preserved nor defended against** — new item §K.7, from
the blind list: reading a file to hash it can update its atime, and
`O_NOATIME` is not generally available. Recorded rather than claimed, because
"taking a snapshot changes nothing" is a sentence a later session could read as
covering more than it does.

**Honest caveats for the reader, none of them hidden:**
- **The store is not protected from a same-user attacker.** The record detects
  accidental alteration; it is not cryptographic, and the store sits in the user's
  own account. The design works only if the store is genuinely outside the agent's
  reach, which is **2e's boundary**, not this phase's. §H.4 and §K.5 state it.
- **Restore is not crash-safe.** A power cut or ENOSPC midway leaves a
  half-restored environment; mitigation is that nothing is deleted until the
  snapshot verifies. The full fix (stage beside and swap) is declined as beyond
  what the plan asks (§K.2).
- **A snapshot does not freeze a tree being written to.** It detects churn
  (`ChangedWhileCopying`, above); it does not prevent it.
- The tool is **not agent-facing** and deliberately **not** routed through the
  resolver: it takes real host paths and there are no virtual paths here. It is
  called by the app, as the gate is.

**The independent blind list earned its place again — second time running.**
`deleg_59a8f0f7` (leaf, kimi-k3, 76.33 s, 2 API calls) was given a plain-words
description and nothing else. Observed from the live transcript: **exactly one
`write_file` call, no reads, no skill, no project-tree writes.** Preserved verbatim
as `blind-attack-list-2c.md` (18,436 bytes, sha256
`6b2ad920ac35dbe1ec8187438f875048b506d0c5b1fe4750412c2b552599d03c`, byte-identical
to the child's file). Compared **by mechanism, not by text** — a literal diff
reports a large overlap that is an artifact of both lists covering the same
subject. Result, honestly stated: most of its ~70 items were already covered here;
**six mechanisms were genuinely new** and became gates — a FIFO (the trap being
that a naive copy *opens* it and blocks forever), record-level idempotence
(snapshot → restore → snapshot, records must match), locale independence, the
read-only-directory line, a symlink planted at a snapshot's own name, a snapshot
tree swapped for a symlink to a decoy directory, and an *extra* file inside the
tree. **It found defect 2 above.** Unverifiable limit, stated: the contamination
check proves the child read no project files; it does not prove the model had no
prior knowledge of Atrium. Same caveat as the earlier blind lists.

**Frozen files and records untouched (observed, by mtime).** `crates/resolver/`,
`crates/fileops/`, `crates/shell/` sources all predate this phase's work, as do
`attack-list.md`, `attack-list-1b.md`, `attack-list-2a.md`, `attack-list-2b.md` and
`crates/resolver/hand-test.sh`. No instrumentation and no negative-test markers left in
`crates/snapshot/src/` or its tests or harness (greps return 0).

**Not done, on purpose:** no copy-on-write (see `DECISIONS.md`; the root's
filesystem is 2e's unmade decision and a tmpfs cannot reflink at all — observed
"Operation not supported"), no cage (2e), no automation (Phase 5), nothing
touched outside `crates/snapshot/` plus this STATUS append, `attack-list-2c.md` and the
BUILD-PLAN banner.

**Muffin's gate — the only thing that signs this off:**
`cd "/home/muffin/VibeCodeProjects/atrium/crates/snapshot" && bash hand-test-2c.sh`
Expect the last line `lines run: 111   failures: 0   holes demonstrated (expected,
2e's): 0` and `hand-test-2c: every line behaved as required`, exit 0. Any `[FAIL]`
line, any `!!` line, or a non-zero exit means it is not holding. Sections H and I
print statements (what this tool is not, and what it does not preserve) rather than
checking anything.

### Phase 2b — the real shell: built by a subagent that timed out; finished, corrected and verified by the parent session (14 Sep 2026, Hermes session)

> **Superseded, later on 14 Sep 2026: Muffin ran `crates/shell/hand-test-2b.sh` himself
> and Phase 2b is SIGNED OFF — see "Verified hands-on" above. This entry is left
> as written because it was the state when it was written. Its "NOT signed off"
> line is no longer current.**

**Read this entry as evidence of what was RUN, not as a pass.** Phase 2b is NOT
signed off. The gate below is Muffin's to run. Full command-by-command evidence
is in `phase-2b-evidence.txt`.

**What happened.** A new crate `crates/shell/` was built for Phase 2b from
`delegation-briefs/phase-2b-shell.md`, whose spec is `attack-list-2b.md` (written
*before* the code, as Phases 1b and 2a were). Build and blind-list were
dispatched together (`deleg_3ca1b68b` build, `deleg_ade9c6f9` blind). **The build
child timed out at its 900 s limit with no report** — 44 API calls, still editing
its own harness, `README.md` never written. Its files were complete enough to
build. **Nothing it claimed is evidence**; this entry records only what the
parent re-ran afterwards.

**Built by the child (files on disk):** `crates/shell/{Cargo.toml, src/lib.rs,
src/main.rs, tests/shell_tests.rs, hand-test-2b.sh, README.md,
.hermes/environment.json}`. One dependency, by path, on our own `atrium-resolver`
— no external crate, `std` only otherwise. `crates/resolver/` and `crates/fileops/` untouched
(mtimes unchanged).

**What it does, and the honest scope of it.** Runs a real command with its
working directory resolved through the resolver **before anything is spawned**, a
fixed non-inherited environment, stdin not connected to a terminal, a time limit,
an output cap, and a **three-way status** (exit code / terminating signal /
timeout). It guarantees **where a command starts and what it is handed**. It does
**not** confine what the command can reach — a real command can still `cd /` and
act on the host. That is `BUILD-PLAN.md` §2e, a required later phase, and the
harness **demonstrates the hole and marks it** (5 `HOLE` lines) rather than
letting a green run imply a sandbox.

**Parent verification, all observed 14 Sep 2026** (real output, not summary):

- `cargo fmt --check` → clean (rc 0).
- `cargo build` → exit 0.
- `cargo test` → **45 passed, 0 failed** (36 from the child + 9 added below).
- `hermes verify --json` → `ok: true`.
- `bash crates/shell/hand-test-2b.sh` → **rc 0, 78 lines, 0 failures, 5 holes
  demonstrated**; the outside directory and the host `/etc/passwd` unchanged
  across the whole run (checked after every line, by looking at the disk).

**A real defect the independent blind list found, and the fix — the reason this
round earned its place.** The blind list (`blind-attack-list-2b.md`, 62 items,
one tool call, no project files read, sha256 `8818f24819…5048c16` verbatim)
supplied no new promise but several mechanism-level gaps the first list had
missed; those became `attack-list-2b.md` §N. One of them was a genuine bug:

- **A command that backgrounds a process inheriting its output pipes made the
  runner block until the *survivor* ended — past its own time limit.**
  `sh -c '(sleep 30) &'` with a 2000 ms limit **blocked for the full 12 s outer
  guard** (12002 ms, observed). The runner joined its reader threads, so it
  waited on the grandchild's lifetime. This is the shape a real build tool or
  daemon takes, so it would have hung ordinary work.
- Fixed by not blocking on the readers: poll for a 200 ms grace after the command
  ends, then report what was captured and return. Observed after the fix:
  **222 ms, `status=exit 0`.** The survivor is deliberately **not killed** —
  a process-group kill is containment, which is §2e — and that is documented in
  `README.md`.

**Three proofs, all observed** (a harness that has never failed is not evidence):

1. **The harness can fail:** one expectation broken → `[FAIL]` lines and exit 1.
2. **The escape detector fires:** a deliberate change to the outside sentinel →
   `!! OUTSIDE TOUCHED` ×1 and exit 1. *(Disclosed: my first attempt at this
   proof was a **false pass** — the injected line broke the harness's quoting, it
   aborted at exit 2, and I had only checked "exit code ≠ 0". Corrected by
   syntax-checking the harness first and asserting the `OUTSIDE TOUCHED` count.
   Same mistake class as 2a's flawed detector negative test.)*
3. **The two new guards are load-bearing:** reverting the fix makes
   `cargo test n8_` fail after 12.0017 s; replacing the error re-wording with a
   verbatim pass-through makes the disclosure test fail and **print the leaked
   real path**. Both sources restored byte-exact afterwards (hashes compared).

**What the passing 78 lines do and do not prove.** A passing line and a failing
line differ by one printed word, so a run cannot be checked by eye for
correctness — only for the absence of that word. Reading the script is what
establishes that the right lines ran. `§A`–`§K` were written before the code;
`§N` was written **while the code was being built**, so the child never saw it
and its harness did not run it until the parent added it. That extension is the
agent's, not part of the phase's original gate — stated plainly rather than
presented as a clean hand-off.

**Uncertain / not established:**

- **The 2b/2e split is Muffin's to accept or reject.** 2b runs a real command
  with a real limit and does not cage it; §G is expected to reach the host, and
  the step "confirm it cannot reach the host" has moved to 2e. Recorded in
  `DECISIONS.md` and `BUILD-PLAN.md` §2e.
- **The time limit and the output cap are safety stops, not the kill switch**
  (`DESIGN.md` §7.1, Phase 4). Defaults: 30 s, 1 MiB per stream. No design value
  fixes these; the child's choices stand pending objection.
- **Recorded, not 2b's:** the §K symlink-object limitation, TOCTOU (§N.19),
  resource exhaustion (§N.21), concurrency semantics (§N.22), app-state-in-root
  (§N.23), and non-UTF-8 arguments being unrepresentable (§N.17).
- **A command can learn the sandbox's real on-disk path** through its own output
  (§A.8/§H.4). The runner does not redact command output — that would corrupt
  data. §2e closes it by removing the path from the command's view.
- This entry was written by the parent session (`AGENT-RULES.md` §10 — append
  only; only Muffin moves an entry to "Verified hands-on").

### A hole in the plan's own shell promise, found before 2b was started; new phase 2e (14 Sep 2026, Hermes session)

**Read this as a stop-and-decide record.** No 2b code was written. Phase 2a is
still unsigned. **One live decision here is Muffin's: the phase boundary moved.**

**What happened.** Starting Phase 2b (the real shell) meant reading what
"confined to the environment root" actually promises. It promises less than it
reads as.

- `DESIGN.md` §3.3 says the terminal "runs actual commands **on the host**, with
  its working directory confined to the environment root", and explicitly rejects
  a fake shell in favour of a real one.
- So "confined" = the **starting directory**. The command itself is a real
  program with the user's own access. It can `cd /` and act anywhere the user
  can. The boundary the rest of the design rests on **does not exist for a
  command**, and no path check can create it.
- This is not a new discovery in kind. It is the same gap 2a already observed in
  a smaller form: a **hard link** inside the root let the sandbox read and
  overwrite a file outside it (§N / L.4 in `attack-list-2a.md` — a hard link is a
  second name for one file, not a path, so no path check sees it). A command is
  the same hole at full size.

**The finding that forced a real decision:** `BUILD-PLAN.md`'s Phase 2
verification block — written long before this session — already contained
*"Try `cd /` and then a destructive command. Confirm it cannot reach the host."*
2b cannot satisfy that step. Strong confinement was therefore already implied by
the plan's own gate; the alternative was to delete a verification step.

**Decision taken (14 Sep 2026):** a new phase **2e — strong confinement for `run
commands`**, after 2d, **required before Phase 5** (the agent loop). Requirement
is mechanism-agnostic: inside the command's own view of the filesystem, outside
the root does not exist. Mechanism is 2e's own later decision. Rejected: folding
it into 2b (two hard things in one phase — the same unattributability problem the
1b split exists to avoid); deferring to v2 (contradicts the plan's own gate and
the project's central claim); a fake shell (already rejected, untouched by this).
Recorded in full in `DECISIONS.md`.

**Documents changed this pass** (all verified landed; nothing else touched):

- `BUILD-PLAN.md` — new `## 2e` section; correction block under 2b; the host step
  **moved** out of the Phase 2 verification block into 2e's (kept, not deleted,
  with a dated note saying where it went).
- `DESIGN.md` §3.3 — dated correction: "working directory confined" is not
  confinement; pointer to 2e; "run commands" in §8 is the same weight class.
- `DECISIONS.md` — new decision **"Strong confinement for `run commands` is its
  own phase, and it gates the agent loop"**, with the three rejected alternatives.
- `STATUS.md` — this entry.

**Mechanism availability, checked on this machine (observed, not assumed):**
bubblewrap 0.12.0 installed and **measured working** — a caged process could not
see `/home`, `/etc/passwd` did not exist inside it, writes landed in the bound
root, `python3` ran, the host was untouched; and the root mounted at `/` means
virtual paths line up exactly as Phase 1 defines them. `unshare -Urm` works
unprivileged; Landlock is compiled **and active** (`/sys/kernel/security/lsm`);
podman and docker are installed. Recorded so 2e does not have to re-probe.

**Not done:** no shell code, no brief, no attack list for 2b or 2e. No delegation
dispatched. `crates/fileops/` and `crates/resolver/` code untouched by this pass.

**Uncertain:**

- **Whether the phase split is right is now Muffin's to accept or reject** — the
  boundary he watches moved. The requirement behind it is not optional; the
  *packaging* (2e as a phase vs some other arrangement) is his.
- **Whether "confined" should have been read as starting-directory-only** is a
  reading of the old text; if he intended strong confinement when the plan was
  written, 2e is simply that requirement arriving late rather than a new scope.
- **2e's actual mechanism is undecided** — all four candidates were only probed
  for availability, not compared for fit (path mapping, dev/proc, performance of
  spawning per command, what a build tool inside the cage needs).
- Written by the parent session (`AGENT-RULES.md` §10 — append only).

### Phase 2a — file operations: built by a subagent that was cut off; finished and corrected by the parent session (14 Sep 2026, Hermes session)

**Read this entry as evidence of what was RUN, not as a pass.** Phase 2a is NOT
signed off. The gate below is Muffin's to run.

> *(Superseded in part, 14 Sep 2026, later the same day. Muffin ran the gate —
> `lines run: 64   failures: 0`, `hand-test-2a: every line behaved as required`,
> exit 0 — and the phase is signed off; his sign-off is recorded above in
> "Verified hands-on". This paragraph is left as written because it was true when
> written: nothing in this entry, and no agent's re-run, could have been the gate.)*

**What happened.** A new crate `crates/fileops/` was built for Phase 2a from
`delegation-briefs/phase-2a-file-operations.md`, whose spec is
`attack-list-2a.md` (written *before* the code, as Phase 1b's was). Build and
blind-list were dispatched together as `deleg_f8c377a6` (kimi-k3). **The build
child did not finish and produced no report** — it was killed by the account's
weekly usage limit (HTTP 429 on `requ3gge`, "weekly usage limit reached",
`exit_reason=max_iterations`) 8m33s in, part-way through editing its own harness.
Its files were complete enough to build, but **nothing it claimed is evidence**;
this entry records only what the parent re-ran afterwards.

**Built by the child (files on disk):** `crates/fileops/{Cargo.toml, src/lib.rs,
src/main.rs, tests/fileops_tests.rs, hand-test-2a.sh, README.md,
.hermes/environment.json}`. One dependency, by path, on our own `atrium-resolver`
— no external crate. `crates/resolver/` untouched (mtimes unchanged: `src/lib.rs`
2026-09-11 21:43:20).

**Parent verification, all observed 14 Sep 2026** (real output, not summary):

- `cargo build` → exit 0.
- `cargo test` → **56 passed, 0 failed** (55 from the child; the 56th is the
  regression guard added below). Includes the §E.1 recursive-delete test and the
  §E.2 loop test.
- `bash crates/fileops/hand-test-2a.sh` → **rc=0, 64 lines, 0 failures, "every line
  behaved as required"**; outside-directory listing and sentinel checksum
  unchanged; host `/etc/passwd` checksum and mtime unchanged.
- **Parent's own spot attacks** (`/tmp/spot-2a.sh`, `/tmp/blind-2a-probe.sh`):
  recursive delete of a tree containing an outside-pointing link left the
  outside files byte-identical; a self-link (link to the root) refuses; `delete /`
  refuses; non-empty dir without `--recursive` refuses; traversal out creates
  nothing outside.
- **The escape detector was proven non-vacuous**: with a wrapper binary that
  touches the outside directory mid-run, the harness printed `!! OUTSIDE TOUCHED`
  repeatedly and exited 1. So "0 escapes" is a measured result, not a blind check.

**Two real defects the parent found and fixed — both recorded, neither hidden:**

1. **A §H.2 violation: thirteen refusals named the sandbox's real location.**
   The operations passed the resolver's `Display` text straight through, and
   three of its variants name a real on-disk path. Observed: `write
   /outside-link/…` → "its target is `/tmp/atrium-2a-outside`". Every *success*
   was already clean; the leak was in refusals. Fixed in `src/lib.rs` by
   re-wording `SymlinkEscapes`, `TraversalAboveRoot` and `EscapesRoot` from the
   resolver's **structured** fields — naming the virtual path, dropping the real
   one — with **no resolver change**. Guarded by a new test
   (`h_no_real_path_is_disclosed_in_any_reason`); the guard was **negatively
   tested** (bug re-introduced → test FAILED and harness exited 1 with 13 flags;
   fix restored → both pass).
2. **An error in the attack list itself (`attack-list-2a.md` §C.12).** The parent
   wrote C.12 as `MUST DO IT INSIDE` on the strength of Phase 1b accepting
   absent-inside paths — but the absent target was that path's *parent*, and the
   brief also says write-file does not create parents (§F.2). Two requirements of
   the parent's own making contradicted each other. Corrected by splitting it:
   C.12 refuses for the missing-parent reason; new **C.13** creates the absent
   target *through* the dangling link and must succeed. Recorded in the attack
   list with a dated correction, not silently.

**Two findings that came from the independent blind list**
(`blind-attack-list-2a.md`, 36 cases, verbatim copy, sha256
`d1f1d3952e…28c3d`, one tool call, no source or brief seen):

- **Hard links are a hole the first list missed entirely** (now §N). Observed: a
  hard link inside the root pointing at a file outside it let the sandbox **read**
  that file's contents and **write** to it — the outside file was modified, and
  no path check can see it, because a hard link is a second *name* for the same
  file, not a path. Cross-filesystem hard links are impossible (observed:
  `Invalid cross-device link`; `/tmp` is device 50, `/home` is device 49), so the
  mitigation is that **the environment root must be its own mount point** — a
  deployment fact `DESIGN.md` §3.1 does not yet state. Recorded as open item L.4.
  **Not fixed in 2a; not fixable in 2a.**
  *(14 Sep 2026, later: `DESIGN.md` §3.1 now states it — the environment root is
  its own filesystem or tmpfs — and `BUILD-PLAN.md` 2e carries the requirement to
  honour it. L.4 is discharged as documentation; the mount itself is 2e's.)*
- Symlink-as-middle-component-then-`..`-back-in, and the TOCTOU race class —
  checked, refused correctly / not reachable through the six operations; grouped
  as the class a future resolution-hardening phase owns.

**What Muffin should run by hand (the gate for 2a — but see the note below):**

```
cd "/home/muffin/VibeCodeProjects/atrium/crates/fileops" && bash hand-test-2a.sh
```

Watch each line; the run exits non-zero on any `FAIL`, any `!! OUTSIDE TOUCHED`,
or any `!! HOST PATH DISCLOSED`.

**Uncertain / not established:**

- **Whether 2a is the right gate unit.** `BUILD-PLAN.md` PHASE 2 states **one**
  verification block covering 2a+2b+2c+2d. Whether Muffin watches 2a alone now or
  after more of Phase 2 exists is **his call and is unresolved** (it was flagged
  as an open structuring question before this build started). This entry does not
  decide it.
  *(Resolved by his action, 14 Sep 2026: he ran the 2a harness by itself and
  signed 2a off as its own gate. The single Phase 2 block in `BUILD-PLAN.md` is
  therefore now inaccurate on this point — it covers 2a–2d, and 2e has its own
  block. Recorded rather than silently rewritten; the plan text still needs the
  correction.)*
- **The §K symlink-object limitation is reproduced, not fixed** — `delete
  /inside-link` removes the *target* and leaves the link dangling. Fixing it needs
  a resolver change, which is a stop-and-ask item. Reproduced by the child's
  tests (`k2`, `k3`) and recorded in the attack list.
- **Formatting.** The child left `main.rs` and `tests/fileops_tests.rs`
  unformatted — 44 hunks reported by `cargo fmt --check`, i.e. the whole files,
  not a stray line. Formatted 14 Sep 2026 *after* the two correctness fixes
  above; behaviour-neutral, and re-verified: build, `cargo test` (**56 passed**)
  and `hand-test-2a.sh` all green afterwards, `cargo fmt --check` clean.
- **No blind-list case was skipped deliberately**; the ones not fixed are named
  above with reasons.
- This entry was written by the parent session (`AGENT-RULES.md` §10 — append
  only; only Muffin moves an entry to "Verified hands-on").

### Phase 1b built by a delegated subagent; verified by the parent session (13 Sep 2026, Hermes session)

**What happened:** at Muffin's go, Phase 1b (non-existent-path resolution) was
dispatched to a leaf subagent (`deleg_a1e84631`, kimi-k3, 582 s) from
`delegation-briefs/phase-1b-path-resolution.md`, with the child instructed to
read the brief from disk and told the file wins over the dispatch text. The
independent blind attack list required by the brief §13 was commissioned
separately (`deleg_b6b1894b`) from a child that saw none of the resolver source,
the brief, `attack-list.md` or `attack-list-1b.md`. Both returned and were
verified by this session, not accepted on report.

**Observed by the parent session, after the child stopped** (all commands run
here, not in the child):

- `cargo build` → exit 0. `cargo test` → **13 passed, 0 failed**.
- `cargo run -- demo` → exit 0, "demo: all cases behaved as required".
- `bash hand-test-1b.sh` → exit 0, **92 lines**, 43 accepts, 49 rejects, 0 FAIL,
  0 ESCAPE. (Extended by this session to cover `attack-list-1b.md` §M; see
  below.)
- Phase 1's **unedited** `hand-test.sh` → **28 lines "fail", 0 ESCAPE.** All 28
  are reject→accept verdict inversions, every ACCEPT inside the root: §B 10
  host-looking names (`/etc/passwd`, `/root`, `/proc/self/cwd`, `/dev/null`…),
  §E 2 absolute backslash names, §F 9 Unicode lookalikes and both `café`
  spellings, §G 7 (the 200-nested line, a 255-byte name, trailing dots and
  spaces). The 300-character line still rejects; only its reason changed. These
  are verdict changes, not regressions — see the note at the top of
  `attack-list.md` and `DECISIONS.md` "Phase 1b — three resolver rulings".
- **Length (Muffin's decision, byte-accurate, measured by the resolver itself):**
  255-byte ASCII absent → ACCEPT; 256-byte absent → REJECT *"the component … is
  256 bytes long; a single path component may be at most 255 bytes (NAME_MAX)"*;
  200 Hebrew characters (400 bytes) absent → REJECT with the same own reason;
  200 nested short components → ACCEPT. **No operating-system error leaks into
  the reason** — the pre-1b binary said *"the operating system refused … (`File
  name too long (os error 36)`)"*, and the new tests assert that string is gone.
  The 255-byte accept is also tested as a **real file on disk**; the 256-byte
  case can only be tested absent, because the filesystem refuses to create it.
- **Parent's own spot attacks** (independent of the child's tests): every
  BUILD-PLAN §1–4 shape, both length-vs-normalisation and 20-level depth pairs,
  percent-encoded traversal, the trailing-slash deviation preserved, all
  contained; 0 escapes.
- **Phase 1 tests updated** (parent read the current file and recovered the
  pre-edit text from the session record at 21:23:11; no backup exists and the
  project is not a git repo): `tests/resolver_tests.rs` §B all 10 lines
  reject→accept; §E 2 of 4 lines reject→accept; §F 7 lines reject→accept;
  `:304-307` 300-char — same verdict, new required reason (`NameTooLong{300}`
  plus a no-"os error" assertion, where the old check was generic enough that
  the OS error satisfied it); `:308-311` 200-nested — **verdict inverted** to
  accept. New tests `section_j_component_length` and `phase1b_attack_list`.

**Changed by this session, 13 Sep 2026:**
- `crates/resolver/src/main.rs` — `link-rel-out` added to the fixture set: a **relative
  symlink target** (`../../outside`) that climbs above the root, required by
  `attack-list-1b.md` §M.3. A relative target is resolved against the link's own
  directory, so the traversal never appears in the requested path; absolute
  fixtures could not test it.
- `crates/resolver/hand-test-1b.sh` — extended to run every §M line in the existing
  shape (path, verdict, an independent containment check on every ACCEPT, and a
  fail on any reason that cites mere absence). Now 92 lines.
- `DECISIONS.md` / `attack-list-1b.md` edits by the Claude session were **read
  and verified** here, not assumed: ruling 4 (a dangling symlink followed by
  `..` resolves against the target's location — `/link-to-nothing/..` ACCEPTs,
  `/link-to-nothing-out/..` REJECTs) and §M (the blind list's uncovered lines) do
  say what was claimed.

**Not done:**
- **The phase gate has not been run.** Phase 1b is **not signed off**. The gate
  is Muffin driving `attack-list-1b.md` by hand; nothing in this entry
  substitutes for it. This entry sits under "Agent-reported, unverified" for
  exactly that reason, and only Muffin moves it.

  *(Superseded, 13 Sep 2026, later the same day. Muffin ran
  `hand-test-1b.sh` — 43 accepts, 49 rejects, `every line behaved as
  required`, exit 0 — and the phase is signed off. His sign-off is recorded
  above in "Verified hands-on", where only his own hands-on pass can put an
  entry. This paragraph is left as written because it was true when written:
  nothing in this entry, and no agent's re-run, could have been the gate.)*
- The 300-character component's §G line is listed in `attack-list.md` as a
  reject whose reason changed rather than a verdict change; `attack-list.md` and
  `hand-test.sh` were deliberately not edited.
- Nothing was added to the phase's own record to make results look consistent.

**Uncertain:**
- **One judgement call, now ruled on after the fact.** The implementing child
  hit `<dangling-symlink>/..` with no rule covering it and handled it by its own
  reading of ruling 1. That reading was correct and `DECISIONS.md` ruling 4 now
  states it as a rule, but the code was written before the rule existed. Ruling
  4 is a post-hoc description of behaviour, not a specification that drove it —
  the two agree, and a re-verification confirmed contained, but the ordering is
  recorded honestly here.
- Whether the blind child's model had prior knowledge of Atrium cannot be
  proven; only that it made no file reads and no project access (exactly one
  tool call, `write_file` to its own file).
- The child's own report miscounted its verdict-inversion groups (it listed §B
  10 + §E 2 + §F 11 + §G 7 = 30 against its own stated total of 28; §F is 9).
  The figures above are this session's reading of the run output.

### Phase 1 signed off by Muffin; STATUS updated on his instruction (13 Sep 2026, Hermes session)

**What happened:** Muffin ran the Phase 1 hands-on pass and reported it passed.
Verbatim from chat: ran `hand-test.sh` — *"24 accepts, 67 rejects, zero
failures"* — then tested **his own paths, "including a symlink-plus-escape
combination and one that climbs out and back in."** All behaved correctly, and
*"the rejection reasons named the specific step each time."*

**Changed by the agent, at his instruction:**
- New entry at the top of **"Verified hands-on"** recording the sign-off, what
  he ran, the extra cases he tried himself, and the NUL-byte coverage gap.
- **"Current phase"** rewritten: Phase 1 signed off 13 Sep 2026; Phase 1b is
  now the next build; Phase 2 is gated behind Phase 1b.
- Two stale "Phase 1 is NOT signed off" lines in the agent-reported entries
  struck through with a dated superseded note. Left visible rather than deleted,
  so the pre-sign-off record stays honest.
- **"Notes for the next session"** — the resolver line no longer says the pass
  is outstanding.

**The NUL-byte paths, as instructed:** the three NUL-byte cases (sections F and
H) are recorded as **covered by automated tests only** — a command-line argument
is terminated by a zero byte, so no terminal can pass one through. They are in
`cargo test` (`section_f_unicode_and_lookalikes`,
`section_h_nasty_combinations`), not in the by-hand pass.

**Authority note:** `AGENT-RULES.md` §10 reserves moving an entry into
"Verified hands-on" to Muffin. That is what happened here — he directed the move
in chat. The entry says so explicitly, so nobody later reads it as an agent
promoting its own work.

**Not done:** nothing else in STATUS was touched. No code was run or changed in
this pass; `crates/resolver/` is untouched (`src/lib.rs` still at its 11 Sep 21:43
mtime, no session markers).

**Uncertain:** none. This entry records the user's own report of a pass he ran
himself; the agent did not re-run the attack list for this entry, because a
re-run by the agent is not what the gate was.

### Handoff report written for the original agent (Sep 2026, Hermes session)

Muffin asked for a written report of the consistency sweep so he can hand it to
the Claude session that coordinated the project up to 11 Sep 2026, on the basis
that it may know intent not recorded in the docs.

**Created:** `consistency-sweep-report.md` — the full record: every change by
file, the four judgment calls I made that could be overturned, the two questions
I could not decide and why, the skill-desync risk, and an explicit list of what
is proven versus inferred.

**Action for whoever picks it up:** `consistency-sweep-report.md` §4 carries the
two questions it could not answer. **Both were answered by Muffin on 12 Sep 2026
— see the new sweep entry below and `DECISIONS.md`.** The report stands as the
record of what was changed and why; its §4 is now historical.

**Not a blocker for Phase 1 sign-off.**

### Consistency sweep — follow-up decisions (12 Sep 2026, Hermes session)

The sweep above left two collisions it could not fix, because they were design
decisions rather than documentation. **Muffin decided both, and the copying
abolition he asked for is the third change.**

| # | Item | Decision | Files changed |
|---|---|---|---|
| 1 | DESIGN §3.2 "allowed roots" vs the resolver's single root | Loose wording. `resolve()` stays **single-root permanently**; mounts get their own layer above it mapping to `(root, subpath)` | `DESIGN.md` §3.2, `DECISIONS.md`, `STATUS.md` Known-broken |
| 2 | Phase 1b had no home in the plan | Becomes **`PHASE 1b`**, its own numbered phase between Phase 1 and Phase 2 — not folded into 2a | `BUILD-PLAN.md` (new section + Phase 1 note), `DECISIONS.md`, `STATUS.md` Known-broken |
| 3 | Two skills duplicated two documents, unsynced | **Copying abolished.** Both SKILL.md bodies are now a pointer to the source document's absolute path | `atrium` + `muffin-style` SKILL.md, `HANDOFF.md` §6 |

**Result of #1 and #2:** `STATUS.md` **Known-broken / blocked** is empty again —
the two entries are now recorded as decided, with their reasoning, rather than
open. Neither was ever a Phase 1 sign-off blocker.

**Result of #3:** the drift risk is gone rather than managed. The old `diff`
checks are dead and must not be re-added; a skill body growing beyond a pointer
is the drift returning.

**Also changed this pass, forced by the above:** the delegation brief's §4 quotes
`DESIGN.md` §3.2 verbatim and so still carries the superseded "allowed roots"
sentence. Annotated in place as a superseded quotation — the brief is discharged
working documentation and its recorded quotes are left verbatim, with the change
flagged. No other file referred to the old wording, the Phase 1b gap, or the
skill copies.

**Not run:** nothing in this pass touched code, and no build, test, demo or
`hermes verify` run was performed. **The resolver and its tests are untouched** —
`src/lib.rs` still has its 11 Sep 21:43 mtime and no `crates/resolver/` file was opened
for writing in this pass.

### Project-wide consistency sweep (Sep 2026, Hermes session)

Muffin asked for a sweep of the whole project for collisions between documents,
and for them to be fixed. Eleven were found and fixed; two could not be fixed by
documentation alone and are recorded in **Known-broken / blocked** above.

**Fixed — each was one document contradicting another, or contradicting the
code:**

| # | Collision | Resolution |
|---|---|---|
| 1 | `HANDOFF.md` §3 said "No Atrium code exists. Phase 1 has not started", contradicting its own §7 and STATUS.md | Corrected to the real state |
| 2 | `STATUS.md` "Current phase" still said Phase 0, Phase 1 begins when 0c passes | Rewritten to Phase 1 built / unsigned |
| 3 | `STATUS.md` "Notes for the next session": "Nothing is built" | Corrected |
| 4 | `DECISIONS.md` §Path normalisation: "Action required … Not done yet" — it was done 12 Sep | Updated to done, with the G/I case added |
| 5 | `attack-list.md` header + Independence: second list "required" though it was produced 11 Sep | Marked satisfied, points at `blind-attack-list.md` |
| 6 | `STATUS.md` brief entry "User action: dispatch happens after 0c passes" — it was already dispatched | Corrected |
| 7 | `HANDOFF.md` §1: "Phase 1 and Phase 1b still need verification" — Phase 1b does not exist | Corrected |
| 8 | `HANDOFF.md` §5: `hermes verify` note said "use `--skip-start`" — a recipe fix now makes it pass unflagged | Added, with the caveat that it is per-project |
| 9 | `hermes-recon.md`: undated, asserted `glm-5.3` as current, contradicting STATUS.md | Dated banner added; marked stale; STATUS.md named the single home |
| 10 | `crates/resolver/README.md` test description did not mention the NUL gap | Gap recorded |
| 11 | `crates/resolver/tests`: four of the five moved lines tested, one not; comments stale | All five now tested |

**Also fixed, same class but not contradictions:** the delegation brief's status
line said "APPROVED for dispatch … when the user says go" after it had been
dispatched and built; `BUILD-PLAN.md` Phase 1 still required "the user writes
the attack list" with no note that this was replaced.

**Checked and found already correct** (listed so the sweep is not overclaimed):
`pipeline-check.md` §6 really does list 15 failure modes; `REASONING.md`,
`OPEN-QUESTIONS.md` and `pipeline-check.md` carry no phase-status claims.

**New risk recorded, not a collision:** the two Hermes skills are copies of two
project documents and **nothing syncs them**. Editing a document silently
desynchronises the skill a subagent loads. `HANDOFF.md` §6 now carries the
`diff` commands to check. *(Superseded 12 Sep 2026 by the follow-up decisions
above, item 3: the copying was abolished and both skills are now pointers, so
there is nothing left to sync.)*

**Observed after all edits:** `hermes verify` → `ok: true`, build exit 0, test
exit 0. `cargo test` 11/11. `hand-test.sh` → 24 accept, 67 reject, no failures.
Skills re-checked identical. Phase 1 remains **not signed off** — Muffin's
hands-on pass is untouched by this sweep and is still the gate.

*(Corrected 13 Sep 2026: it no longer remains unsigned. Muffin ran the hands-on
pass later that day and signed Phase 1 off, then Phase 1b as well. The counts
above are what this session observed at the time and are left as its record —
`cargo test` was 11 tests then and is 13 now.)*

### Attack-list corrections + hands-on pass prepared (Sep 2026, Hermes session)

**Changed:**
- `attack-list.md` — three contradictions fixed. Five section-E lines (`//`,
  `///home///documents`, `/home/documents//`, `/home/./documents/.`,
  `/home/documents/..`) moved into section I, where they belong. The bare `/`
  line removed from section G (it was listed as reject there and accept in I).
  A limitation note added to section F: the three NUL lines cannot be passed
  through the command-line harness at all.
- `crates/resolver/src/main.rs` — the demo's five "informational" spec-conflict lines
  are now real section-I pass/fail cases; the informational loop and its
  comment block deleted. `/` removed from the G reject list.
- `blind-attack-list.md` — created. The §12 blind list existed only in
  `/tmp/blind.json` and a delegation cache file; both are outside the project
  and `/tmp` does not survive a reboot. Copied in verbatim.
- `crates/resolver/hand-test.sh`, `crates/resolver/probe-parent.sh`,
  `crates/resolver/probe-blind.sh` — created. Run the whole list, the parent's own
  list, and the blind list respectively.
- `phase-1-hands-on.md` — created. Step-by-step for Muffin, with what counts as
  working and what counts as broken.
- `phase-1-evidence.txt` — created. Every check, its verbatim output, and what
  each result means.
- `crates/resolver/.hermes/environment.json` — created. The verification recipe.
  `start` is `null`: `atrium-resolver` is a command-line tool with no server,
  and the auto-detector's guess of `cargo run` made every `hermes verify` run
  report failure on a readiness poll for a web server that does not exist.

**Verified with the project's own recipe:** `hermes verify --json` → `ok: true`,
build and test phases both exit 0. `cargo test` 11/11. Before the recipe was
corrected it reported `ok: false` — build and test still passed; the failure
was the readiness poll. Fault in the detected recipe, not the resolver.

**Observed (parent-run, this session):**
- `cargo build` exit 0. `cargo test` 11/11 pass. `demo` exit 0, "all cases
  behaved as required".
- `hand-test.sh` — 91 lines run against a fresh fixture root: 24 ACCEPT,
  67 REJECT, zero FAIL, zero escapes. Every ACCEPT independently checked
  against the root prefix inside the script.
- `probe-parent.sh` — parent's own probes, including a prefix-sibling
  directory (`/tmp/atrium-probe-evil`) whose name shares the root's name as a
  string but is not inside it. Zero escapes.
- `probe-blind.sh` — the blind list's novel lines with their fixtures planted
  (`/proc/self/root`, `/proc/self/cwd`, `/dev/stdin`, self-looping symlink,
  middle-component symlink inside a real directory, prefix-sibling path).
  Zero escapes, zero hangs.
- Length cases re-run correctly after a scripting bug in the first attempt —
  the first version of `hand-test.sh` built a 600-character path that was 300
  repetitions of `/a`, not one 300-character name, so it was testing the wrong
  thing. Fixed; 301-char and 400-char paths now genuinely tested, and a
  256-character single component confirms the OS-length rejection path.

**Not done:** ~~Muffin's hands-on pass. Phase 1 is still NOT signed off.~~
**Superseded 13 Sep 2026 — Muffin ran the pass and signed Phase 1 off.** See
the "Verified hands-on" section at the top. This entry is left as written so the
pre-sign-off record stays honest.

**Uncertain:** none new on containment — it held again under a third run of the
blind list plus the parent's own list. The NUL lines remain unproven by hand
and can only be proven by the Rust test suite; that limitation is now recorded
in the list itself rather than left implicit.

**User action:** `phase-1-hands-on.md` — four steps, with what counts as
working and what counts as broken.

### Phase 1 resolver built by delegated subagent (kimi-k3); independently verified by the parent (Sep 2026)

Phase 1 (`crates/resolver/`, crate `atrium-resolver`, std-only, lib + bin) was built by
a delegated subagent per `delegation-briefs/phase-1-path-resolution.md`. The
parent then verified it itself — did not trust the child's report.

**Parent-observed, after the child returned:**
- `cargo build` exit 0; `cargo test` 11/11 pass.
- Independent attack sweep driven by hand through the harness
  (`resolve --root … <path>`), using a fresh fixture root, covering
  attack-list.md sections A–I plus 10 extra attacks the parent invented.
  **Zero escapes.** Every ACCEPT landed inside the canonical root; every
  escape attempt was REJECTed with a specific reason.
- §12 blind second attack list obtained: a subagent that saw none of the
  resolver source, the brief, or attack-list.md was asked what paths it would
  try. Every novel line was run against the resolver, including rebuilding the
  symlink/middle-component/magic-link fixtures its list required. **Zero
  escapes.** Its "most decisive battery" (symlinks, prefix-siblings) all held.

**Findings (parent-observed, honest):**
- **Section E and section I of attack-list.md contradict each other.** Five
  E-lines (`//`, `///home///documents`, `/home/documents//`,
  `/home/./documents/.`, `/home/documents/..`) are lexically identical to
  section-I accepts after normalisation, but E lists them as reject-cases. The
  resolver accepts them, resolving *inside the root* — the same behaviour
  section I demands. **This is a spec conflict, not a security hole.** The
  child silently reclassified them as "informational" in the demo; the parent
  re-ran each one by hand and confirmed every accept stays inside the root.

  *Resolved 12 Sep 2026:* the list was corrected rather than left standing.
  The five lines now sit in section I, where they belong, and the demo treats
  them as ordinary accept cases. See the entry at the top of this section.
- **Trailing slash on a file is accepted, not rejected.** `/afile.txt/`
  returns `<root>/afile.txt`. POSIX says a trailing slash on a non-directory
  should fail (ENOTDIR). The result is still inside the root, so this is a
  cosmetic deviation, not an escape — but it is a deviation and is recorded
  rather than smoothed over. **Re-confirmed 12 Sep 2026 with the corrected
  list** (`probe-blind.sh`); still accepted.

**Not done:** ~~Phase 1 is NOT signed off. Muffin has not run it hands-on, and
BUILD-PLAN requires the *user's* verification. The child's claim of a passing
`demo` was re-run twice by the parent (the child's tests and the parent's
independent harness) but neither counts as hands-on.~~
**Superseded 13 Sep 2026 — Muffin ran the pass and signed Phase 1 off.** The
parent-verification described in this entry was necessary but not sufficient;
the user's own pass, not an agent's, was the gate.

**Prepared for the hands-on pass 12 Sep 2026** — `phase-1-hands-on.md` has the
four steps in plain text, including what counts as working and what counts as
broken. No code change to the resolver itself; the demo and the scripts around
it were corrected to match the fixed attack list.

**Uncertain:** none material on containment — it held under every case tried,
by two differently-biased attack lists plus the parent's own. The residual
uncertainty is the ordinary one: no finite test proves a sandbox.

### Spike 0c latent TLS bug — found and fixed, transport now proven (Sep 2026)

**Changed:** `spikes/0c-mcp-http/Cargo.toml` — added `"reqwest"` to the rmcp
feature list. `spikes/0c-mcp-http/README.md` — findings written up.
Added `spikes/0c-mcp-http/src/bin/probe_noauth.rs`, a throwaway no-auth probe
(not part of the 0c deliverable; delete when 0c passes).

**Observed:** 0c compiled clean but had **no TLS backend in its dependency
tree at all** — `grep -E '^name = "(rustls|native-tls|openssl)"' Cargo.lock`
returned nothing. `rmcp`'s `transport-streamable-http-client-reqwest` feature
enables the reqwest *crate* but not any TLS feature; TLS is a separate opt-in.
Every `https://` request therefore died at the transport layer:

```
Error: Send message error Transport [...] error: Client error: error sending
request for url (https://mcp.deepwiki.com/mcp), when send initialize request
```

No mention of TLS in that message, and the same URL returned HTTP 200 via
`curl` — the fault was local. **This would have failed on Muffin's token run
too, and would have looked like a GitHub auth problem.** After adding the
feature, `rustls 0.23.43` and `hyper-rustls 0.27.9` compiled in.

**Verified after the fix (observed):** the probe connected to
`https://mcp.deepwiki.com/mcp` — `[1] initialized`, listed 3 tools, called
`ask_question`, returned text, `is_error: Some(false)`, exit 0. The main 0c
binary still builds (exit 0) and still refuses cleanly without a token
(`GITHUB_PERSONAL_ACCESS_TOKEN not set`, exit 1). GitHub's endpoint confirmed
reachable: HTTP 401, `bad request: missing required Authorization header`.

**Not done:** the GitHub round-trip itself. Still blocked on the PAT.

**Uncertain:** whether GitHub accepts the auth header as constructed
(`config.auth_header = Some("Bearer <token>")`). Plausible, untested. That is
now the *only* unknown in 0c — the transport underneath it is proven.

**What this changes:** 0c's two stacked unknowns are separated. Before today a
token-run failure could mean either "transport broken" or "auth rejected" with
no way to tell them apart. Now it means auth.

---

### Phase-1 delegation brief — drafted while 0c is paused (Sep 2026, glm-5.3 session)

**Changed:** `delegation-briefs/phase-1-path-resolution.md` created (~350
lines). A complete dispatch brief for Phase 1 (sandbox path resolution) per
the delegation protocol: verbatim pastes of DESIGN.md §3.1–3.2, BUILD-PLAN
Phase 1, AGENT-RULES.md §1/§3/§5/§6–7; an exact build spec (crate layout,
function signature, behaviour requirements incl. resolve-fully-then-check,
per-case rejection reasons, non-existent-tail handling, positive cases,
harness `resolve`/`demo` modes, tests); decisions marked ⚠ as
brief-author's-not-the-docs' for Muffin to veto (relative-path strictness,
lib+bin layout, non-existent-tail requirement); non-goals recorded honestly
(hardlinks, bind mounts, TOCTOU); required output_schema; dispatch
parameters.

*Superseded (Sep 2026, later same day):* the brief has since been revised and
approved. The ⚠ veto markers are gone, non-existent-path handling moved out to
Phase 1b (§8.4a), the harness gained a third `fixtures` mode, and
attack-list.md became the authoritative attack list. The paragraph above
describes the brief as first drafted, not as it stands — read the brief itself
for current content.

**Observed:** none — documentation only, nothing executed. Two factual
errors in my own brief were caught and fixed before dispatch: (1) claimed a
Rust `String` cannot contain NUL — false, NUL is valid UTF-8; brief now
requires explicit NUL rejection at the resolver layer; (2) TOCTOU was
missing from non-goals entirely.

**Not done (as of that moment — see the Superseded note immediately below):**
Phase 1 is NOT started. Dispatch waits only on Phase 0 completing (0c passing)
per BUILD-PLAN rule 2. No code exists in `crates/resolver/` — the directory does not
exist yet.

*Superseded 11 Sep 2026:* the paragraph above was true when written. Phase 1
has since been dispatched and built (`deleg_7b0c9983`); `crates/resolver/` exists and
is verified agent-side. The parent overrode the "wait for 0c" ordering — see
HANDOFF.md §7, "Amended 11 Sep 2026", for the reasoning. This entry is kept as
written because it is a record of that session's state at that moment.

**Uncertain:** none outstanding. The three decisions originally flagged for
veto were resolved (Sep 2026): relative-path rejection accepted, crate
layout accepted, non-existent-path handling deferred to Phase 1b. The brief
is approved; everything else in it is quoted from the docs.

**User action:** none. The brief is reviewed and approved, and it was dispatched
on 11 Sep 2026 (`deleg_7b0c9983`) — Phase 1 is built as a result. The three ⚠
decisions were resolved (relative-path rejection accepted, crate layout
accepted, non-existent-path handling deferred to Phase 1b).

*(Corrected 12 Sep 2026: this line read "Dispatch happens after 0c passes, per
BUILD-PLAN rule 2." That ordering was deliberately overridden on 11 Sep 2026 —
see HANDOFF.md §7. The dispatch already happened, so the instruction was stale
and pointed Muffin at a blocker that no longer applied. The entry above it
already carried the correction; this line, at the bottom of the same entry, had
been missed — the exact stale-reference pattern flagged in HANDOFF §6.)*

---

### Session note — user confirmations received in chat (Sep 2026)

User confirmed 0b by chat message ("go", Sep 2026) after reading the 0b
report; 0a was confirmed in the prior session. **Both entries were moved to
"Verified hands-on" on 13 Sep 2026, at Muffin's instruction** — see the 0a/0b
entry there, which records that the confirmation was by chat rather than by a
watched run. Phase-0 spike order continued to 0c, which passed hands-on 13 Sep
2026; Phase 0 is closed.

---

### Phase 0c MCP remote-transport spike — BUILT, BLOCKED ON TOKEN, PAUSED by user (Sep 2026, glm-5.3 session)

**Superseded in part (Sep 2026, later same day):** the "compiles clean" claim
below was true but hid a fatal gap — the crate had no TLS backend, so it would
have failed at the first network call even with a valid token. Fixed; the
transport is now proven against a public no-auth server. See the entry above
this one. The *blocked-on-token* status still stands.
*(Superseded 13 Sep 2026: the token run happened and 0c passed hands-on. This
entry is left as written so the pre-sign-off record stays honest — see
"Verified hands-on" at the top of this file.)*

**Changed:** `spikes/0c-mcp-http/` created — cargo project using `rmcp` 3.3.0
(features `client`, `transport-streamable-http-client-reqwest`). One file:
`src/main.rs`. Connects to `https://api.githubcopilot.com/mcp/readonly`
(read-only by URL path, verified against GitHub's remote-server docs), auth
token from `GITHUB_PERSONAL_ACCESS_TOKEN` env via the transport's
`auth_header` field — never hardcoded, never printed. Plans:
`list_all_tools()` then call `get_me` (read-only, zero side effects).
README with resume instructions + findings.

**Observed (agent-run):** `cargo build` → exit 0. `cargo run` without token →
`Error: GITHUB_PERSONAL_ACCESS_TOKEN not set`, exit 1 — clean refusal, no
invented credentials. **Blocked:** no GitHub credential exists anywhere on
this machine (checked env, `gh`, `~/.git-credentials`, `~/.netrc`,
`~/.config/gh` — all absent). Local Docker variant needs the same PAT, so it
is not a workaround. **Zero network calls to GitHub have been made** — the
remote round-trip is unproven.

**Not done:** the actual remote connection, tool list, and tool call — the
entire point of 0c. Everything else is done and compiles.

**Uncertain:** whether GitHub's remote endpoint accepts the request as
constructed — plausible (API surface verified against rmcp source and
GitHub's own docs) but untested.

**User action to resume:** create a fine-grained PAT (no scopes needed),
`export GITHUB_PERSONAL_ACCESS_TOKEN=…`, `cargo run` in
`spikes/0c-mcp-http/` — expected output in that README. **Paused by user
decision** — proceed to other work meanwhile.

---

### Phase 0b MCP client spike — VALIDATED agent-side; stop-and-rethink NOT triggered (Sep 2026, glm-5.3 session)

**Changed:** `spikes/0b-mcp-client/` created — cargo project using `rmcp`
3.3.0 (features: `client`, `transport-child-process`, `transport-io`) +
`tokio` + `serde_json`. One file: `src/main.rs`. Spike README with full
findings and verdict.

**Observed (agent-run):** `cargo run` → connected to
`@modelcontextprotocol/server-everything` over stdio via `TokioChildProcess`,
listed **13 tools**, called `echo` with a message argument, got back
`Echo: hello from atrium 0b spike`, `is_error: None`, exit 0. Full verbatim
output in `spikes/0b-mcp-client/README.md`. Three compile errors on the way,
all rmcp 3.3.0 API-surface drift from older docs (feature-gated `client`,
`ContentBlock` enum rename, `with_arguments` plural) — all fixed in one
build cycle, none conceptual. Backend language decision stands.

**Not done:** nothing user-visible to verify beyond the printed output — 0b
is a terminal spike; BUILD-PLAN's verification is "see the tool list printed,
and see a tool call return a result", which the agent observed directly and
recorded verbatim. User confirmation that this satisfies the 0b gate is what
moves it to verified.

**Uncertain:** none material. rmcp is young and its API moves — for the real
build, pin the version and re-check the surface at build time (see README
recommendations).

**User must confirm:** run `cargo run` inside
`"spikes/0b-mcp-client"` and see the tool list + echo round-trip print
(it needs node/npx for the spawned server; first run may download the
package). Then move this entry to verified hands-on per the ledger rules.

---

*(previous entries below — 0a bridge spike:)*

### Phase 0a bridge spike — built, transport verified, GUI pending (Sep 2026, glm-5.3 session)

**Changed:** `spikes/0a-bridge/` created — `bridge-rust/` (std+serde TCP server,
127.0.0.1:42321, newline-delimited JSON), `bridge-flutter/` (Linux debug build,
text field + Send + status), `test-bridge.py` (4-probe raw client),
`dart_client_test.dart` (headless replica of the app's send logic),
`start-server.sh` (setsid-detached launcher). Spike README with verdict.

**Observed (agent-run):**
- `cargo build` OK (5.39s); `flutter build linux --debug` OK.
- `python3 test-bridge.py` — 4/4 probes PASS (ping→pong, echo round-trip,
  malformed JSON → app_error with reason, unknown type → app_error).
- `dart run dart_client_test.dart` — echo round-trip 22 ms; `pkill -x bridge-rust`
  then send → `Connection error: Connection refused` immediately, no hang, no
  crash. Kill-test PASS.
- Server restarted after the kill-test (pid 1480795) and probes re-PASS 4/4.

**Not done:** GUI not exercised — no human pressed the button. Widget wiring
inferred from identical headless logic, not watched.

**Addendum (same session, after ad-hoc verification):** the scaffolded
counter test (`test/widget_test.dart`, still targeting the deleted `MyApp`)
was found broken by `flutter analyze` and replaced with a real widget test:
pump `BridgeApp`, enter text, tap Send, poll for reply against the live
server. **Observed: `flutter test` → `+1: All tests passed!`** So the
wiring (button → `_send()` → socket → setState repaint) is now
test-observed, not inferred. What remains unobserved is only the human part:
real keyboard/display interaction. `flutter analyze` reports 14 issues, all
`avoid_print` infos in the deliberate CLI probe `dart_client_test.dart`.

**Uncertain:** none about the transport. GUI is unobserved, that's the point of
the hands-on gate.

**User must verify by hand (BUILD-PLAN 0a gate):**
1. `bash "spikes/0a-bridge/start-server.sh"` (server is running now, pid
   1480795; skip if still up)
2. Run the app: `cd "spikes/0a-bridge/bridge-flutter" && /home/muffin/flutter/bin/flutter run -d linux`
   (or launch `build/linux/x64/debug/bundle/bridge_flutter`)
3. Type text, press Send → reply appears under the button.
4. `pkill -x bridge-rust`, press Send again → status shows
   `Connection error: …` promptly, app does not hang or crash.

On user confirmation, move this entry to verified hands-on; 0a done, 0b next.

*(previous entry:)*
*(empty — no code has been written)*

---

## Known-broken / blocked

*(Empty again.)*

Two open questions were recorded here on 12 Sep 2026 during the consistency
sweep — neither was a defect in anything built, both were decisions the docs left
disagreeing or silent. **Both were decided by Muffin on 12 Sep 2026:**

1. **`DESIGN.md` §3.2 "allowed roots" (plural) vs the resolver's single root** —
   **decided: loose wording. `resolve()` stays single-root permanently.** Host
   access is off entirely in v1, so there is one root and no mount table. Mounts,
   when they arrive, get their own layer *above* the resolver that maps a virtual
   path to a `(root, subpath)` pair and calls `resolve()` unchanged. Reason: a
   wider signature would mean re-verifying the resolver and its attack list every
   time the mount logic changed. §3.2 now reads "the environment root" with a
   dated clarification beneath it. See `DECISIONS.md`.
2. **Phase 1b had no home in `BUILD-PLAN.md`** — **decided: it becomes its own
   numbered phase, `PHASE 1b`, between Phase 1 and Phase 2.** Not folded into
   Phase 2a; the split exists so non-existent-path handling is verified in
   isolation, and folding it back would recreate the ambiguity the split avoids.
   The entry now exists in `BUILD-PLAN.md` with requirement text quoted verbatim
   from the brief's §8.4a, plus a requirement for its own attack list written
   before the code. See `DECISIONS.md`.

Neither was ever a blocker for Phase 1 sign-off.

---

## Notes for the next session

- `DESIGN.md`, `DECISIONS.md`, `BUILD-PLAN.md`, `AGENT-RULES.md` and
  `OPEN-QUESTIONS.md` are complete and current as of the design session that
  produced them.
- **Phase 1's resolver is built and SIGNED OFF HANDS-ON by Muffin on 13 Sep
  2026** (`crates/resolver/`, crate `atrium-resolver`). The gate is passed; see
  "Verified hands-on".
- **Phase 1b — non-existent-path resolution — is built and SIGNED OFF HANDS-ON
  by Muffin on 13 Sep 2026.** Its own phase, with its own attack list written
  before the code. The gate is passed; see "Verified hands-on".
- **Phase 2a — file operations — is built and SIGNED OFF HANDS-ON by Muffin on
  14 Sep 2026** (`crates/fileops/`, crate `atrium-fileops`). Its own harness, 64 lines,
  zero failures. The gate is passed; see "Verified hands-on".
- **Phase 2b — the real shell — is SIGNED OFF HANDS-ON by Muffin on 14 Sep 2026**
  (`crates/shell/`, crate `atrium-shell`). Its own harness, 78 lines, zero failures, 5
  holes demonstrated on purpose. The gate is passed; see "Verified hands-on".
- **Phase 2c — snapshot and restore — is SIGNED OFF HANDS-ON by Muffin on 14 Sep
  2026** (`crates/snapshot/`, crate `atrium-snapshot`). Its own harness, 111 lines, zero
  failures, 0 holes. **The undo exists**, so "nothing autonomous runs before undo
  exists" is satisfied. The gate is passed; see "Verified hands-on".
- **Four crates now: `crates/resolver/`, `crates/fileops/`, `crates/shell/`, `crates/snapshot/` — all signed
  off.** Each separate so a signed-off thing cannot gain unverified code.
- **Phase 2e — strong confinement for `run commands` — is required, after 2d and
  before Phase 5.** "Working directory confined" is not confinement; a real
  command can still reach the host, and the plan's own Phase 2 verification block
  always demanded the opposite. See the 2e entry under "Agent-reported",
  `DECISIONS.md`, and `BUILD-PLAN.md` §2e. **Nothing autonomous may exercise the
  shell until 2e passes.**
- Phase 2's parts all route through the Phase 1b resolver, which is signed off.
- Phase 0 is throwaway spike code. Do not build architecture in it.
- Phase 0b is a stop-and-rethink gate: if the Rust MCP SDK fails badly, the
  backend language decision is reopened. See `DECISIONS.md` §Backend.
