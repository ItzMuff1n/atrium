# Atrium — Build Plan

Read `AGENT-RULES.md` before starting any phase. Read `DESIGN.md` for what is
being built and `DECISIONS.md` for why alternatives were rejected.

---

## The two rules that govern this plan

**1. Every phase ends with a verification the user performs by hand.**
Not a test the agent wrote. Not a report the agent produced. The user runs
something, observes something, and confirms it. If a phase cannot be verified
this way, the phase is defined wrong and must be redefined before it is built.

**2. No phase starts on an unverified foundation it actually depends on.**
The failure mode for a vibe-coded project is building on an unverified
foundation. By the time it is visible it is unrecoverable.

The default reading is the strict one: the previous phase's verification must
have passed. The dependency clause is the exception, and it is narrow — it
applies only when the unfinished work shares nothing with the phase about to
start: no code path, no library, no design question the earlier work exists to
answer. Wanting to make progress is not a dependency argument. If you have to
reason for more than a line or two about whether something is a dependency, it
is, and you wait.

**Worked example — spike 0c, 11 Sep 2026, still the only override.** 0c tests
MCP over remote HTTP with auth. Phase 1 was Rust-only: no MCP, no network, no
transport. 0b had already answered the question "spikes first" exists to answer
(is the Rust MCP SDK usable), and a spike that cannot run for want of a
credential is not a foundation at all. Phase 1 was dispatched while 0c sat
blocked.

**The judgment was right; the mechanism was wrong, and this is not blessed
precedent.** An agent made that call alone, inside a file edit, rather than
asking. `AGENT-RULES.md` §5 makes weakening a gate rule a stop-and-ask item and
§6 forbids adding an exception to one. Both were breached. The exception now
lives in this rule, so the same call in future is following a rule rather than
bypassing one — and a case this rule does not already cover is still a
stop-and-ask, not a second override.

**What 0c does block: Phase 9**, which is where MCP surfaces are built. It does
not block Phase 1b or Phase 2. This is written here, in the rule, because it was
re-argued from scratch twice — the rule read as absolute while the overrides
lived in notes elsewhere. Reasoning also in `HANDOFF.md` §7, "Amended 11 Sep
2026".

Record every verification in `STATUS.md`, in the correct section — **verified
hands-on** or **agent-reported, unverified**. Never move an item between those
sections without a hands-on check.

---

## Scope warning

What `DESIGN.md` describes is not a v1. Orchestration, multi-agent operation,
workspaces and the surface store are v2 and v3 material. **If everything ships
at once, nothing ships.**

**v1 ends at Phase 9.** Single agent, real environment, four surfaces, MCP, the
gate, snapshots. Complete, useful, demonstrable.

Phases 10+ are the rest of the design, sequenced. They are not optional
long-term; they are optional *now*.

---

# PHASE 0 — Spikes

**Purpose:** prove the two technologies that could still kill the project.
Throwaway code. Do not build architecture here.

## 0a. The bridge

Build a Rust process and a Flutter app that talk over a local socket with JSON
messages.

- Flutter has one button and one text field.
- Pressing the button sends a JSON message to the Rust process.
- Rust replies.
- Flutter displays the reply.

Not FFI. A socket.

**Verification (user):** press the button, see the reply appear. Kill the Rust
process, press again, see a clean error rather than a hang or crash.

## 0b. The MCP client

In Rust, using the Rust MCP SDK, connect to the official `everything` reference
server over stdio.

- List its tools.
- Call one.
- Print the result.

`everything` is chosen deliberately: it is maintained by the MCP steering group
and exists to exercise protocol features rather than do useful work. If the
client fails against it, the fault is in Atrium's code, not the server's.

**Verification (user):** see the tool list printed, and see a tool call return a
result.

**Stop-and-rethink condition:** if the Rust MCP SDK fails in ways that are not
quickly fixable, the backend language decision changes. This must be discovered
now, before there is a codebase. See `DECISIONS.md` §Backend for the fallback.

## 0c. MCP over remote transport

Connect to GitHub MCP **in read-only mode** over streamable HTTP. List tools.
Call one read-only tool.

Local stdio and remote HTTP are different code paths. Both are needed
eventually; finding out now is cheap.

**Verification (user):** see a real result from a remote server.

---

# PHASE 1 — Sandbox path resolution

**Rust only. No UI. No agent. No filesystem operations yet.**

One function: agent-supplied path in, resolved real path out, or a rejection.

This is the only part of Atrium where a mistake is *dangerous* rather than
annoying. It is built alone, first, and hardened before anything else exists.

**Requirements:**

- Resolve a virtual path (`/home/documents`) against the environment root.
- Reject anything that escapes the root, after full canonicalisation.
- Symlinks are resolved before the check, not after.
- Rejections return a reason, not a bare failure.
- The function is the *only* way any other code turns a virtual path into a real
  one. Nothing else in the codebase may construct a real path.

**Verification (user) — the most important verification in the project:**

The user writes the attack list. Not the agent. At minimum:

- `../../..` and deeper
- `/../..` from the root
- absolute host paths (`/etc/passwd`, `/home/muffin`)
- a symlink inside the environment pointing outside it
- a symlink pointing to another symlink pointing outside
- trailing dots and spaces
- null bytes in the path
- unicode lookalike separators
- very long paths
- `.` and `..` interleaved (`a/./../../..`)
- a path that resolves inside the root but passes through outside it

Every one must be rejected, and the user must watch each rejection happen.

**Superseded 11 Sep 2026 — "the user writes the attack list."** Muffin has no
security background and should not be asked to author path-traversal attacks.
Claude wrote `attack-list.md` instead, and the **independence requirement was
met a different way**: a second attack list from a subagent that saw none of
the resolver source, the brief, or the first list (`blind-attack-list.md`),
with every novel line run against the resolver. The two-list mechanism replaces
the requirement rather than waiving it. See `HANDOFF.md` §4, "The independence
replacement", and `DECISIONS.md`.

The rest of this verification stands unchanged: **Muffin watches each line's
verdict himself.** That is the part that needs no security background —
ACCEPT or REJECT is readable by anyone — and it is still the gate.

**Resolved 12 Sep 2026 — Phase 1b is now in this plan.** Non-existent-path
resolution was split out of Phase 1 during the brief's review. It was defined
only in `delegation-briefs/phase-1-path-resolution.md` §8.4a and had no home
here. By decision (Muffin, 12 Sep 2026) it becomes **its own numbered phase,
`PHASE 1b`, immediately below** — between Phase 1 and Phase 2. It was
deliberately *not* folded into Phase 2a: if non-existent-path handling shipped
alongside file-operation plumbing and the attack list found a hole, there would
be no way to tell which half was wrong. §8.4a remains the origin of the
requirement.

**Do not proceed to Phase 2 until this passes.** Everything after this point
assumes the sandbox holds.

---

# PHASE 1b — Paths that do not exist yet

**Built and SIGNED OFF HANDS-ON by Muffin, 13 Sep 2026.** The gate described
below is passed; see `STATUS.md` "Verified hands-on" for what he ran.

**Rust only. No UI. No agent. No filesystem operations.**

**Requirement (verbatim from `delegation-briefs/phase-1-path-resolution.md`
§8.4a, which remains the origin of this requirement):**

> Resolving paths that do not exist yet is NOT part of Phase 1. std::fs::canonicalize fails on a path whose target is absent, so this resolver only handles paths that already exist on disk. That is intentional and sufficient for Phase 1 verification.
>
> Phase 1b will add it: canonicalise the longest existing prefix, then validate the non-existing remainder so it cannot traverse. It is split out because it is the only part of the resolver that reasons about something the filesystem cannot confirm, and it gets its own isolated verification round against its own attack list. Phase 2 depends on it, so it is postponed rather than dropped.
>
> Do not build it, do not stub it, do not add a flag for it. If a test case requires it, that test case belongs to Phase 1b.

*(End of verbatim quote. The paragraph below is new.)*

**Why it is its own phase and not part of Phase 2a.** Non-existent-path handling
is the only part of the resolver that reasons about something the filesystem
cannot confirm. If it shipped alongside Phase 2a's file-operation plumbing and
the attack list found a hole, there would be no way to tell which half was
wrong. The split exists precisely so the two can be verified in isolation.

**Verification — its own attack list, written before the code:**

Phase 1b needs an attack list covering **non-existent paths** specifically:
paths whose target is absent, paths whose target is absent *and* whose parent
chain walks outside the root, a non-existent remainder attached to a symlinked
existing prefix, and the same for a path that creates a traversal only after the
non-existent tail is appended. Written before the code, in the same shape as
Phase 1's two lists, and watched by Muffin line by line like the first one.

**Phase 2 does not start until Phase 1b passes.** Resolution must hold for paths
that do not exist before anything creates files.

**Not built, not stubbed, no flag exists for it today.** Phase 1 deliberately
contains none of this; any Phase 1 test case covering it belongs here instead.

*(Superseded in part, 13 Sep 2026. The two paragraphs above were written while
this phase was unbuilt and are kept as the record of what the plan required. By
13 Sep 2026 the phase was built and its hands-on gate passed: an absent path
whose resolved location is inside the environment root now ACCEPTs, and escapes
still REJECT with a reason naming the specific step. The line "no flag exists for
it today" is no longer true. The requirement paragraph and the verbatim quote
above remain exactly as written and are still binding.)*

---

# PHASE 2 — The environment

**Rust only. Still no UI, still no agent.**

## 2a. File operations

Create, read, write, move, delete, list — all routed through Phase 1's resolver.

## 2b. The real shell

> **SIGNED OFF HANDS-ON by Muffin, 14 Sep 2026.** The crate is `shell/`; the
> harness is `bash shell/hand-test-2b.sh` — 78 lines, zero failures, 5 holes
> demonstrated on purpose. 2c comes next; **2e is required before anything
> autonomous exercises this.**

Run actual commands with the working directory confined to the environment root.
Capture stdout, stderr and exit code. **Confines where a command starts and what
it is handed — NOT what it can reach; that is 2e.**

> **Correction, 14 Sep 2026 — what "confined" does and does not mean.**
> "Confined" here means the **starting directory** only. The command itself runs
> as a real program with the user's own access, on the host. This section
> previously read as though the sandbox held against a command; it does not, and
> no path check can make it. Observed in 2a: a **hard link** inside the root let
> the sandbox read *and overwrite* a file outside it, because a hard link is a
> second name for one file, not a path — nothing about it looks wrong to a path
> check. A real command is worse: it can `cd /` and act anywhere the user can.
> **The cage is `2e`, below, and it is not optional.**

## 2c. Snapshot and restore

Snapshot the environment root. Copy-on-write if the filesystem supports it,
otherwise a plain copy. Restore from a snapshot.

Snapshot lands here, before anything autonomous touches files. **Nothing
autonomous runs before undo exists.**

> **Built 14 Sep 2026 — SIGNED OFF HANDS-ON by Muffin the same day. The gate is
> passed.** The crate is `snapshot/`; the harness is `bash snapshot/hand-test-2c.sh`
> (111 lines, 0 failures). See `STATUS.md` "Verified hands-on" for what he ran, and
> `phase-2c-evidence.txt` for command-by-command output.
>
> **The plain copy was taken, not copy-on-write** — `DESIGN.md`'s own sanctioned
> fallback. `DECISIONS.md` records why: the environment root's filesystem is 2e's
> unmade decision and the plan's likely answer (a tmpfs) cannot reflink at all
> (observed: "Operation not supported"), `std` cannot issue `FICLONE`, and the cost
> has now been **measured** rather than assumed — 5,000 small files in 277 ms.
>
> **Two things 2c does not do, so a later session does not read them into it:**
> it does not protect the store from a same-user command (that is 2e's boundary),
> and it does not make snapshots happen automatically ("before every task" is
> Phase 5's caller). Both are stated in `STATUS.md` and in `snapshot/README.md`.

## 2d. Filesystem watcher

> **Built 15 Sep 2026 by the parent session; awaiting Muffin's hands-on gate. The
> crate is `watcher/`; the harness is `bash watcher/hand-test-2d.sh` — 16 lines,
> zero failures, zero real-path leaks.** See `STATUS.md` "Agent-reported" for what
> was run, and `phase-2d-evidence.txt` for command-by-command output.
>
> **Two defects found after the phase was already green, both recorded rather than
> tidied:** a populated subtree moved *into* the root was watched only at its top
> level (found by the independent blind list), and the header line printed the
> root's real on-disk path (found by this phase's own harness). Both fixed, both
> proven load-bearing by reintroducing them.
>
> **No dependency was added**, deliberately: the inotify entry points are declared
> as `extern "C"` symbols resolved against the already-linked libc, so nothing is
> fetched from a registry. Allowed by `DECISIONS.md`, which records the rejected
> alternatives (`libc`, raw `syscall`, polling).
>
> **What 2d does not do, so a later session does not read it in:** it does not cage
> a command (2e), does not gate or block anything (Phase 4), and emits **change
> notifications, not effects** — Phase 3 turns them into effects and owns the
> de-duplication against the file-operation layer. Linux only.

Watch the environment root for changes made outside the file-operation layer
(i.e. by shell commands). Emit change notifications. They have nowhere to go yet
— Phase 3 gives them one.

**Mechanism (decided 15 Sep 2026, measured before the spec was written):** Linux
`inotify`, **one watch per directory** — the kernel is not recursive, measured. The
notifications are `Change` records: created / modified / deleted / moved / attributed
/ directory-gone / **overflow** / error. `IN_ACCESS` is deliberately not watched (a
read is not a change) and `IN_MODIFY` is consumed in favour of one record per
completed edit.

## 2e. Strong confinement for `run commands`

**Added 14 Sep 2026 — the hole 2b cannot close, and the phase that closes it.**

**The requirement, mechanism-agnostic.** A command run through the shell must be
unable to reach anything outside the environment root — not "should not", not
"we check its paths first". Inside the command's own view of the filesystem,
outside the root **does not exist**. Read, write, delete, rename, `cd /`, a
script that walks upward, a program that opens `/etc/passwd` by absolute path:
all of them must fail because there is nothing there to reach.

**Why it is its own phase and not folded into 2b.** The same reason Phase 1b is
its own phase: isolated verification. A cage is a piece of security machinery
with its own attack list, and if 2b shipped alongside it and a hole were found,
there would be no way to tell whether the hole was in the command runner or in
the cage. Two hard things in one phase is exactly the ambiguity the 1b split was
made to avoid (`DECISIONS.md` — "Phase 1b is its own phase, before Phase 2").

**Why it cannot be deferred.** `BUILD-PLAN.md`'s own Phase 2 verification block
has always included: *"Try `cd /` and then a destructive command. Confirm it
cannot reach the host."* That step is not satisfiable by 2b, and deleting it
would be deleting the gate. Strong confinement is therefore **required before
anything autonomous exercises the shell** — it gates **Phase 5**, the agent loop.
Nothing autonomous runs before undo exists (2c); nothing autonomous runs through
a shell that can reach the host (2e).

**Known mechanism, checked on this machine (14 Sep 2026, observed).** An
OS-level cage does the job — the command gets a private view of the filesystem
where the environment root is mounted at `/`, so virtual paths line up exactly as
Phase 1 defines them (`/home/documents` **is** the root's `home/documents`).
Verified working unprivileged: bubblewrap 0.12.0 (a caged process could not see
`/home`, `/etc/passwd` did not exist inside, writes landed in the root, `python3`
still ran, the host was untouched); user namespaces + bind mounts work directly
(`unshare -Urm`); Landlock is compiled **and active** (`/sys/kernel/security/lsm`
lists it); podman and docker are installed. Which one becomes the implementation
is 2e's decision, not this section's — the *requirement* is what is fixed here.

**Also owed by 2e, carried over from 2a:** the environment root must be **its own
mount point** (its own filesystem or tmpfs). Cross-filesystem hard links are
impossible — observed in 2a: `Invalid cross-device link`, `/tmp` being device 50
and `/home` device 49 — so an isolated mount is what stops a hard link from being
the hole it currently is. Recorded there as open item L.4; it belongs here.

**Verification (user):**

- **The moved step, kept verbatim:** *"Try `cd /` and then a destructive command.
  Confirm it cannot reach the host."*
- Plus, inside the shell, try to reach the host and fail: `cat /etc/passwd`, list
  and write to a real directory outside the environment.
- Then the other direction: confirm a file created **inside** the environment does
  appear on the host inside the environment root — the cage must not be so tight
  that the sandbox cannot be used.

Watched line by line, from an attack list written **before** the code, as Phases
1, 1b and 2a have been.

**Verification (user) for 2a–2d — this block does NOT include the host step:**

- Drive the file operations by hand. Try to escape. Fail.
- Run shell commands. Confirm the working directory is confined.
- Snapshot, delete everything in the environment, restore, confirm it is back.
- Run a shell command that creates a file. Confirm the watcher noticed.

> *(Moved 14 Sep 2026: "Try `cd /` and then a destructive command. Confirm it
> cannot reach the host" has been removed from this block and belongs to **2e**.
> It was never satisfiable here, and leaving it would have made this block
> unpassable — or worse, passable only by pretending. It is not deleted; it moved
> to the phase that can answer it.)*
>
> *(Also 14 Sep 2026: 2a was verified by Muffin as its **own** gate — he ran
> `fileops/hand-test-2a.sh` on its own, 64 lines, zero failures, and signed 2a
> off. So this block no longer describes one gate covering all four parts. Read it
> as the standing list of what a user verification of Phase 2 covers: 2a has
> already been watched; 2b, 2c, 2d remain, and 2e has its own block above. The
> sentence below has been left as written — it was the plan when it was written;
> this note is what changed.)*
>
> *(Updated 15 Sep 2026: the last line of this block — "Run a shell command that
> creates a file. Confirm the watcher noticed" — is **2d's**, and 2d is built and
> awaits its own gate: `bash watcher/hand-test-2d.sh`. The line about "confirm it is
> back" after a snapshot is 2c's and was signed off 14 Sep 2026.)*

---

# PHASE 3 — The effect stream

Every operation from Phase 2 emits an effect record.

Fields: `kind`, `target`, `agent`, `surface`, `count`/`size`, `source`,
`follow_pane`, `label`, timestamp.

Kinds for now: `created`, `changed`, `moved`, `deleted`, `read`, `ran`,
`reported`. (`fetched` and `linked` arrive with MCP in Phase 9.)

Effects go to SQLite and to a text log. **No visuals yet.**

The filesystem watcher from 2d emits effects for whatever shell commands did.
The `ran` effect covers the command and its output; the watcher covers its
consequences.

**Verification (user):** perform a set of operations by hand — create a file,
edit it, move it, delete a folder, run a shell command that writes a file. Then
read the log. It must describe what was actually done, with correct counts.

---

# PHASE 4 — The gate and the kill switch

Rules run against tool calls **before** execution. This must exist before
anything autonomous runs.

**Rules to implement:**

- path outside the sandbox → reject
- a tool the agent is not currently scoped to → reject
- identical call with identical arguments N times in a row → reject
- destructive action above a threshold (delete count, file size) → reject or ask
- rate limit: destructive effects above N per minute → pause and ask

**Every rejection returns an explanation**, not a bare failure. "Denied — that
path is outside the environment. Available paths: ..." The agent re-plans.

**Per-rule setting:** `allow` / `deny-and-explain` / `ask-me`.
**Default everything to `deny-and-explain`.** Individual rules move to `ask-me`
later, once something proves annoying in real use.

**Kill switch:** one command that stops every agent at its next action boundary
and releases nothing. Deliberately crude. It must work when everything else is
confused.

**Testable without an agent** — feed it fabricated tool calls.

**Verification (user):** hand the gate a call targeting `/etc`, a 500-file
delete, and the same call five times running. All rejected, each with a reason
the user can read and understand. Then trigger the kill switch during a long
shell command and confirm it stops.

---

# PHASE 5 — The agent loop (one agent)

Model in → tool call out → gate → execute → effect → repeat.

**No locks. No tasks. No orchestration. No workspaces.** One agent, one job.

**Includes:**

- Model configuration by API key, provider-agnostic.
- Tool definitions built from what Phase 2 exposes.
- A conversation loop with tool results fed back.
- **A budget cap.** This is the first phase that costs money. A cap that pauses
  and asks — never one that kills — must exist here, not later.
- Agent states emitted: `thinking`, `acting`, `running`, `idle`, `failed`.

**Verification (user):** give the agent a small job — "create a folder called
notes and write three files in it summarising what is in the environment". Watch
the effect log. Confirm the files exist and the log describes what happened.
Then set a very low budget and confirm it pauses rather than dying.

**This is the first time Atrium works.**

---

# PHASE 6 — First UI: desktop, island, notes surface

Flutter enters the picture properly.

**Build:**

- A desktop window.
- The floating island: agent status dots (four group colours), current task,
  kill switch, budget remaining.
- **One surface: notes.** A window showing note contents.
- **The first animation:** text appearing as the agent writes a note. Replayed
  over the agent's total elapsed time at an even pace — *not* the model's bursty
  token timing. Floor for short text, speed ceiling for long.
- Effects from Phase 3 drive the UI over the Phase 0a bridge.
- A crude effect log view. This grows into the overseer at Phase 13.

**Why notes first:** cheapest animation that proves the entire thesis. No icons,
no layout, no drag-and-drop, no z-order.

**Verification (user):** ask the agent to write a note. Watch the text appear in
the window. Watch the island dot change from thinking to acting to idle.

**This is the moment the project visibly exists.**

---

# PHASE 7 — File explorer and the movement animation

The signature visual.

**Build:**

- The file explorer surface: folders, files, icons, navigation.
- Effects drive it: `created` appears with a highlight, `changed` pulses,
  `moved` travels old → new, `deleted` fades out.
- **One animation queue per surface**, not one global queue.
- **Bulk delete:** the explorer opens to the location, icons vanish
  sequentially, speeding up with volume. Not 400 simultaneous animations.
- Windows with z-order. Agents open them.

**Verification (user):** ask the agent to reorganise a folder. Watch the icons
travel. Then check the actual disk contents and confirm they match what was
animated. Then ask it to delete a folder with several hundred files and confirm
the sequential animation is watchable rather than a freeze.

---

# PHASE 8 — Terminal and editor surfaces

**Terminal surface:**

- Streams command output live.
- Saves scrollback as text across restarts.
- Never attempts to restore a live process.

**Editor surface:**

- Opens files, shows content, allows the user to edit.
- **Live refresh:** when an agent saves, a clean view refreshes silently; a
  dirty view prompts (keep-mine / take-theirs).
- The prompt **names the agent and the task**, not "changed on disk".
- **Continuous autosave** of the user's drafts to a side file, crash-recoverable.
- Separate window from the explorer.

**Verification (user):** open a file in the editor. Have the agent modify it.
Confirm the view refreshes and the prompt names the right agent. Then type
something without saving, have the agent modify the same file, and confirm the
prompt appears rather than the work being lost. Then kill the app mid-edit and
confirm the draft survives.

---

# PHASE 9 — Manifests, effect mapping, generic surface, real MCP

**The plugin architecture arrives. This is the largest single phase.**

## 9a. The manifest format and loader

Implement the format from `DESIGN.md` §5.5. **One loader** — every pane reads
manifests through the same shared code.

## 9b. Rewrite existing surfaces as manifests

Explorer, terminal, editor and notes all become manifest-declared surfaces.

**This is the test of whether the format works.** If rewriting them is painful,
the format is wrong, and this is the cheapest possible moment to find out.

## 9c. The vocab file and stable ids

Effect ids, labels, animations, aliases. Defined in exactly one place in code;
everything else refers to it. Unknown ids fall back to a generic visual.

## 9d. Scoped tool loading and the catalog

The overseer holds the full tool inventory. Navigation tools always loaded;
a surface's tools load on entering it; surfaces above a threshold load only a
tool-search tool.

**Selection must stay stable within a task** — reshuffling breaks provider prompt
caching and can raise cost even while reducing tool count.

## 9e. MCP surfaces

Connect real MCP servers as surfaces. In this order:

1. `everything` — protocol correctness
2. GitHub read-only — remote transport and auth
3. Excalidraw (`npx -y mcp-excalidraw-server`) — a real 26-tool surface

## 9f. The generic surface

Any MCP server renders with **zero custom code**, driven by effect kinds alone.

## 9g. Auto-classification of unmapped tools

Diff the tool list on connect. New tools render generic immediately. A model
classifies in the background. Suggestions sit **pending** in the overseer, never
auto-applied. Destructive effects always require approval. Confidence shown.

**Verification (user):**

- Confirm the four rewritten surfaces still work identically.
- Connect Excalidraw and confirm its 26 tools appear.
- Have the agent draw something and confirm the generic surface shows sensible
  effects — creations, changes, deletions in the right order with the right
  targets.
- **If it reads as mush, the effect vocabulary is wrong.** That is a real
  finding, not a bug — stop and revise the vocabulary before continuing.
- Confirm unmapped tools render generic and appear as pending in the overseer.
- Confirm nothing was auto-applied.

---

# ▲ v1 ENDS HERE ▲

Single agent, real environment, four native surfaces, MCP, generic surface, the
gate, snapshots, budgets. Complete, useful, demonstrable.

Everything below is the rest of the design.

---

# PHASE 10 — Tasks as objects

- The task object: id, instruction, agent, state, locks, allowed surfaces,
  parent, effect history.
- **Outcome records from day one**: effort used, tokens spent, success, retries,
  scope requests. The effort optimizer depends on these existing.
- Pause and resume. Pause finishes the current action, then stops.
- Budgets attached to tasks. Ceiling pauses and asks; 80% warning.
- Snapshot before every task.
- Everything survives restart, in SQLite.

**Needed before locks**, since locks are held by tasks.

**Verification (user):** start a task, pause it, close the app, reopen it,
resume it, confirm the agent continues correctly. Confirm the snapshot exists
and can restore.

---

# PHASE 11 — Locks, workspaces, multiple agents

**The biggest behavioural phase. First time two agents can collide.**

- Directory locks, read-write semantics, lock ordering, retries, then
  fail-and-re-plan.
- Heartbeat renewal; expiry ~60s; countdown visible only in the final 20s.
- Lock UI: padlock in the agent's colour, no timer when healthy, distinct mark
  when paused.
- Per-agent workspaces. The desk is a *view* — windows and arrangement are
  per-agent, the environment underneath is shared.
- Browser-style tab strip for switching.
- Parent workspaces persist; child workspaces are removed at task end.
- Agent notes, readable by all.
- The handoff warning dialog (agent / task / recency, three buttons).
- The hold: whole agent, ~30s inactivity auto-release, no lock changes.
- Human actions enter the same effect stream, tagged as user actions.
- Paused-task window: hideable, never removable, names who is blocked.

**Verification (user):** run two agents in overlapping directories. Confirm one
blocks and idles rather than corrupting. Confirm the padlock shows the right
colour. Kill an agent process and confirm its lock expires with a visible
countdown. Edit a file an agent is working in and confirm the warning names the
right agent and task. Use the hold and confirm it auto-releases.

---

# PHASE 12 — Orchestration

- Parent and child tasks.
- Continuous re-planning on child events, slow interval backstop.
- Compact status table as orchestrator input, never full transcripts.
- Scope ceiling inheritance.
- Token budget bounding child spawning; hard depth limit.
- Cancellation as a request with a defined wind-down.
- Approve-first plans, per-task "just run it" toggle.
- The keybind task window with the pre-filled options panel.
- The model scoring ruleset — query the provider's model list at runtime, rank
  by context size, cost, tool-calling reliability, latency. Never a hardcoded
  table.

**Verification (user):** give it a task needing several agents. Confirm the
options panel is pre-filled sensibly before anything runs. Confirm a child
failing causes a re-plan. Confirm a child cannot be given wider scope than its
parent.

---

# PHASE 13 — The overseer proper

Grows from the crude log added in Phase 6.

- Live logs, interleaved human and agent history.
- Pending effect mappings with confidence, approve/correct.
- Permission grants, with a visible list of what everything currently holds.
- Paused tasks, locks, budgets.
- Declared custom states, so misclassification is catchable.
- **Policy engine and dashboard stay separate in code.** The dashboard only
  reads; the policy engine never shows anything.

---

# PHASE 14 — Surface install and permissions

- Manual install with a UAC-style permission prompt.
- `run commands` gets its own confirmation, separate from the list.
- Warnings before installing unofficial surfaces.
- Grants revocable in the overseer.

No registry, no drag-and-drop yet.

---

# LATER

Roughly in order of value, not sequence:

- The drift audit (LLM manager on idle desks) — near-free supervision
- Agent faces and thought bubbles
- The idle desktop "needs you" panel
- The effort optimizer
- Media viewer
- Desktop ornaments
- Native external surfaces (Figma etc.)
- The browser, as an external Playwright process
- The surface store — **decide what "officially supported" promises first**
- Host access — and the permission model that must precede it
- Undo for the user's own actions

---

## Sequencing rationale

Why this order, so it is not rearranged casually:

- **Spikes first** because two technologies are new enough to change the stack.
- **Sandbox before anything** because it is the only dangerous part.
- **Snapshot before autonomy** because undo must exist before an agent can act.
- **Gate before the agent** because it is testable with fake calls and must
  exist before anything runs unsupervised.
- **Effects before UI** because the UI is a consumer of the effect stream, and
  the stream is verifiable as text.
- **Notes before the explorer** because it is the cheapest animation that proves
  the thesis.
- **Manifests after four working surfaces** because rewriting real surfaces is
  the only honest test of the format (`DESIGN.md` §5.6 rule 4).
- **Generic surface before native surfaces** because it carries the app for its
  first year and tests the vocabulary.
- **Tasks before locks** because locks are held by tasks.
- **Locks before orchestration** because orchestration creates concurrent agents.
