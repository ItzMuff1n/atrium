# Atrium — Design

Codename: Nexus. Product name: **Atrium**.

This document is the settled design. It is the source of truth for what Atrium
is and how it behaves. If code disagrees with this document, the code is wrong
or this document needs updating — resolve it explicitly, never silently.

Nothing in this document is a suggestion. Items still undecided are listed in
`OPEN-QUESTIONS.md`. Rejected alternatives and the reasoning behind them are in
`DECISIONS.md` — read that before proposing a change to anything here.

---

## 1. What Atrium is

Atrium is a desktop application containing a simulated desktop environment that
AI agents and the human operate inside **together**.

It is not a virtual machine. It has no OS, no drivers, no kernel. It simulates
a desktop: windows, a file tree, a shell, an editor, a taskbar-equivalent.

It is not a chat interface with a sidebar. The agent's actions produce visible
changes to a shared workspace, and the human can reach into that same workspace
and change things back.

### The thesis

Human-AI interaction is currently text-only, the way computing was before the
GUI. Agents emit logs; humans read logs. Atrium replaces the log with a place.

### The differentiator

Several tools visualise agent activity (see `DECISIONS.md` §Prior art). All of
them are **observation decks** — the human reads and interjects, but never acts
on the workspace itself.

Atrium's novelty is **shared state**: the agent moves a file, the icon moves on
screen, the human drags it back, and the agent is told. One workspace, two
parties.

### The governing principle

> **The user must always understand their own actions and the AI's actions.**

This is not a nicety. It is the test every design decision passes or fails:

> Does this make the user understand more, or just make the app do more?

Anything that fails this test is wrong for Atrium even if it is a good feature
elsewhere. This principle is why Atrium prefers a vague-but-honest visual over a
confident-but-guessed one, why unverified mappings never auto-apply, why the
warning dialogs show recorded facts instead of risk scores.

---

## 2. Stack

| Layer | Choice | Notes |
|---|---|---|
| Frontend | **Flutter** (Dart) | GPU-native. Verified on target hardware, see §2.1 |
| Backend | **Rust** | Sandbox, locks, gate, plugin loader, agent loop, MCP clients |
| Bridge | **Local socket, JSON messages** | Not FFI |
| MCP | **Rust MCP SDK** (Tier 1 since 2026-08-21) | New; expect rough edges |
| Persistence | **SQLite** | Tasks, effects, agent state, grants |
| Browser | **External process** (Playwright-style) | Deferred past v1 |

### 2.1 Hardware verification (passed, Sep 2026)

Tested on the target machine: i7-14700KF, RTX 5090 32GB, Arch Linux, KDE
Plasma 6, Wayland, 2560x1440.

| Load | FPS | Avg frame |
|---|---|---|
| 200 animated squares | 372 | 2.69 ms |
| 1000 animated squares | ~154–158 | ~6.4 ms |

60fps budget is 16.7 ms. Roughly 3x headroom under load far heavier than Atrium
will produce.

- Native Wayland (peers with `wayland-0`, no XWayland).
- Impeller backend on OpenGL ES. Not Vulkan, not Skia.
- **Zero** rendering errors, no flicker, no blank windows, no resize crash.

This test existed because Tauri's WebKitGTK has documented NVIDIA/Wayland
failures on exactly this hardware. Flutter cleared all of them. See
`DECISIONS.md` §Stack for why Tauri, Slint, egui, iced and TypeScript were
rejected.

**Do not change the stack without re-running an equivalent test.**

---

## 3. The environment

### 3.1 Shape

A **closed environment with its own filesystem**. One directory on the host is
the environment root. Everything an agent sees lives inside it. Paths are
translated: the agent asks for `/home/documents`, the app resolves it to
`<root>/home/documents`. The agent never learns the real path exists.

Specific host folders can be **mounted** into the environment at a chosen path
(`/mnt/project` → a real directory). Same mechanic as a VM shared folder.

**v1: host access is OFF entirely.** Environment root only. Not a denied
request — the option does not exist.

**The environment root is its own mount point** — its own filesystem or a tmpfs,
never a directory sitting inside a larger volume. *(Added 14 Sep 2026; this was
open item L.4 in `attack-list-2a.md`.)* Reason, observed: a **hard link** is a
second name for one file rather than a path, so no path check can see it — in 2a
a hard link inside the root let the sandbox read *and overwrite* a file outside
it. Cross-filesystem hard links are impossible (`Invalid cross-device link`;
`/tmp` is device 50, `/home` is device 49), so an isolated mount is what makes
that channel unreachable rather than merely documented. Same requirement applies
to whatever the root is mounted from in `BUILD-PLAN.md` 2e.

### 3.2 Path resolution — the most important code in the project

Every path an agent supplies is resolved and checked against the environment root
**before anything runs**. Without this check, `../../..` walks straight out of
the sandbox.

**Clarification, 12 Sep 2026 — mounts do not widen the resolver.** This sentence
previously read *"the allowed roots"*, plural, which read as though a path could
be checked against more than one root; §3.1's mounted host folders reinforced
that reading. Resolved by design decision (Muffin, 12 Sep 2026): **the resolver
always takes exactly one root, and that is permanent.**

When mounts arrive they get their own layer *above* the resolver. That layer maps
a virtual path to a `(root, subpath)` pair and then calls `resolve()` unchanged.
Mounts are real in the design but host access is off entirely in v1, so v1 has
exactly one root and no mount table.

Reason for keeping the signature narrow: `resolve()` is the one function where a
mistake is dangerous. Widening it to accept a set of roots would mean
re-verifying it — and its whole attack list — every time the mount logic changed.
Keeping it single-root means the mount layer can be wrong without the sandbox
being wrong.

Phase 2 onward builds on the current signature, as built and verified.

This is a known bug class (path traversal). It is the single place where a
mistake is dangerous rather than annoying. It is built and verified first, alone,
before any other subsystem exists. See `BUILD-PLAN.md` Phase 1.

### 3.3 The shell is real

The terminal runs actual commands on the host, with its working directory
confined to the environment root.

**Correction, 14 Sep 2026 — "working directory confined" is not confinement.**
This section says the command runs *on the host*, and that is exactly what
happens: it runs as a real program with the user's own access. Confining the
directory it starts in does not confine what it does. It can `cd /`; it can open
an absolute path; it can do anything the user can. The boundary the rest of this
design rests on — that nothing reaches outside the root — **does not exist for a
real command**, and no path check can create it, because a path check looks at
paths and a program is not limited to paths. (Observed in Phase 2a: a **hard
link**, a second name for one file, let the sandbox read *and overwrite* a file
outside the root. A command is a much larger version of the same gap.)

**Strong confinement is therefore its own build phase — `2e`,** and it is a
requirement, not an enhancement: commands must be unable to reach outside the
root because, in the command's own view of the filesystem, outside the root does
not exist. It gates Phase 5 — nothing autonomous exercises a shell that can reach
the host. Mechanism (an OS-level cage; bubblewrap, user namespaces, or Landlock —
all checked available on this machine 14 Sep 2026) is `BUILD-PLAN.md` 2e's
decision; the *requirement* lives here. See also §8, where "run commands" is
described as a different weight class — this is why.

It is **not** a reimplemented fake shell. Reimplementing coreutils, git, python
and npm is years of work that would never be 1:1, and a fake shell could never
run real builds — which the long-term goal requires.

Consequence: commands change files outside Atrium's own file layer, so the
explorer would go stale. A **filesystem watcher** on the environment root emits
the resulting effects. The `ran` effect covers the command and its output; the
watcher covers what the command did.

### 3.4 Snapshots

The environment root is snapshotted **before every task**. Copy-on-write if the
filesystem supports it, otherwise a plain copy.

This makes nearly every filesystem failure an undo rather than a loss. It is the
cheapest and most valuable safety mechanism in the design.

Snapshots protect the environment root only. Once host access exists, changes
out there are not covered — another reason host access stays off in v1.

**Effects sent to external apps are not recoverable.** A Figma frame deleted
through MCP is gone. Destructive actions against external apps therefore go
through an approval prompt instead, since there is no undo to fall back on.

---

## 4. Agents

### 4.1 Atrium owns the loop

Atrium runs its own thin agent loop: call model → receive tool call → gate →
execute against a surface → return result → repeat.

It does **not** integrate Cursor, Claude Code, or similar. Those own their own
UI and control flow; redirecting their tool calls into Atrium's surfaces means
fighting their design. Owning the loop makes models interchangeable parts.

Any model reachable by API key can drive it.

### 4.2 Agent identity

**Parent/orchestrator agents are long-lived identities**, not per-task
instances. They are called again, they accumulate notes, they tidy their
workspace over time.

Consequences: an agent needs a name and identity that survives restarts, its
notes persist alongside task data, and it will accumulate cruft over months.

**Child agents are temporary.** Their workspace is removed at task end.

### 4.3 Agent states

A fixed vocabulary emitted by Atrium, never declared by the agent and never in
a manifest.

Base list: `thinking`, `acting`, `browsing`, `running`, `waiting-on-lock`,
`blocked-by-gate`, `held`, `stuck`, `looping`, `idle`, `failed`.

**Four visual groups.** One dot per agent in its group colour; hover reveals the
exact state and task.

| Group | Contains |
|---|---|
| working | thinking, acting, browsing, running |
| waiting | waiting-on-lock, blocked-by-gate, held |
| wrong | stuck, looping, failed |
| idle | idle |

**One state at a time.** Priority: `wrong` > `waiting` > `working` > `idle`.

**Custom states**: a surface author may declare one, but must classify it into
an existing group. They supply the name and a short plain label; Atrium supplies
the colour and shape. No new groups, no custom colours, icons or animation.
Declared states are visible in the overseer so misclassification is catchable.

Rationale: the moment surfaces bring their own visuals, the desktop stops being
learnable, and comprehensibility is the project.

### 4.4 Standing agent rules

These go in every agent's system prompt:

- **Prefer asking for more scope over inventing a workaround.** The failure mode
  is not an agent being blocked; it is an agent quietly doing something ugly
  inside its scope rather than asking.
- Report what was actually done, not what was intended.
- When told the workspace changed underneath you, work out the implications
  first, then correct the plan.

---

## 5. Surfaces

### 5.1 Definition

A **surface** is one workspace inside the environment — a place with its own
persistent state that both agents and the human can act on, plus a pane where
that state is visible.

- A **tool** is a verb (a single action).
- A **surface** is a place (has state over time).
- "Plugin" and "surface" are the same thing. **Use "surface" everywhere**,
  including for installation.

Not "application" — Figma runs elsewhere, the terminal is a real shell. Nothing
runs *inside* Atrium.

### 5.2 The Board

The Board is a **surface in its own right**, not a home screen. It shows widgets,
one per installed surface. Clicking a widget opens that surface.

Naming: Board items are **widgets**. Desktop ornaments (a later, purely cosmetic
feature) are **ornaments**. Do not use "widget" for both.

**Not in v1.** The Board shows what is happening at a glance, and almost
everything it would show — tasks, pending approvals, blocked agents — does not
exist until Phase 10 and later. A board with nothing on it teaches the user to
ignore a panel they will later need to trust. It arrives once it has content.

### 5.3 v1 surface inventory

**Core:**

| Surface | Purpose |
|---|---|
| File explorer | Folders and files. Where movement animations live. |
| Terminal | Real shell, scoped to the environment root. |
| Text editor | Separate window from the explorer. Live-refreshing. |
| Notes / scratchpad | Agents leave findings; the human leaves instructions. |

**Designed, built after v1:**

| Surface | Purpose | Arrives |
|---|---|---|
| Overseer | Dashboard: live logs, pending mappings, permissions, paused tasks, locks, budgets. | Phase 13 — a crude log view exists from Phase 6 |
| Task window | Keybind-summoned chat, options panel, running tasks. | Phase 12 |
| The Board | Widgets for installed surfaces. | After tasks exist — see §5.2 |
| Media viewer | Images and video, human-side rendering only. Agent vision deferred. | Later |

**External (v1):** Excalidraw, as a **generic** surface. See §5.7.

**Deferred:** browser, Figma, all native external surfaces.

### 5.4 Native vs generic surfaces

**Native surfaces** get bespoke UI, hand-built per integration. Expensive —
likely weeks each, forever, per app.

**Generic surfaces** are driven entirely by the effect vocabulary and need zero
custom code.

**Build order consequence: the generic surface comes first.** Building a native
Figma surface first yields one impressive demo and nothing else working. The
generic surface carries the app for its first year.

The generic surface is also **the test of whether the effect vocabulary is
right**. If an arbitrary MCP server renders sensibly with no custom code, the
vocabulary works. If it renders as mush, it does not — and that is a cheap,
early, honest test, runnable before any desktop UI exists.

**The line between them** (so "native" does not drift into meaning "prettier"):

- Native surfaces may render the app's **own object model** — frames, layers,
  shapes.
- Generic surfaces may render **effects only** — something was created, changed,
  deleted.

### 5.5 The manifest

Each surface ships a small manifest describing it to Atrium. There is no single
global manifest; each is bounded by its own surface's complexity and nothing
accumulates.

**Sections:**

- **Identity** — `id` (permanent, never renamed), `name` (display, changeable),
  `version`, `manifest_version`.
- **Connection** — `builtin` | `mcp` | `adapter`, plus launch/reach details.
- **Effect mapping** — which vocabulary id each tool produces, and
  `target_param` (which parameter names the thing acted on, so the animation
  knows what to point at).
- **Load mode** — all tools at once, or search-only. Per-surface, not global.
- **UI** — `native` | `generic`, component reference, icon.
- **Permission requests** — see §8.

**Tools are discovered from the MCP server at runtime, not listed.** Listing
them duplicates data that goes stale. The manifest holds only the effect
mapping for them.

**Locking is NOT in the manifest.** Locks live in the overseer, tied to agent
pipelines. Whether a directory is locked is a property of the workspace, not of
the surface.

**Permissions are requested, not declared.** A surface declaring its own
permissions is a plugin marking its own homework. The manifest requests; Atrium
grants.

### 5.6 Format stability rules

The manifest format will need to change. These four rules make that survivable:

1. **Version the manifest.** Each declares its format version. Atrium reads old
   versions. Nothing breaks when the format changes.
2. **Only add, never remove or repurpose.** New fields are optional with
   defaults. An existing field never changes meaning. This one rule prevents
   almost all breakage.
3. **Keep the manifest small.** Anything uncertain is left out — it can be added
   later, but not taken back.
4. **Own the first three surfaces yourself.** Build explorer, terminal and one
   external surface using the same manifest an outsider would. Inadequacies
   surface immediately rather than after strangers depend on the format.

**Rewrite aggressively early, freeze later.** While it is only Muffin, a
breaking version bump costs an afternoon. Once other people ship surfaces, it
costs them work — that is when discipline starts.

**One loader.** Every pane reads the manifest through one shared piece of code.
A version bump then changes one file, not every pane.

### 5.7 Excalidraw (v1 external surface) — verified constraints

Chosen: `yctimlin/mcp_excalidraw` (`npx -y mcp-excalidraw-server`), over the
official `excalidraw/excalidraw-mcp`. 26 tools, CLI and REST API as a fallback
if the Rust MCP SDK is rough, computes diffs of user edits.

**Verified by hands-on test — two blocking findings:**

1. **Pixels require an open browser tab.** Screenshots, PNG/SVG export, viewport
   control and mermaid conversion all broadcast a render request to a connected
   frontend and reject with `No frontend client connected` otherwise. There is no
   CLI path around it. Headless rendering is only "planned".
2. **Nothing persists across restart.** In-memory only, snapshots included.
   Verified: created an element, killed the server, restarted, canvas empty.

**Working headless:** element CRUD, queries, `.excalidraw` JSON export, file I/O.

**Therefore Excalidraw is a GENERIC surface in v1.** Excalidraw runs in the
user's normal browser; Atrium shows what the agent did to the canvas. This still
tests everything that needed testing — MCP connection, 26 tools against scoped
loading, tool discovery, effect classification. The canvas rendering was never
the point of including it.

---

## 6. Effects — the visual vocabulary

### 6.1 Why it exists

With hundreds of tools, hand-designing a visual per tool is impossible. Instead
every tool declares which **kind of outcome** it produces, and a visual is
designed once per kind.

`write_file`, Figma's `create_frame` and Excalidraw's `add_shape` are three
unrelated tools from three unrelated sources. All three are "a new thing came
into existence". One visual covers all three forever, including tools that do
not exist yet.

**Never add a kind for something that could be a field.** "Browsed" is `fetched`
with a `source`. This rule is what keeps the vocabulary from fragmenting.

### 6.2 v1 vocabulary

| id | Meaning | Visual |
|---|---|---|
| `created` | A thing that did not exist now does | Appears in its pane with a brief highlight |
| `changed` | An existing thing's contents or properties differ | Pulses in place; changed region highlights |
| `moved` | An existing thing is somewhere else | Icon travels old → new. **The signature animation.** |
| `deleted` | A thing that existed no longer does | Fades and vanishes |
| `read` | The agent looked at a specific existing object, changed nothing | Glows briefly |
| `ran` | Something executed and produced output | Output streams into the console area |
| `fetched` | Data came in from outside the environment | Inbound item, marked external |
| `reported` | Information produced about state, no object touched | Transient panel or list, not a persistent object |
| `linked` | One thing now references another | A line drawn between two things |

`read` vs `reported`: `read` has a target you can point at; `reported` does not.
Kept separate deliberately.

`ran` is the odd one — its real consequences are invisible, since a command can
create, change and delete at once. It covers the execution and output; the
filesystem watcher emits the rest.

### 6.3 Fields on every effect

`agent`, `surface`, `target`, `count` / `size`, `source`, `follow_pane`, plus a
human-readable label.

`count` is not decorative: it is a **gate rule input**. "Deny any delete over 50
items" needs a count to check.

### 6.4 Stable ids and the vocab file

**Manifests reference effects by a stable id word that is never renamed.**
A separate vocab file holds everything that *can* change: label, animation,
description, aliases.

    manifest:  effect: "moved"
    vocab:     moved → label "Relocated", animation icon-travel, aliases [shifted]

Change the label freely; no manifest is touched, because no manifest ever
contained the label. The id is readable but never displayed, so it never needs
renaming.

**Aliases** are forwarding addresses for ids you stopped using: `shifted → moved`
means an old manifest saying `effect: "shifted"` still resolves. They exist so
that breaking your own no-rename rule is survivable.

**Rejected:** a custom file format, and having the vocab file write back into
manifests. Write-back means Atrium rewriting files it does not own, leaving
half-migrated state on failure, and silently reverting on reinstall. Indirection
solves the same problem with none of that. JSON or TOML is sufficient — writing
a parser is a bad place to spend risk budget on a project the owner cannot read.

**Unknown effect ids fall back to a generic visual** rather than crashing.

**Vocabulary defined in exactly one place in code**; everything else refers to
it rather than spelling the words out.

### 6.5 New and unmapped tools

Detection is a diff of the tool list on connect. What happens next is the safest
available path:

1. New tool renders **generic immediately** — works, and shows something honest.
2. A model classifies it in the background against the vocabulary.
3. The suggestion sits **pending** in the overseer. **Never auto-applied.**
4. Until approved, the tool keeps rendering generically.
5. Destructive effects — `deleted` especially — always require approval.
6. Classifier confidence is shown. High confidence is one click; low confidence
   gets read properly.

**Rationale:** a wrong mapping shows something confidently false. A delete drawn
as a read is worse than a delete drawn as "something happened", because the user
would stop watching. Generic is vague but never lies.

Nothing waits on the user — the surface is fully usable with mappings pending.
Approval is cleanup, not a gate.

This also solves the cold-start case: an arbitrary MCP server works visually on
day one with zero manual work.

---

## 7. The overseer

Two things that must stay separate in the code:

- **Policy engine** — the rules, the gate, permissions, the tool catalog.
  Deterministic, never shows anything.
- **Dashboard** — a view of what happened. Only reads, never decides.

Conflating them scatters the rules through UI code.

### 7.1 The gate

Every tool call passes through Atrium's own code before execution. The code
allows, modifies, or rejects. **It costs zero tokens — it is not a model.**

Deterministically catchable, no AI:

- path outside the sandbox
- writing to a directory locked by another agent
- a tool the agent is not scoped to right now
- same call, same arguments, N times in a row
- destructive actions above a threshold (delete count, file size)

**On rejection, return an explanation, not a bare failure.** "Denied, that path
is locked by Agent B, read-only available." The agent re-plans. Rejection is
steering, not error.

Per-rule setting: `allow` / `deny-and-explain` / `ask-me`. **Default everything
to `deny-and-explain`**, then flip individual rules to `ask-me` once something
proves annoying in practice.

**The gate does not apply to the human.** The human is the authority the gate
protects on behalf of.

### 7.2 The tool catalog

The overseer holds the full inventory of every tool across every installed
surface, updated per surface install, and decides — per agent, per task — which
small subset to send to the model.

**Important:** tools sitting in manifests cost nothing. Only what reaches the
prompt costs tokens. The manifest can be as complete as it likes.

**Scoped loading** is the primary mechanism, and Atrium's architecture gives it
for free since the world is already partitioned by surface:

- Always loaded: a handful of navigation tools — list surfaces, enter, leave.
- On entering a surface: that surface's tools load, if few enough.
- If a surface has too many: only a **tool search** tool loads, and the agent
  asks for what it needs.

Threshold is a per-surface config value, not a global rule. Terminal has three
tools, always loaded. Figma has eighty, search-only.

**Selection must stay stable within a task.** Providers cache the stable front of
a prompt; reshuffling tools every turn breaks that cache and can raise cost even
while reducing tool count.

Because selection is centralised, usage is measurable — tools never chosen can
stop being sent.

### 7.3 Watchdog

Deterministic, no AI:

- **Stuck** — no tool call in N minutes, no lock held.
- **Looping** — same tool, same arguments, N times.

**Drift** — active, non-looping, quietly doing the wrong thing — is the one case
no rule can catch. That needs an LLM audit, and it is near-free: AgentGUI
measured under 1% of total tokens for up to 34 points of completion improvement.
Run it on idle desks or on command, not continuously.

### 7.4 Failure containment

- **Snapshot before every task** (§3.4). The most valuable one.
- **Heartbeat-expired locks.** A live agent renews periodically; missed renewals
  release the lock automatically. This is the *crash* case, distinct from a
  *paused* task which holds locks deliberately.
- **Rate limits on destructive effects.** Deletions above a threshold per minute
  pause the task and ask. Uses the existing `count` field.
- **Global kill switch.** One key, stops every agent at its next action boundary,
  releases nothing. Deliberately crude — it must work when everything else is
  confused.
- **External effects get a confirmation instead of a snapshot**, since they
  cannot be undone.

---

## 8. Permissions

**Permission list** (keep it short — every permission is something the user must
understand at install time):

- read inside the environment
- write inside the environment
- run commands
- network access
- talk to a specific external service
- read other surfaces' data

**Host access is not requestable in v1.** The option does not exist. *(Flagged:
revisit when host access ships.)*

**`run commands` is a different weight class.** A command can do anything the
shell can. It deserves its own confirmation, not a line in a list.

**No isolation between surfaces in v1.** Real isolation means separate processes
and is significant work. Instead: unofficial surfaces show a warning before
install; officially supported ones eventually get their own store.

**Grants happen once at install**, revocable in the overseer, with a visible list
of what everything currently holds.

**Grant UI is a Windows-UAC-style prompt.** Caution: UAC-style prompts fail when
frequent — people click yes without reading. Keep them to install time and
genuinely dangerous actions, never routine ones.

**v1: manual install with a permission prompt only.** No registry, no
drag-and-drop. Drag-and-drop is a security decision, not a convenience one — a
surface is code with filesystem access running inside an environment agents
operate autonomously. Grants must be enforced before it ships.

*(Flagged for later: "officially supported" implies code review. That is an
ongoing commitment, and a label applied without review is worse than no label —
it converts a warning people would heed into trust they should not have. Decide
what the label promises before anything carries it.)*

---

## 9. Tasks

### 9.1 A task is a tracked object

Not just a message. It holds:

- `id`, the human's original instruction
- assigned agent (or none, if queued)
- state: queued, running, blocked, done, failed, cancelled
- claimed locks
- allowed surfaces
- parent task, if split from one
- the effect history it produced
- **outcome records**: effort used, tokens spent, success, retries, scope
  requests

**The outcome records are a hard dependency of the effort optimizer (§9.6) and
must exist from day one**, even though the optimizer comes much later.

### 9.2 Task creation flow

1. Global keybind → chat window.
2. The user types the request.
3. The orchestrator analyses it. *(Visible delay — show it working, do not
   freeze.)*
4. A **pre-filled options panel** appears: scope, token budget, effort, and
   orchestrator-chosen specifics such as how many agents the task needs.
5. The user approves or adjusts.

This is stronger than approve-the-plan: one glance at a filled-in form tells the
user whether the orchestrator understood the task.

### 9.3 Scope

A **5-point slider**, increasing reach:

| # | Scope |
|---|---|
| 1 | Read-only — look anywhere in the environment, change nothing |
| 2 | Single folder — read anywhere, write only in one declared directory |
| 3 | Folder tree — write across a directory and everything under it |
| 4 | Full environment — write anywhere inside the sandbox |
| 5 | Environment + granted host paths — **disabled in v1** |

**Default: 2.**

- Adjustable mid-task, but **requires a pause**.
- An agent may **request** more scope; the user approves or refuses. This turns a
  dead end into a decision point and reveals when the default is wrong.
- **Children inherit the parent's scope as a CEILING** — never wider, possibly
  narrower. This prevents the orchestrator escalating privileges by delegating.

### 9.4 Orchestration

The orchestrator is an agent whose job is **splitting, not doing**.

- **Runs continuously**, re-evaluating rather than planning once. Trigger:
  child events (finished, failed, blocked) primarily, with a slow interval as a
  backstop. Pure interval wastes calls when nothing changed.
- **Model selection**: Atrium ships a **scoring ruleset**, not a hardcoded model
  list. It queries the provider's own model list at runtime and ranks by context
  size, cost per token, tool-calling reliability, and latency, showing the
  reasoning. A hardcoded table goes stale in weeks. The user picks; Atrium
  recommends.
- **Cost control**: the orchestrator's input is a compact status table, never
  full child transcripts. The input size is the cost driver, not the model.
- **May spawn children mid-run**, bounded by a **token budget** rather than a
  fixed count — a task legitimately needing twenty small children is not
  blocked, and one child looping cannot hide behind a low count. Plus a hard
  depth limit (2–3). **The budget is user-controllable.**
- **Cancellation is a request, not a kill.** The child pauses at its next action
  boundary, runs a defined wind-down (finish current action, report what is
  half-done), and locks release only on user action.
- **Plan approval: approve-first by default**, with a per-task "just run it"
  toggle once trusted.

### 9.5 Budget behaviour

- Exposed as a spend ceiling per task, in whatever unit is meaningful.
- **Hitting the ceiling pauses and asks — never kills.** A cap must not destroy
  work in progress.
- Warning threshold at ~80% so the pause is not a surprise.
- Effort is allocated **within** the budget. The orchestrator never chooses the
  limit, only how to spend it.

### 9.6 Effort and the optimizer

**Effort** is a first-class metric: how much effort to spend versus tokens to
save. Three dials that may move together:

- how hard the model thinks per step (a real API parameter on several providers)
- how thorough the agent is — verify its work, or trust it
- how much the orchestrator supervises

**The effort optimizer** is a manual, user-triggered batch pass over *completed*
tasks. It ranks whether the assigned effort was actually warranted and builds a
catalog of recommendations. Prompted only after N tasks, only in the task
surface.

It produces **defaults the orchestrator consults**, never rules it must follow.

Cautions:

- **Small samples lie.** Ten tasks is not enough. Show the sample size; set a
  meaningful threshold.
- **Each pass is versioned and reversible.** If an optimization makes things
  worse, the user must see what changed and undo it.

Split as everywhere else: **Atrium records and applies; the model reads
outcomes and proposes.**

### 9.7 Interruption

- **Pause = finish the current action, then stop.** No mid-write kills.
- Locks **can** be released without cancelling the task. On resume the agent is
  handed a **factual diff** of what changed while paused. Atrium records the
  diff; the orchestrator explains it. **If they disagree, the recorded diff
  wins** and the explanation is discarded.
- Paused tasks and locks **survive app restart**. Locks are re-verified on
  restore, not blindly restored — a lock on a folder that no longer exists must
  not block anything.
- Paused tasks appear as a **persistent UI element in its own window** —
  hideable, never removable. It shows **who is blocked**, not just a count. A
  count with no consequence gets ignored.
- **Pause cascades down, never up.** Pausing a parent pauses its children;
  pausing a child does not stop the parent, which may route around it. Pausing a
  parent must look visibly different from pausing one child. A parent resuming
  with failed children hands them back to the orchestrator — re-planning is its
  job.

---

## 10. Concurrency

### 10.1 Locks

**Directory-level**, not file-level. Advisory locking with read-write semantics:
many readers, one writer, writer blocks other writers.

**File-level was rejected**, and there is no foolproof scheme. The obstacle is
not technical: "which files does this task touch" is unknowable before the agent
starts. Directory locks work because the agent declares a workspace up front.
Directory locks are also **drawable** — a padlock on a folder icon; file-level
locks scattered across a tree are invisible clutter.

**Deadlock prevention: lock ordering.** Locks are acquired in a fixed order
(alphabetical path), with variable retry attempts, falling back to
fail-immediately-and-re-plan.

**An agent blocked on a lock burns nothing** — no API calls while waiting. But it
needs a visible signal, or the user sees a frozen desk and assumes a crash.

### 10.2 Lock UI

| State | Visual |
|---|---|
| Locked and healthy | Padlock tinted with the owning agent's colour. **No timer.** Hover names agent and task. |
| Locked but silent (heartbeat missed) | Countdown appears **only in the final 20 s** before auto-release. |
| Locked and paused | Distinct mark, no countdown — it never expires. Needs a way to act on it. |

A countdown on a healthy lock is misleading: a working agent renews constantly,
so it would reset every few seconds and read as "about to expire" when nothing
is wrong. The timer only carries information when something is actually wrong.

Expiry window must be meaningfully longer than 20 s — roughly 60 s, giving 40
quiet seconds where a brief hiccup resolves itself. Agents legitimately go silent
during a long model call or slow MCP request; a padlock must not flash every time
something takes 30 seconds.

Exact numbers are tuning, not design. The rule: silent for a while is normal,
silent for too long shows a countdown, countdown expires and the lock releases.

---

## 11. The desktop

### 11.1 Workspaces

**The desk is a view, not a place.** This is what reconciles per-agent
workspaces with the shared-state thesis.

| Per-agent | Shared |
|---|---|
| Which windows are open | The files |
| Their arrangement | Terminal history |
| Focus | Every surface's actual contents |
| Its own notes | The effect stream |

Every agent has its own desktop; they are all looking at the **same**
environment. Agent 2's explorer window and the user's explorer window show the
same folder; move a file and both update. Same model as Linux workspaces — same
filesystem, different arrangements.

- The user has their own desk, where the Board and overseer live once they exist (§5.3).
- Switching works like Linux workspaces, plus a **browser-style tab strip** at
  the top of the app.
- **Parent workspaces persist** across tasks. **Child workspaces are removed** at
  task end.
- **Agent notes are readable by everyone**, not private — it lets agents hand off
  knowledge.
- Agents work primarily on their own desk. **Agents may only come to the user's
  desk in defined, mechanical situations**: blocked and needing a decision,
  requesting scope, or finished with something to show. Never on their own
  judgment of "importance" — an agent deciding what is important enough to
  interrupt will be wrong in both directions, and one of those directions is
  invisible.

*(Open: a child's notes should be handed up to the parent during wind-down, or
deleting its workspace destroys knowledge the parent needs.)*

### 11.2 Launch and layout

- A desktop. **v1 opens to the desktop and the island**, with surfaces opened by the user or by an agent. The Board is not in v1 (§5.2); once it exists it auto-opens on startup.
- **Not a taskbar — a floating island.**
- **The island is for agents, not windows**: agent status dots, current task,
  kill switch, budget remaining. Window switching lives elsewhere (a shortcut or
  overview gesture).

  Reason: an island is small by definition. With six agents and eight windows it
  would either grow into a taskbar or hide things. Agent status is a stable,
  small set; window buttons are not.

- **Window persistence**: the user's desk restores, parent agents' desks
  restore, children's do not. Windows restore their **location** (folder, file),
  not transient state. Where transient state *can* be saved, save it — crashes
  make loss annoying.

  - Editor drafts autosave continuously to a side file (crash-recoverable).
  - Terminal saves scrollback as text; a live process is never restorable.
  - Agent conversation state persists regardless, since paused tasks survive
    restart.

- **Idle state**: the desktop shows what **needs the user** — pending mappings,
  paused tasks, blocked agents — and clears to empty when nothing is
  outstanding. The island separately carries who is here.

  **Rule:** if an item cannot be cleared by acting on it, it belongs in the
  overseer, not the desktop. This is what stops the desktop becoming a second
  dashboard.

### 11.3 Animation

- **Lag is acceptable.** Panes narrate rather than mirror live. This removes
  most of the hard engineering — no catch-up logic, no skipping, no dropped
  frames.
- **One animation queue per surface, not one global queue.** A flood in the
  explorer does not delay the browser window above it. Lag is therefore contained
  to a single surface during a single burst.
- **Surfaces stack like windows with z-order.** Agents open them.
- **Bulk delete**: the explorer opens to the location and icons vanish
  sequentially, speeding up with volume. Not 400 simultaneous animations.
- **Notes animation** (the first one built): replay over the agent's **total
  elapsed time** at an even pace — *not* the model's bursty token timing, which
  replays as stutter-freeze-stutter and looks broken. Floor for short text,
  speed ceiling for long.

### 11.4 Agent presence

- A visible **stroke** on a window while an agent works in it, carrying the
  agent's assigned colour (same palette as lock padlocks) so identity reads
  without hovering.
- Hovering pops a small panel above the window: **agent, task, recency** — the
  same three facts as every other dialog, so the pattern is learned once.
- Each agent gets an **avatar surface window on its own workspace**: an animated
  face reflecting its state, with a thought bubble while thinking, clickable to
  open its reasoning.

  Constraints: expressions map **only** to the four state groups — no finer range
  than Atrium actually knows. The `wrong` group must read as **genuinely
  broken**, not merely sad. A cheerful face during silent failure works directly
  against the governing principle.

  Because faces live on agent desks, **the island dot is the alarm and the face
  is the detail.** An agent going wrong on its own workspace is otherwise
  invisible.

---

## 12. Human–agent handoff

The differentiator, stated as behaviour.

- **Locks do not hard-block the human.** Acting inside a locked directory prompts
  first.
- **The warning names consequences, not risk.** Three recorded facts:

      Agent 2 is working here.
      Task: "clean up the build output"
      It last touched this file 12 seconds ago.

      [Do it anyway]  [Pause it first]  [Cancel]

  The agent's name gives someone to trust or blame; the task in plain language
  shows whether the edit conflicts; the recency shows how live the conflict is —
  12 seconds means back off, 4 minutes means it has probably moved on. No
  prediction, no severity score, no colour-coded risk. Everything in it is
  recorded truth.

- **The agent is told at its next action**, never interrupted mid-action.
- **It is told what changed and who did it.** A human edit is an instruction, not
  corruption. An agent that knows the user renamed something treats it as
  intentional.
- **It works out the implications first, then corrects its plan.** This costs one
  extra model call per affected agent, but only when something actually changed
  underneath — which is exactly when the agent should stop and think.
- **The human's own actions enter the same effect stream**, animated and recorded
  identically, tagged as user actions. The overseer therefore shows one
  interleaved history of everything that happened in the environment, human and
  agent together — the clearest possible answer to "what happened here", and it
  falls out of the design for free.
- **The gate does not apply to the human.** *(Consequence: the human's bulk
  deletions are not snapshot-protected either, unless snapshots also run on a
  schedule. A later "undo my own actions" is worth having; not a v1 concern.)*

### 12.1 The hold

Distinct from a task pause, and it must look distinct.

- Press it. The agent finishes its current action and **stops entirely** — not
  per-window. An agent's actions are sequential; it cannot work elsewhere while
  skipping one surface without re-planning around the interruption, which costs
  a turn and can send it somewhere worse.
- **Auto-releases after ~30 s of user inactivity.** A hold you must remember to
  undo becomes a stuck agent you forgot about — silent, invisible, and it looks
  like a bug. Auto-release fails safe.
- Manual release available for immediate resumption.
- **Does not touch locks, budget, or task state.** Locks protect against other
  agents, and a held agent still owns its work.
- Shown on the island dot ("held by you") and by a changed window stroke.
  Otherwise there is a stopped agent with no visible cause.

### 12.2 Live editing

Two windows on the same file — the user's and the agent's, never shared.

- **Refresh silently when the user's copy is clean; prompt when dirty**
  (keep-mine / take-theirs). Standard editor behaviour.
- The prompt **names which agent and which task**, not a vague "changed on disk".
  Atrium records every effect, so it can.
- Real-time co-editing is not attempted. It is genuinely expensive and out of
  scope.

---

## 13. Modularity

Not a code-quality aspiration — a concrete requirement:

> **A surface must be addable without touching the core.**

A new surface is a folder with a manifest declaring its tools, effect mappings,
and optionally custom UI. Atrium discovers and loads it. **No core file is
edited.**

This is the single decision that determines whether the project survives two
years of additions, and it is what makes outside contributions possible — someone
adds a surface without understanding Atrium's internals.

Cost, stated honestly: plugin architectures are slower to build up front and
force the manifest format to be designed before it is fully understood. §5.6 is
the mitigation.

---

## 14. Open questions

Tracked in `OPEN-QUESTIONS.md`. None block a v1 build.
