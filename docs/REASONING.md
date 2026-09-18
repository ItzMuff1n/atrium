# Atrium — reasoning behind the decisions

**Written:** 11 Sep 2026, from the design conversation that produced this
project. **For:** anyone — model or human — who needs to know WHY something
was decided, not just what.

DESIGN.md records what was settled. DECISIONS.md records rejected stack
alternatives. This file records the reasoning: the arguments that produced
the design, and the ideas that were raised and killed.

**Read this before proposing a change to anything in DESIGN.md.** If your
proposal appears below under "rejected", it has already been argued. Do not
relitigate it without new evidence, and say what the new evidence is.

---

## 1. What the project is actually for

The idea came from a video arguing that AI-human interaction has regressed to
text-only interfaces, the way computing was before graphical interfaces
existed. You talk to an extremely capable system through a scrolling log.

The first framing was "a central application that links agents and platforms
together" — a hub. That framing was examined and set aside, because the
linking problem is largely solved: MCP already handles agent-to-application
plumbing, and a hub that only connects things is a thinner product than it
sounds.

**What survived the examination: the differentiator is shared state, not
connection.** The agent moves a file, the icon moves on screen, you drag it
back, and the agent is told it happened. Both parties act on one workspace.

This is what separates Atrium from the prior art:

- **AgentGUI** (arxiv 2607.26300) — the closest existing thing. A local
  interface for observing and steering long-running agents, unifying Hermes
  and Claude Code trajectories. It is an observation deck. You watch.
- **Marble** (marbleos.com) — same founding thesis as the video, task-as-card
  workspace, Xerox PARC framing.

Both let you watch an agent work. Neither lets you and the agent manipulate
the same objects. **Studied for architecture and user needs, explicitly not
copied.**

---

## 2. The principle that decides everything else

Stated early and repeatedly: **the user must always understand their own
actions and the AI's actions.** Comprehensibility is a core design goal, not
a nicety.

Late in the design it became clear this principle IS the project. It is what
explains, independently, a long list of decisions that otherwise look
unrelated:

- unmapped tools render as a generic visual rather than a guessed one
- classifier suggestions sit pending until approved, never auto-applied
- agent actions become visual effects rather than log lines
- the handoff warning shows three recorded facts, not a risk score
- animations narrate rather than mirror

**The test for any future decision: does this make the user understand more,
or just make the app do more?** If it is the second, it is probably wrong for
this project even when it is a good feature.

---

## 3. Build your own agent loop

**Decided:** Atrium owns a thin agent loop — a driver it controls.

**Rejected:** integrating an existing agent platform (Cursor and similar).

Reasoning: the whole product is the relationship between what an agent does
and what appears on screen. Handing the loop to an external platform means
the effects arrive as whatever that platform chooses to emit, in whatever
shape it chooses. The visual layer would be built on something Atrium does
not control and cannot extend. The loop is thin; the coupling is not.

---

## 4. Closed environment, real shell

**Decided:** a closed environment with its own filesystem. One host directory
is the environment root. Paths are translated — the agent asks for
`/home/documents` and gets `<root>/home/documents`, never learning the real
path exists.

**Decided:** a real shell, confined to the environment root. Not a
reimplemented fake one.

Reasoning on the shell: a fake shell is an endless catalogue of commands that
almost work, and every gap is a place an agent gets stuck in a way nobody
anticipated. A real shell with a hard boundary has one thing to get right
instead of hundreds.

**Decided:** v1 has host access OFF entirely — not a denied request, the
option does not exist.

Reasoning: a permission that exists can be granted by mistake. A capability
that has not been built cannot. Host access needs a permission model and a
snapshot story before it is safe, and neither is designed.

---

## 5. Flutter over everything else

**Decided:** Flutter frontend, Rust backend, local JSON socket between them.

**Rejected — Tauri.** This was the recommendation until research overturned
it. Tauri on Linux uses WebKitGTK, which has documented failures on NVIDIA
plus Wayland — blank windows, resize crashes, DMABUF errors — matching
Muffin's exact hardware. The workarounds disable GPU acceleration, which
defeats the point for an animation-heavy app. The real fix is postponed to
Tauri v3.

**Rejected — pure Rust GUI** (Slint, egui, iced). Poor model training data,
which matters enormously for a project built entirely by AI agents, and no
hot reload.

**Rejected — TypeScript backend.** Loses compile-time safety on the three
subsystems where a mistake is dangerous: the sandbox, the locks, the gate.

**Verified, not assumed.** Flutter was hardware-tested on the actual machine
before the decision was locked: 200 animated squares at 372fps/2.69ms, 1000
squares at ~155fps/6.4ms, against a 16.7ms target for 60fps. Native Wayland,
Impeller on OpenGL ES. Zero rendering errors, no flicker, no blank windows,
no resize crash — precisely where WebKitGTK fails on this hardware.

The general lesson: **the stack decision was reversed by research, then
confirmed by a hands-on test.** Neither step was skippable.

---

## 6. Effects, and why the vocabulary is shaped this way

Nine effect kinds in v1: created, changed, moved, deleted, read, ran, fetched,
reported, linked.

**`read` and `reported` kept separate** — reading a file and telling you
something are different events and should look different.

**`fetched` kept as its own kind** rather than folded into `read` — network
access is a different weight class from local access.

**`ran` stays** because it feeds the overseer's live logs.

**Rejected — a custom file format** for the vocabulary. No reason to invent
one.

**Rejected — letting the vocab file write back into manifests.** The
indirection is the point: manifests reference an effect by a stable id word
that is never renamed; a separate file holds the label, animation, and
description. Renaming a label must never require touching a manifest. Old ids
forward to current ids via aliases.

**Unknown effect falls back to a generic visual** rather than a guess. Same
reasoning as §2 — a wrong mapping lies confidently, a generic one is vague
but honest.

---

## 7. Tool mapping, and the pending queue

**Decided:** tools are discovered from the MCP server at runtime, not listed
in the manifest. The manifest holds only the effect mapping.

Reasoning: a manifest listing tools goes stale the moment the server changes.
Discovery cannot.

**Decided:** when a new tool appears that has no mapping, the overseer detects
it by diffing the tool list on connect. The tool renders generic immediately.
A model classifies it in the background. The suggestion sits **pending** until
approved or corrected. Never auto-applied. Destructive effects — especially
`deleted` — always require approval. Classifier confidence is shown.

Reasoning: an auto-applied wrong mapping means an agent deletes something and
the screen shows a file being written. The generic fallback is uninformative;
the wrong mapping is actively misleading. Uninformative is recoverable.

---

## 8. Permissions: manifest requests, app grants

**Decided:** the manifest REQUESTS permissions; the app GRANTS them, via a
permission prompt to the human modelled on Windows UAC.

**Decided:** locking does NOT belong in the manifest. Locks live in the
overseer, tied to agent pipelines.

Reasoning: a surface declaring its own locks is a surface that can declare
itself unlockable.

**Decided:** tool selection is the overseer's job. It holds a catalogue across
all installed surfaces and picks the per-turn subset.

**Decided:** that selection must stay STABLE within a task rather than
reshuffling per turn — reshuffling breaks provider prompt caching, which is a
direct and ongoing cost.

**Flagged, unresolved:** "run commands" is a different weight class from every
other permission. A command can do anything the shell can. It deserves its own
confirmation rather than a line in a list.

**Flagged for later:** "officially supported" in a surface store implies code
review, which is an ongoing commitment. A label applied without review is
worse than no label — it converts a warning people would heed into trust they
should not have. Decide what it promises before anything carries it.

---

## 9. Concurrency

**Decided:** directory-level locking, unless a foolproof file-level scheme
exists. Lock ordering as the primary deadlock fix, with variable retry, then
fail-immediately-and-replan as fallback.

**Decided:** an agent with no other task idles until the lock releases, rather
than working around it.

Reasoning on the locking level: file-level locking sounds better and is a
larger surface for subtle bugs. AGENT-RULES puts it plainly — concurrency
bugs are invisible until they are catastrophic, and they do not reproduce
reliably. Prefer the boring implementation.

**Decided:** locks do not hard-block the human. Acting inside a locked
directory prompts first, with a warning naming the actual consequence rather
than a generic "are you sure".

**Decided:** the agent is told at its NEXT action, never interrupted
mid-action. It is told WHAT changed and WHO did it — a human edit reads as
intentional, not as corruption. It works out the implications first, then
corrects its plan.

**Decided:** the human's own actions enter the same effect stream, animated
and recorded identically, tagged as a user action. The gate does not apply to
them.

---

## 10. The three-fact warning

The handoff dialog shows: the agent's name in bold, the task in plain
language, how recently it touched this file. Then [Do it anyway] [Pause it
first] [Cancel].

**Rejected — risk scoring or predicting what will go wrong.** Three recorded
facts, no prediction.

Reasoning: Atrium records every effect, so it can state facts. It cannot
predict consequences, and a predicted risk that is wrong teaches the user to
ignore the dialog. The same three facts appear in the window hover panel — one
pattern learned once.

---

## 11. Workspaces: the desk is a view, not a place

The tension: each agent should have its own desktop, but they all work in one
environment.

**Resolution:** per-agent means which windows are open, their arrangement,
focus, and its own notes. Shared means the files, the terminal history, every
surface's actual contents, and the effect stream. The same model as Linux
workspaces.

**Decided:** agent notes are readable by everyone, not private — "it would be
fun", and it lets agents hand off knowledge.

**Decided:** parent/orchestrator workspaces PERSIST across tasks. They get
called again, tidy up over time, leave notes. Child workspaces are temporary
and removed at task end.

**Implication accepted:** parent agents are therefore long-lived identities,
not per-task instances. They need names that survive restarts, persistent
notes, and eventually an answer for accumulated cruft.

**Open, unresolved:** child workspaces are deleted at task end, which destroys
their notes. Likely fix is handing a child's notes up to the parent during
wind-down. Not designed.

**Decided:** an agent may only come to the user's desk in mechanical, defined
situations — blocked and needs a decision, requesting scope, finished with
something to show. Never on its own judgment of what is "important".

Reasoning: "important" is a judgment an agent will make badly and constantly.

---

## 12. The hold, and why it is not a pause

**Decided:** a button that stops the WHOLE agent after its current action.
Auto-releases after ~30s of user inactivity. Manual release available. Does
not touch locks, budget, or task state.

Reasoning on stopping the whole agent rather than one window: an agent's
actions are sequential. It cannot work elsewhere while skipping one surface
without re-planning around the interruption, which costs a turn and can send
it somewhere worse.

Reasoning on auto-release: a hold you must remember to undo becomes a stuck
agent you forgot about — silent, invisible, and indistinguishable from a bug.
Auto-release fails safe.

---

## 13. Scope, effort, and the optimizer

**Decided:** a 5-point scope slider, deliberately shaped like a video-game
difficulty setting. 1 read-only, 2 single folder, 3 folder tree, 4 full
environment, 5 environment plus granted host paths (disabled in v1). Default 2.

Reasoning: the user sets scope, not the orchestrator — but a real permission
model is unusable by someone who does not want to think about permissions.
Radical simplification was the requirement, not a compromise.

**Decided:** children inherit parent scope as a CEILING. Narrower is allowed,
wider never. This prevents an orchestrator escalating its own privileges by
spawning a child.

**Decided:** agents must PRIORITISE asking for more scope over inventing
sketchy workarounds. This is in the standing agent rules.

**Decided:** cancellation is a REQUEST, not a kill. The child pauses at its
next action boundary, winds down in a defined way, reports what is half-done.
Locks release only on user action.

**Decided:** the orchestrator can spawn children mid-run, capped by a token
budget per task — organic rather than a fixed count — plus a hard depth limit.
The budget must be user-controllable.

**Decided:** effort is a first-class metric, bounded by the user-set token
budget. The orchestrator allocates within a limit; it never chooses the limit.

**Decided:** the effort optimizer is a manual, user-triggered batch pass over
COMPLETED tasks. It produces recommendations the orchestrator consults, not
rules it must follow. Prompted only after N tasks.

**Dependency flagged:** task objects must record outcomes from day one —
effort used, tokens spent, success, retries, scope requests — or the optimizer
has nothing to analyse. This is why the task model is an object rather than a
message.

**Cautions attached:** small samples mislead, so show sample size and set a
threshold. Every optimization pass must be versioned and reversible.

**Correction accepted during design:** the original plan was a hardcoded table
recommending models. That was replaced with a scoring RULESET that queries
provider model lists at runtime and ranks by context size, cost, tool-calling
reliability and latency. A hardcoded table is stale the week after it ships.

---

## 14. Modularity is a requirement, not an aspiration

> A surface must be addable without touching the core.

**Cost stated honestly at decision time:** plugin architectures are slower to
build up front, and they force the manifest format to be designed before it is
fully understood.

**Accepted anyway**, because this is the single decision that determines
whether the project survives two years of additions, and it is what makes
outside contributions possible.

---

## 15. Things deliberately deferred, and why

Not open questions. Decided to postpone.

- **Agent vision** — the media viewer is human-side rendering only.
- **The browser surface** — when built, it runs as an EXTERNAL process
  (Playwright-style, screenshots in, coordinates out), never embedded. Muffin
  dislikes web rendering as laggy; an embedded browser also reintroduces the
  exact WebKitGTK problem that killed Tauri.
- **Native external surfaces** (Figma and similar).
- **Process isolation between surfaces** — v1 instead warns before installing
  an unofficial surface.
- **Drag-and-drop surface install** — flagged at design time as a security
  decision, not a convenience one. Permission grants must exist before it
  ships. v1 is manual install with prompts.
- **Real-time co-editing of a file** — genuinely expensive, out of scope.
- **Docker-per-workspace isolation** (AgentGUI's approach) — worth revisiting.

---

## 16. Testing decisions that were reversed

Recorded because the reversals were correct and the pattern is worth keeping.

**Excalidraw was going to be a native surface.** A hands-on test found the MCP
canvas is browser-only: pixels — screenshots, PNG/SVG export, mermaid,
viewport — require an open browser tab, because the server broadcasts render
requests to a connected frontend and rejects with "No frontend client
connected" otherwise. Headless rendering is only "planned". Element CRUD,
queries and .excalidraw JSON export work headless. Nothing persists across
restart. **It became a GENERIC surface in v1.**

**The Tauri reversal** (§5) — research overturned the recommendation.

The common thread: **both were caught by testing or research, not by thinking harder.** Neither would have been caught by review alone. Worth noting the contrast — the Phase 1b split (HANDOFF §4) WAS reached by reasoning forward, before any code existed. Both routes work; the mistake is assuming review alone catches the first kind.

---

## 17. A trap worth carrying forward

Fourteen MCP reference servers — GitHub, Postgres, SQLite, Slack, Google
Drive and others — were archived in May 2025 with no security guarantees.
Listicles and tutorials still recommend them. Seven remain active: Everything,
Fetch, Filesystem, Git, Memory, Sequential Thinking, Time.

**Always take the vendor-maintained version.** This generalises beyond MCP:
the popular answer and the current answer diverge, and the gap is where the
security problems live.
