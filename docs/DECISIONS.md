# Atrium — Decisions and Rejected Alternatives

Every entry here was considered and settled. **Read this before proposing a
change to anything in `DESIGN.md`.** If a proposal matches a rejection below,
it has already been argued — do not relitigate it without new evidence, and say
what the new evidence is.

Format: **Decision** — what was chosen. **Rejected** — what was not, and why.

---

## Stack

### Frontend: Flutter

**Rejected — Tauri (Rust + webview).** This was the recommendation until
research overturned it. On Linux, Tauri renders through WebKitGTK, and on NVIDIA
GPUs WebKitGTK and the driver disagree — blank windows, flicker on resize, the
app dying on resize with no useful error, DMABUF framebuffer errors, Wayland
protocol error 71. The documented workarounds all disable hardware acceleration
or compositing, which makes the animation problem worse rather than better. A
July 2026 user report: 40fps on WebKitGTK with a 3070, versus 240fps after
converting to Chromium. The most active tracking issue has decisions postponed to
at least Tauri v3. **The target machine is RTX 3080 + Wayland + KDE — precisely
the documented failure profile.**

**Rejected — Slint.** The only production-rated Rust-native GUI, and genuinely
capable. But it uses its own markup language with far less model training data,
every UI change is a recompile with no hot reload or devtools, and there is no
component ecosystem. For a project the owner cannot read, the language models
write well matters more than raw render efficiency.

**Rejected — egui.** Most model training data of the Rust GUI options and fast,
but immediate-mode: it redraws everything every frame and looks like a debug
tool. Making it look like a desktop means fighting the framework.

**Rejected — iced.** Still 0.x, verbose, widely rated not production-ready.

**Rejected — Qt/QML.** Massive training data, GPU-rendered, strong on KDE. Ruled
out on licensing constraints for commercial use and a maze of binding options.

**Rejected — embedding a browser engine (CEF/Chromium) in a native window.**
Would ship a browser engine inside an app built natively *to avoid* shipping a
browser engine. Same bloat, more work, worse tooling.

**Rejected — writing a browser engine.** One of the largest software artifacts
that exists. Nobody writes one.

### Backend: Rust

**Rejected — TypeScript everywhere.** Faster to write, biggest MCP/AI ecosystem,
one language across the stack, probably half the time to a working v1. Rejected
because the failure modes are asymmetric: a path-traversal bug in TypeScript is
found in testing, while in Rust it often will not compile. The project is fully
vibe-coded, so **the compiler is the only reviewer this codebase gets**. Slow
progress is annoying; an agent-written path check that silently accepts `../..`
is the user's home directory.

Counterweight acknowledged: models write TypeScript better than Rust, and the
borrow checker will eat sessions the owner cannot help debug. Accepted knowingly.

**Fallback if Rust proves genuinely unworkable:** keep Rust for sandbox, locks
and gate only; move the agent loop and MCP clients to TypeScript. Uglier, but it
preserves safety exactly where it matters.

### Bridge: local socket with JSON

**Rejected — FFI.** Faster, but it fails in ways neither the owner nor Claude
can debug, and the traffic here is tiny. Not worth the risk budget.

### MCP: Rust SDK

The Rust SDK reached **Tier 1 on 2026-08-21** — 67/67 server and 50/50 client
conformance, including sampling and elicitation. Tier 1 now spans TypeScript,
Python, C#, Go and Rust, so language choice is free of SDK availability
concerns. **Caveat: it is new. Expect rough edges the TypeScript SDK does not
have.** Phase 0b exists to find out early.

---

## Architecture

### Atrium owns the agent loop

**Rejected — integrating existing agent platforms** (Cursor, Claude Code, etc.).
They own their own UI and control flow; redirecting their tool calls into
Atrium's surfaces means fighting their design. "Works with any agent via API
key" and "integrate agent platforms" are opposite directions — the first means
owning the loop with models as interchangeable parts, the second means being a
plugin inside someone else's product. Cannot be both.

### Real shell, scoped

**Rejected — a fake shell** (reimplemented commands). Two stated goals were
incompatible: "1:1 copy of a terminal with all its commands" and "controls the
host machine". Reimplementing coreutils, git, python and npm is years of work
that would never be 1:1, and a fake shell that can act on the host is a boundary
with a door in it — paying the cost of a sandbox and getting none of the safety.

A real shell with a controlled, deliberately widened scope achieves the same
long-term goal and can grow into host access without a rewrite.

**Cost accepted:** loss of automatic visibility. A filesystem watcher on the
environment root covers it — standard, cheap, solved by existing libraries.

### MCP as the connection layer

**Rejected — building the plumbing.** MCP already solved *how an agent talks to
an app*. Nobody solved *how a human watches and shares that work*. Consuming an
existing standard and adding the visual half is a far stronger position than
building both.

**Cost accepted:** Atrium's reach equals the MCP ecosystem's reach plus
hand-built adapters. Cursor is a client, not a server — it would not plug in as
a surface at all.

### Plugin architecture from the start

**Rejected — hardcoding surfaces and refactoring later.** The refactor never
happens, and the risk of one mistake reaching the whole codebase is exactly what
a vibe-coded project cannot absorb.

---

## Effects and manifests

### Effect types, not per-tool visuals

**Rejected — a visual per tool.** Caps the project at maybe twenty integrations
forever. Mapping tools to a small fixed set of *effect kinds* means eight visual
behaviours cover unlimited tools.

**Rejected — adding a kind for anything that could be a field.** "Browsed" is
`fetched` with a `source`. Adding it invites "browsed vs scraped vs api-called",
fragmenting the vocabulary by *source* instead of by *outcome*. This is a general
rule, not a one-off.

**Rejected — `failed` as an effect kind.** Failure is an agent *state*; nothing
happened to any object. Same for `thinking`, `searching`, `stuck`, `looping`.
The realisation that these are a second category is why states and effects are
separate vocabularies. Effects are declared in manifests and render into panes;
states are emitted by Atrium and render on the agent indicator. States are
entirely Atrium's, which makes them much safer to change later.

**Rejected — merging `read` and `reported`.** Argued for (both are "looked,
changed nothing") but kept separate by decision.

**Rejected — dropping `fetched` in favour of `created` + source field.** Argued
for, kept by decision — outside data deserves a distinct visual.

### Stable ids with an indirection file

**Rejected — a custom text file format with injection into manifests.** Inventing
a format means writing a parser, and an agent-written parser the owner cannot
read is a bad place to spend risk budget. JSON or TOML already do referencing.

**Rejected — the vocab file writing back into manifests.** Atrium would be
rewriting files it does not own; a half-completed migration leaves some manifests
on the old word and some on the new; and reinstalling a surface silently reverts
it. If manifests point at the vocab file, they never need editing at all.

**Rejected — numeric ids** (`effect: 7`). Safe but unreadable to whoever writes a
manifest. A stable *word* treated as an id and never renamed is readable and
equally safe, because the id is never displayed.

**Rejected — treating vocabulary change as catastrophic.** Overstated earlier and
corrected: with ~5–10 self-owned surfaces, a rename is an afternoon and an agent
can do it across a codebase in seconds. Adding a kind breaks nothing; only
removing or renaming does. Lock-in is a year away, not today. **Rewrite
aggressively early, freeze once outsiders ship surfaces.**

### Auto-classification of new tools never auto-applies

**Rejected — heuristic name-matching applied automatically** (`create_*` →
created). Cheap and usually right, but occasionally wrong in a way that
misleads.

**Rejected — model classification applied automatically.** A wrong mapping shows
something confidently false. A delete drawn as a read is worse than a delete
drawn as "something happened", because the user stops watching. **Generic is
vague but never lies.**

**Rejected — blocking the surface until mappings are approved.** Nothing should
wait on the user; approval is cleanup, not a gate.

---

## Concurrency

### Directory-level locks

**Rejected — file-level locks.** No foolproof scheme exists, and the obstacle is
not technical: "which files does this task touch" is unknowable before the agent
starts. Directory locks work because the agent declares a workspace up front, and
they are **drawable** — a padlock on a folder icon. File-level locks scattered
across a tree are invisible clutter. The visual layer decides the granularity,
not the other way around.

### Lock ordering for deadlock

**Rejected — timeouts as the primary fix.** Simple, but a lock can expire
mid-write.

**Rejected — fail-immediately as the primary fix.** Crudest and most robust, but
wasteful as a first resort. **Kept as the final fallback** after lock ordering
and retries.

### Countdown only in the final 20 seconds

**Rejected — a visible timer on every lock.** A working agent renews constantly,
so the timer resets every few seconds and never means anything — it reads as
"about to expire" when nothing is wrong. A timer only carries information when
something is actually wrong.

---

## Interface

### The island is for agents, not windows

**Rejected — an island holding open windows.** An island is small by definition;
with six agents and eight windows it either grows into a taskbar or hides things.
Agent status is a stable small set.

**Rejected — a conventional taskbar.** Chosen against on aesthetics.

### Per-agent workspaces sharing one environment

**Rejected — one shared desktop for all agents.** The literal shared-workspace
reading, and it becomes cluttered immediately with several agents.

**Rejected — fully separate per-agent environments.** That is the
observation-deck model everyone else already built, and it abandons the
differentiator. **The resolution: the desk is a view, not a place** — windows and
arrangement are per-agent, the environment underneath is one.

### Human and agent get separate windows on a surface

**Rejected — sharing one window.** Chosen against; real-time co-editing is
genuinely expensive and out of scope.

### The desktop shows only what needs the user

**Rejected — a permanently populated desktop dashboard.** It would duplicate the
overseer. **Rule adopted:** if an item cannot be cleared by acting on it, it
belongs in the overseer, not the desktop.

### Animation lag is accepted

**Rejected — realtime mirroring with catch-up logic.** Effects arrive faster than
animation can show them (`rm -rf` deletes 400 files in milliseconds). Accepting
lag removes catch-up logic, skip-to-end thresholds and dropped frames entirely.
Per-surface queues contain the lag to one surface during one burst.

**Rejected — replaying the model's actual token timing.** Text arrives in bursts
with pauses; replaying it literally looks broken. Replay over total elapsed time
at an even pace instead.

---

## Tasks

### Tasks are tracked objects

**Rejected — a task as just a message to an agent.** Simple, but then nothing can
hold a lock, nothing can be split between agents, nothing survives a restart, and
the overseer has nothing to show progress against.

### The user sets scope, the orchestrator does not

**Rejected — the orchestrator assigning scope to children freely.** That means an
LLM setting the safety boundaries. **Ceiling inheritance** is the compromise:
children may be narrower, never wider — no privilege escalation by delegation.

**Rejected — exact inheritance with no orchestrator discretion.** Simpler, but
more agents than necessary end up with broad access.

### Continuous orchestration

**Rejected — plan-once orchestration.** Cannot adapt when a child fails.

**Cost control:** the orchestrator's input is a compact status table, never full
child transcripts. Input size is the cost driver, not the model.

### Token budget instead of a child count cap

**Rejected — a fixed maximum number of children.** A task legitimately needing
twenty small children would be blocked, while one child burning tokens in a loop
hides behind a low count. A budget is the number that is actually cared about.
A hard depth limit is kept alongside it.

### Cancellation is a request

**Rejected — killing a child process.** Leaves half-written files and orphaned
locks.

### The gate is not a model

**Corrected misconception:** the gate costs **zero tokens**. It is Atrium's own
code inspecting a tool call. The only token cost is a ~20-token rejection
message, and only when something is blocked. The genuine recurring cost is tool
definitions in every message — which is what scoped loading addresses.

### Effort optimizer is manual and batch

**Rejected — the orchestrator choosing its own effort in the moment.** It has an
incentive problem: nothing stops it picking high effort every time, since it does
not pay the bill. Effort is allocated *within* a user-set budget.

**Rejected — continuous background optimization.** It is a batch job over
completed tasks, not something belonging in the per-task path.

---

## Permissions

### No isolation between surfaces in v1

**Rejected — process isolation per surface.** Significant work. Mitigated by
warnings on unofficial surfaces and, eventually, a reviewed store.

### Host access is not requestable in v1

Not a denied request — **the option does not exist**. *(Flagged: revisit when
host access ships.)*

### Manual install only in v1

**Rejected — drag-and-drop install in v1.** It is a security decision, not a
convenience one: a surface is code with filesystem access running inside an
environment agents operate autonomously. Permission grants must be enforced
before it ships.

---

## Naming

**Chosen: Atrium.** An open interior space you can see across, with rooms off it
— structurally what the desktop is, and it means both things the project is
about: a real space to work in, and a space built so you can see across it.

**Rejected — "The Nexus" (the working codename).** The most-used word in AI
tooling, taken many times over, and it describes *connection* — the part already
solved by MCP. Atrium's contribution is the seeing, not the connecting. Retained
as the codename and the folder name only.

**Rejected — Overlook** (also means "to miss something"), **Wetware** (taken and
misleading), **Studio** (very common as a suffix), **Terrarium** (fits precisely
— a closed environment observed from outside — but risks reading as a toy).

---

## Prior art

Studied for architecture and user needs. **Explicitly not to be copied.**

### AgentGUI (arXiv 2607.26300)

Open-source local GUI for observing and steering long-running agents. FastAPI
server locally, each agent turn in an isolated worker process, events streamed to
a React frontend over WebSockets, one persistent Docker sandbox per desk.
Presented as a pixel-art office — each agent a worker at a desk, tasks started by
picking an empty desk and typing.

Four views per desk: **activity** (reasoning/generation visually separated from
tool requests/responses), **overview** (wall-clock timeline), **console**
(turn-level debug with token telemetry plus a code-only execution stream), and
**files** (one-click previews of the work directory).

**Findings taken seriously:**

- **38% faster, 93% vs 80% accuracy** against the Hermes Dashboard — *same
  information in both*, only the organisation differed. Also lower mental demand,
  frustration and effort. Organisation alone is worth that much.
- The two question types where the interface helped most were **"where did the
  time go"** and **"what files came out"** — not reasoning. People want the
  outcome surface, not the transcript.
- Heavy coding-agent users asked for a **code-only console**: filtering *out* the
  reasoning was the win.
- An **LLM manager auditing mid-run** lifted completion by up to 34 points at
  under 1% of total tokens. Supervision is nearly free.

**Where it stops, and Atrium's opening:** it is an observation deck. The human
reads and interjects but never acts on the workspace. There is no pattern for the
human moving a file and the agent noticing. That gap is exactly the shared-state
idea.

**Difference in isolation approach:** AgentGUI mounts Docker per desk; Atrium
uses an environment root with no Docker. Theirs is stronger isolation, Atrium's
is simpler. Worth revisiting later, not now.

### Marble (marbleos.com)

Explicitly framed on Xerox PARC and the 1984 Macintosh, arguing AI is still at
the command-line stage. Each delegated task becomes a card; jobs run side by
side; files and tools visible at once. Same founding thesis. Still
cards-about-work rather than the work itself.

---

## MCP ecosystem traps

**Fourteen reference servers were archived in May 2025** — including GitHub,
GitLab, Slack, PostgreSQL, SQLite, Google Drive, Puppeteer, Redis, Sentry, Brave
Search and Google Maps. Listicles still recommend them. **The archived versions
carry no security guarantees.** Always take the vendor-maintained version.

**Seven remain actively maintained** by the steering group: Everything, Fetch,
Filesystem, Git, Memory, Sequential Thinking, Time. The repository itself warns
they are educational examples, not production-ready solutions.

**GitHub MCP:** vendor-maintained and official, streamable HTTP or local stdio.
Default toolsets are broad — use read-only mode.

### Excalidraw MCP — verified findings

Two candidates existed. Chose **`yctimlin/mcp_excalidraw`** over the official
`excalidraw/excalidraw-mcp`: persistent live canvas rather than one-shot
generation, 26 tools (a real test of scoped loading and tool search), a REST API
fallback if the Rust MCP SDK is rough, and it computes diffs of user edits.

**Hands-on test found two blockers** (both in `DESIGN.md` §5.7): pixels require
an open browser tab, and nothing persists across restart. Element CRUD, queries
and `.excalidraw` JSON export work headless.

**Consequence: Excalidraw is a generic surface in v1, not native.** This
validates the decision to build the generic surface first — the first real
external integration could not have been native regardless.

---

## Process

### Every phase ends with a human-performed verification

**Rejected — trusting agent completion reports.** Established from prior
experience: an agent's self-reports about its own state are not evidence. In a
project the owner cannot read, every "done, it works" is unverifiable without a
hands-on check.

**Corollary:** if a phase cannot be verified hands-on, the phase is not defined
properly and must be redefined before it is built.

**Amended 25 Sep 2026 — for SCRIPTED gates only.** Muffin retired the hand-run for
deterministic scripts: his re-run of a script the agent had already run adds nothing,
and sign-off for a scripted phase is his approval of the evidence instead. His
hands-on pass returns for what only he can judge — visible behaviour, from Phase 6 (the
first UI) onward. **See "Hand-tests run in CI; a scripted phase is signed off on the
evidence" below for the reasoning and for what this does and does not weaken.** The
corollary above is unaffected: the phases after 2d include the first UI, which is
judged by eye and cannot be scripted at all.

### Tests the agent writes are not proof

They are useful, but a test written by the same process that wrote the bug shares
the bug's assumptions. Tests **defined by the user** and watched to fail first
are worth much more.

**Amended 25 Sep 2026.** The blind attack lists (`TOPICS.md` §4, `docs/blind-attack-list-*.md`)
are the mechanism that addresses this, and they are unaffected by the hand-test rule
change below. What the change does do is put more weight on them: the hand-run was a
second **execution**, never a second **design**, so a misunderstanding shared by the
code and its harness passed both. That was true before the change and remains true
after it — the blind list is the only thing that tests the shared assumption, not the
signature.

### Path normalisation: accept redundant spellings, do not reject them

**Decided 11 Sep 2026, during Phase 1 verification — and it contradicts
`attack-list.md` section E, which must be corrected.**

Five paths appear in attack-list.md section E as *reject* cases:
`//`, `///home///documents`, `/home/documents//`, `/home/./documents/.`,
`/home/documents/..`. But section I lists `/home/./documents` and
`/home/documents/` as *accept* cases — and after component normalisation those
are the same paths. E and I cannot both be right.

**Resolved in favour of I.** Once separators collapse and `.`/`..` resolve, a
redundant spelling is indistinguishable from the ordinary spelling, and the
result is still inside the root. Rejecting on spelling rather than on resolved
location is the "check, then resolve" failure this subsystem exists to avoid —
and it would break agents that emit `/home/./documents`, which is an ordinary
thing for a program to produce.

**The rule:** classification depends on the resolved location, never on how the
path was spelled. Every one of the five E-lines was run by hand against the
resolver and confirmed to land *inside* the root.

**Action required:** attack-list.md section E must drop those five lines (or
section I must drop its two) so the two sections agree.

**Done 12 Sep 2026.** The five lines were moved into section I, where the
normalisation rule above says they belong, and are now ordinary ACCEPT cases
rather than "informational" ones. The demo harness and the test suite were
updated to match. Section I was chosen as the correct side because this
decision already ruled that classification depends on resolved location, not
spelling.

**Separately resolved, same pass:** the bare `/` (environment root) was listed
in section G as a reject-case while section I required it to ACCEPT. Same class
of contradiction, different sections. Section I is correct — resolving the root
is not an escape — and the G line was removed.

**Also noted, same pass:** a trailing slash on a *file* (`/afile.txt/`) is
accepted and resolves to the file, where POSIX says it should fail with
ENOTDIR. Contained, so not a security issue; recorded because it is a real
deviation, not smoothed over.

---

## Post-sweep decisions (12 Sep 2026)

Two collisions found by the consistency sweep could not be fixed by making one
document agree with another, because they were design decisions. Both decided by
Muffin, 12 Sep 2026. Recorded here because `DESIGN.md` and `BUILD-PLAN.md` must
be read alongside this file before either is changed.

### Path resolution: `resolve()` stays single-root permanently

**Decision.** `DESIGN.md` §3.2's "allowed roots" (plural) was loose wording. The
resolver takes **exactly one root, and always will.** Every path is checked
against the environment root.

**Rejected: widening `resolve()` to accept a set of roots.** Rejected because
`resolve()` is the one function where a mistake is dangerous. A root-set
signature would mean re-verifying the function *and its entire attack list* every
time the mount logic changed. Keeping it single-root means the mount layer can be
wrong without the sandbox being wrong — a failure is contained to a layer that
still cannot escape the root it hands to the resolver.

**Mounts are still real in the design**, but host access is off entirely in v1, so
v1 has one root and no mount table. When mounts arrive they get their own layer
*above* the resolver: that layer maps a virtual path to a `(root, subpath)` pair
and then calls `resolve()` unchanged.

**Consequence.** Phase 2 onward builds on the current signature, as built and
verified. `DESIGN.md` §3.2 now reads "the environment root" with a dated
clarification beneath it.

### Strong confinement for `run commands` is its own phase, and it gates the agent loop

**Decision (14 Sep 2026).** A command run through Atrium's shell must be unable to
reach anything outside the environment root. This becomes its own numbered build
phase, **`2e`**, sitting after 2d, and it **must land before Phase 5** (the agent
loop).

**Requirement (mechanism-agnostic).** Inside the command's own view of the
filesystem, outside the environment root **does not exist** — not "is checked
first". An attack list for 2e will be written before its code, as for Phases 1,
1b and 2a. The mechanism — bubblewrap, user namespaces, Landlock, a container —
is 2e's implementation decision, deliberately not fixed here; all were checked
available on this machine 14 Sep 2026.

**Why this was not already the case.** `DESIGN.md` §3.3 has always said the shell
"runs actual commands **on the host**, with its working directory confined to the
environment root". That is true, and it is not confinement: the directory a
program starts in does not limit what it does. `BUILD-PLAN.md` 2b said the same
in one line. The gap was in the documents, not in any built code — Phase 2a
(file operations) is built and is the one piece of Phase 2 that exists, but **no
shell code exists at all**, and 2b was stopped before any was written.

**The evidence that forced the decision.**

1. **Observed in 2a (14 Sep 2026):** a **hard link** — a second name for one file
   — placed inside the root pointing at a file outside it let the sandbox read
   that file's contents *and overwrite it*. No path check can catch this: a hard
   link has no target to inspect, and both names are the same file. Recorded in
   `attack-list-2a.md` §N and as open item L.4.
2. **`BUILD-PLAN.md`'s own Phase 2 verification block required the step.** It has
   always read: *"Try `cd /` and then a destructive command. Confirm it cannot
   reach the host."* Strong confinement was therefore already implied by the
   plan's own gate; 2b cannot satisfy it. Deferring the cage would have meant
   deleting a verification step, so the step was **moved** to 2e rather than
   dropped.

**Rejected: folding the cage into 2b.** Identical reasoning to the 1b split — two
hard things in one phase make a found hole unattributable. If the command runner
and the cage shipped together and something escaped, there would be no way to say
which half was wrong. `DECISIONS.md` — "Phase 1b is its own phase, before Phase
2" — is the precedent, and it is applied rather than re-derived.

**Rejected: accepting this gap in v1** (documenting the promise as narrow and
deferring the cage to v2). It contradicts the project's own gate, and it would
mean shipping a shell the user is told not to trust while the design elsewhere
calls the environment closed. The cost of the cage is a phase; the cost of not
having it is the project's central claim.

**Rejected: a fake shell** (reimplemented commands) — already rejected at
`DECISIONS.md` "Real shell, scoped", and untouched by this decision. The question
was never *whether* the shell is real; it is *what the real shell is allowed to
reach.* That rejection stands.

**Also owed by 2e (carried from 2a).** The environment root must be **its own
mount point** — its own filesystem or a tmpfs — because cross-filesystem hard
links are impossible (observed: `Invalid cross-device link`; `/tmp` device 50,
`/home` device 49). That is what closes the hard-link channel rather than merely
documenting it.

**Consequence.** The Phase 2 verification block covers 2a–2d and no longer
contains the host step; the host step is 2e's verification, and it is Muffin's to
watch like every other gate. Until 2e passes, **`run commands` must not be
exercised by anything autonomous.**

### Phase 2e: bubblewrap, an explicit mount list, and a root that is its own mount point

**Decided 26 Sep 2026. Approved by Muffin in chat the same day.** Four decisions
and two added requirements, each with its one-line reason. This entry is the
record; `BUILD-PLAN.md` §2e carries the requirement and the plan issue #87
carries the measurements.

**1. The cage is bubblewrap, invoked as a system binary, with an explicitly built
mount list.** *Reason: proven working unprivileged on this machine (0.12.0) and on
the GitHub runner behind one sysctl (run 36244899076), binding the root at `/`
makes virtual paths line up exactly as `DESIGN.md` §3.1 defines them, and every
directory the command can see is a line in our own auditable argument list.*

**2. `--clearenv`, then an explicit allowlist: `PATH`, `HOME`, `TERM`, `LANG`.
Nothing else.** *Reason: measured — a bare bwrap cage leaks the host's whole
environment into the command, including `SSH_AUTH_SOCK`, `DBUS_SESSION_BUS_ADDRESS`,
`GH_AUDIT_TOKEN` and every `HERMES_*` variable, so without this the cage hides less
than it leaks.* Enforced by its own test: fake `SSH_AUTH_SOCK`, `GH_AUDIT_TOKEN` and
`HERMES_TEST` set on the host must not appear in `env` inside the cage, and the
output must contain only allowlisted names.

**3. System folders a real shell needs (`/usr`, `/bin`, `/sbin`, `/lib`, `/lib64`)
are bound read-only; `/home` and every host path not explicitly bound must not
exist inside the cage.** *Reason: a cage the sandbox cannot be used in is as broken
as one that leaks, and "not bound" must mean "not there" rather than "there but
unreadable".* Enforced by its own tests: `ls /home` inside the cage fails or is
empty; writing to `/usr` inside the cage fails; and the other-direction check still
passes — a file created inside the cage appears at the real environment root.

**4. The environment root is its own mount point, provided by Atrium's own
unprivileged user+mount namespace.** *Reason: an unprivileged user cannot `mount`
(measured: `must be superuser to use mount`), but can inside a user namespace, where
a tmpfs at the root path makes a hard link from outside it impossible — measured
`Invalid cross-device link`, which is 2a's open item L.4 closed by construction
rather than by a path check.* `run()` refuses a root that is not its own mount
point, read from `/proc/self/mountinfo` (std-only; no dependency).

**Rejected: `--tmpfs /` as the root.** *Reason: measured — a write inside a private
tmpfs never appears at the real root, so 2e's own verification line "a file created
inside appears on the host" cannot pass.*

**Rejected: opening the cage's network to let agents look things up.** *Reason:
Muffin's requirement is that agents can still search the web; the capability must
come from a fetch/search tool the loop calls directly, outside bubblewrap, recorded
as a `fetched` effect — not from a hole in the cage. Added to `BUILD-PLAN.md`
Phase 5 as its own item; that is now the only route to the network before Phase 9.*

**Rejected: hand-rolled `unshare` + bind mounts.** *Reason: equivalent isolation,
more code, and a failure mode already hit while probing this phase — bwrap applies
mounts in order, so binding the root after the system directories silently replaces
the whole tree.*

**Rejected: Landlock.** *Reason: compiled and active here, so available, but it
restricts the caller's own access by path rules, and §2e's requirement is explicit
that a path check is not what is wanted.* **Rejected: a container.** *Reason: a
daemon, an image and a network stack is a far larger surface than the requirement
needs, and it still needs the same sysctl.* **Rejected: chroot.** *Reason: needs
root, and does not stop a process already holding a directory descriptor.*

**Fail closed.** If the cage cannot be established — binary absent, namespaces
refused, root not its own mount point — the command is **refused with a reason**.
There is no path that runs a command uncaged.

**Dependency, named for `AGENT-RULES.md` §5.** 2e adds a **system** dependency:
the `bubblewrap` package, and a one-line `sudo sysctl -w
kernel.apparmor_restrict_unprivileged_userns=0` in the CI hand-tests job. It is not
a cargo dependency, so `deny.toml` and the supply-chain gate are unaffected.
Approved explicitly by Muffin, 26 Sep 2026.

### Snapshots are a plain copy, not copy-on-write, and the design's own fallback is why

**Decision (14 Sep 2026, Phase 2c).** Snapshotting the environment root copies it.
**Copy-on-write is deliberately not implemented.** This is a recorded deviation
from the *preferred* half of `DESIGN.md` §3.4's sentence, which reads:

> The environment root is snapshotted **before every task**. Copy-on-write if the
> filesystem supports it, otherwise a plain copy.

**The design sanctioned this branch, so this is not an overturned decision.** The
sentence is conditional and names its own fallback — "otherwise a plain copy".
Taking the fallback is following the design, not departing from it.

**Why the fallback is the right branch now, in order of weight.**

1. **The root's filesystem is 2e's decision and is not made.**
   `DESIGN.md` §3.1 requires the environment root to be **its own mount point**
   ("its own filesystem or a tmpfs"). A tmpfs **cannot** reflink at all —
   observed on this machine: `cp --reflink=always` on `/tmp` fails with
   `Operation not supported`, while the same command inside the btrfs `/home`
   succeeds. So building COW now means building against a mechanism that has not
   been chosen and that may be chosen *specifically to break COW*. The honest
   order is: choose the filesystem (2e), then optimise for it.
2. **`std` cannot express it, and the alternative is a dependency.**
   Reflinking is the `FICLONE` ioctl. `std` has no ioctl wrapper. Getting it
   needs `libc`/`nix` (a registry dependency — a stop-and-ask under
   `AGENT-RULES.md` §5) or shelling out to `cp --reflink` (a runtime dependency
   on coreutils, and a second failure path in the one mechanism whose job is to
   be there when things have gone wrong). Every crate in this project is
   `std`-only plus path references to its own crates; this is not worth breaking
   that for.
3. **COW changes cost, not behaviour.** `DESIGN.md` §3.4 says what it is for:
   *"This makes nearly every filesystem failure an undo rather than a loss. It is
   the cheapest and most valuable safety mechanism."* The **value** is the undo.
   The **cheapest** is about cost. Cost is not a requirement of this phase, and
   the phase's attack list treats COW as absent on purpose
   (`attack-list-2c.md` §K.1).

**What is owed, so this is a measured decision rather than a shrug.** The claim
"plain copy costs too much" is an assumption until measured. The measurement is:
snapshot time and store bytes versus environment size, taken across a few
realistic sizes. If a copy of a realistic environment is slow enough to be felt
before every task, that is the evidence that justifies reopening this with a
deliberate, recorded dependency — and it should be taken before the agent loop
(Phase 5) runs snapshots automatically, not after.

**Why it is safe to add later without invalidating this phase's verification.**
The snapshot's completed record (`attack-list-2c.md` §F) verifies the *contents*
of a snapshot against a manifest written when it was taken, whatever produced
them. A reflink copy and a byte copy produce the same tree; the verification does
not care which happened. Adding COW later therefore adds an optimisation to a
mechanism that is already verified, rather than changing what was verified.

**Also decided here, and required rather than optional: the snapshot store lives
outside the environment root.** A snapshot that a shell command inside the
environment can delete is not an undo. `--store` inside `--root` is a **refusal**
(`attack-list-2c.md` §B.5). This closes open item **§N.23** from
`attack-list-2b.md` — *"a command can write anywhere in the environment root,
including wherever the app itself keeps state"* — at least for this one case: the
app's undo state is the first durable thing the design puts inside a sandbox's
blast radius, and it is kept out of it.

**Also decided: this tool is not agent-facing, and is therefore not routed
through the resolver.** It takes real host paths (`--root`, `--store`). The
resolver's job is turning *virtual* paths into real ones, and there is no virtual
path in a snapshot operation — the app's own machinery calls this, as it calls the
gate. `AGENT-RULES.md` §6's rule ("the resolver is the only way to turn a virtual
path into a real one") is not weakened, because nothing here is a virtual path.
**Consequence recorded so it is not read as a regression:** 2a and 2b both
required refusals to avoid naming the sandbox's real location on disk; this tool
does the opposite, because the person reading its output *is* the user, who needs
to be told which directory was refused. A later session must not "fix" that
inconsistency — the two are different by design.

### A path dependency on our own verified crate is not the dependency the stop-and-ask rule guards

**Decision (14 Sep 2026).** When a new Atrium crate needs the resolver, it declares
a **path** dependency on `atrium-resolver`:

```toml
atrium-resolver = { path = "../resolver" }
```

This does **not** require a stop-and-ask under `AGENT-RULES.md` §5 ("you are about
to add a dependency"). It must still be stated explicitly in the child's report.

**Why it is not the thing the rule guards.** §5 exists because a dependency
imports code and a supply chain nobody in this project reviews — the user does not
read code, so a third-party crate is unverifiable by the only gate this project
has. A path reference to a crate sitting in this repository adds nothing new to
review, downloads nothing, and pins no version. It is a pointer to code that is
already here and, in `crates/resolver/`'s case, already verified hands-on by Muffin.

**The first use was a brief precedent; this makes it a decision.** Phase 2a's
brief §14.2 said the reading should be recorded in `DECISIONS.md` "not left as a
precedent in a brief". Phase 2b is the second use, so it is now recorded — with
the reasoning, so a later session does not have to re-derive it and cannot
mistake a third-party crate for the same case.

**The boundary, stated so it cannot be stretched.** This covers **path
dependencies on crates in this repository, and nothing else.** Any dependency
fetched from a registry or git remote is still a stop-and-ask. `crates/shell/` is
`std`-only plus this one path reference; if a future phase believes it needs a
real crate, that is a stop-and-ask with the reasoning written down.

**Also decided with it:** a new crate must **not** depend on `atrium-fileops`.
`crates/fileops/` is signed off hands-on by Muffin (14 Sep 2026) for one job, and a
second crate depending on its binary or its library would give a verified artifact
a second failure mode. Code sharing that matters should be lifted into a crate
with its own verification — not borrowed sideways from a signed-off one.

### Phase 1b is its own phase, before Phase 2

**Decision.** Non-existent-path resolution becomes a numbered phase, `PHASE 1b`,
sitting between Phase 1 and Phase 2 in `BUILD-PLAN.md`.

**Rejected: folding it into Phase 2a.** The entire point of splitting it out of
Phase 1 was isolated verification. If non-existent-path handling shipped
alongside Phase 2a's file-operation plumbing and its attack list found a hole,
there would be no way to tell which half was wrong. Folding it back recreates
exactly the ambiguity the split was made to avoid — so the split is preserved
rather than tidied away.

**Also decided:** Phase 1b requires **its own attack list covering non-existent
paths, written before the code**, in the same shape as Phase 1's two lists, and
watched by Muffin line by line. Phase 2 does not start until Phase 1b passes.
`delegation-briefs/phase-1-path-resolution.md` §8.4a remains the origin of the
requirement, and its text is quoted verbatim in the new plan section.

### Project documents are not copied into skills

**Decision.** The two Hermes skills stop being copies and become pointers. Each
SKILL.md keeps its frontmatter; its body is one instruction — read the source
document at its absolute path and treat that file as the authority.

- `atrium` → `/home/muffin/VibeCodeProjects/atrium/docs/AGENT-RULES.md`
- `muffin-style` → `/home/muffin/VibeCodeProjects/atrium/docs/WORKING-WITH-MUFFIN.md`

**Rejected: keeping the copies and syncing them.** A copy that nothing syncs
drifts, and it drifts silently — a subagent follows a stale rule set with no
warning. A pointer cannot drift. Subagents inherit skill *names* only, so the
body matters just when something calls `skill_view`, at which point reading the
real document is strictly better than reading a frozen copy.

**Consequence.** The old `diff`-the-copy checks in `HANDOFF.md` §6 are dead and
must not be re-added. If a skill body ever grows beyond a pointer, that is the
drift returning — fix the skill, not the document.

### The environment namespace follows the Linux tree

**Decision (Muffin, 13 Sep 2026).** Atrium's environment root uses the standard
Linux directory names and structure — `/home`, `/etc`, `/tmp`, `/var`, `/usr`,
`/opt`, `/srv`, `/mnt`, `/root`. **Naming and structure are what matter**, so an
agent or a human landing in the environment finds the layout they already know.

**Scope limit, in his words: only necessary or created files. Anything that
does not make sense in the long run is kept out.** The tree is not populated to
look complete. A directory exists because something needs it or something made
it.

**Excluded: `/proc`, `/sys`, `/dev`.** On real Linux the kernel generates these
live. Atrium has no kernel, so reproducing them means empty directories that
carry the name of something real and hold nothing. The terminal surface runs a
real shell, so commands reading them would fail in ways that read as breakage.
Leaving them out makes them resolve as absent — honest — rather than as empty
lies. This is the comprehensibility principle applied to the filesystem: a
directory that looks real and is not is worse than one that is not there.
Revisit only if a later surface genuinely needs them.

**Rejected: reserving host-looking names so they can never exist.** It buys a
less alarming hand-test and costs a special case in the resolver plus a list to
maintain. The resolver already contains everything by construction — a leading
`/` is the environment root — so a name is only ever a name. Nothing needs
forbidding.

**Consequence for Phase 1b.** `/etc/passwd`, `/root/newfile` and similar must
come back ACCEPT once non-existent paths resolve. By hand they read as
catastrophic escapes and are not. Recorded in `attack-list.md` §B — read it
before driving the 1b pass.

### Phase 1b — three resolver rulings

**Decided by the Hermes agent, 13 Sep 2026, and copied here as the single
home.** They were made in `delegation-briefs/phase-1b-path-resolution.md` and
`attack-list-1b.md`; this section is the authority, those two are the working
copies. Implementation calls, not Muffin's.

**A fourth ruling was added, after the build.** The heading is left reading
"three" because other files cite this section by that exact name — including
`attack-list.md`, which is Phase 1's record and must not be edited — so renaming
it would break those references for no gain. Read to the end of the section:
there are four.

**1. A dangling symlink inside the root ACCEPTs.** A link whose target does not
exist yet is exactly the case Phase 1b is for — Phase 2 has to be able to create
through it. Containment is unaffected: what matters is where the target *would*
land, not whether it is there now.

**2. A file followed by `..` REJECTs, with the operating system's own reason.**
It normalises to somewhere inside the root textually, but on a real filesystem
it is ENOTDIR. The OS reason is reported rather than a synthesised one.

**3. A backslash is a filename character, never a separator.** Atrium's
namespace is the Linux tree (see the section above), so `\` carries no special
meaning at any position.

**Do not "fix" the trailing-slash deviation while working in this code path.**
`/afile.txt/` ACCEPTs today where POSIX says ENOTDIR. It is a recorded Phase 1
deviation, cosmetic, inside the root, and it sits in the same walk as ruling 2 —
so anyone extending that walk will be tempted to tidy it. Leave it.

**Same-spelling trap, recorded before it costs someone an afternoon.** A
symlink target of `/etc/absent.txt` on the host and the virtual path
`/etc/absent.txt` inside the sandbox are the same string on opposite sides of
the boundary. Nothing in the spelling tells them apart.

**Consequence: some Phase 1 REJECT lines invert under 1b.** They rejected as
*absent*, and 1b accepts absent paths — `/home⁄documents` (U+2044) is one.
These are verdict changes, not regressions. `attack-list.md` §F's blanket
"every line must be rejected" reads wrong after 1b lands; see the note at the
top of that file. The 300-character component is the one genuine open call: it
rejects today as ENAMETOOLONG, and whether 1b still rejects it depends on
whether length is judged textually or against the filesystem. Either is
defensible — it must be chosen deliberately and stated, not left to fall out of
the implementation.

**Resolved (Muffin, 13 Sep 2026): follow Linux.** Phase 1b rejects any single
component over **255 bytes**, checking the length itself rather than waiting for
the filesystem to raise ENAMETOOLONG. This keeps the verdict identical whether
the path exists or not, which is the property that matters — a name that can
never be created should not resolve cleanly just because nobody has tried yet.

**Bytes, not characters.** Linux's `NAME_MAX` is 255 bytes. A 200-character
name in Hebrew, Arabic or emoji exceeds it; a 255-character ASCII name does not.
Measure the UTF-8 encoded length. Getting this wrong produces a resolver that
agrees with the filesystem on English names and disagrees on everything else —
the worst kind of near-miss, because it passes every test written in English.

**The limit is per component, not per path.** A long path of short names is
fine. `PATH_MAX` (4096) is a libc buffer convention rather than a filesystem
rule and is deliberately not enforced here.

**4. A dangling symlink followed by `..` resolves against the target's location,
and the step rule applies to it.** *(Added 13 Sep 2026, after the Phase 1b build.
The implementing child hit this case, found no rule covering it, and handled it
by its own reading of ruling 1. It read the principle correctly; this entry makes
that a rule rather than an inference, so the next person extending the walk does
not have to re-derive it.)*

`/link-to-nothing/..` — the link exists, its target `/home/documents/absent.txt`
does not. Canonicalisation of the link's target fails as not-found, so the walk
enters textual mode. The `..` is then applied to **where the target would sit**,
giving `/home/documents` — inside the root, so **ACCEPT**.

Its counterpart **REJECTs**: `/link-to-nothing-out/..`, whose target
`/etc/absent.txt` is outside the root, left the root at the symlink step, and the
step rule does not care that the `..` walks back toward something legal-looking.

**Why this is ruling 1 and not ruling 2.** Ruling 2 hands the verdict to the
operating system *because the prefix exists as a non-directory* — the filesystem
has an opinion and it is authoritative. Here the target does not exist, so the
filesystem has no opinion to give; there is no ENOTDIR to report. Absence is
exactly the condition ruling 1 says must not become a rejection, so the path is
normalised textually and judged on containment alone. The two rulings do not
conflict: existence decides which one applies.

**Consequence.** A dangling link's target location behaves like any other absent
path — traversable as text, contained at every step. Verified contained by probe,
13 Sep 2026.

**5. A symlink chain that leaves the root at ANY hop is refused, even if it ends
inside the root.** *(Ruling by Muffin, 18 Sep 2026. The reason is his, verbatim:
"the resolver's own rule says containment is checked at every intermediate step,
and an out-and-back chain breaks that. Strictness wins.")*

The shape: `<root>/back -> <outside>/relay -> <root>`. The first hop leaves the
environment; the second lands back inside it. The old resolver followed the whole
chain with `canonicalize`, which reports only where it lands, so the walk checked a
final location that is inside the root and **ACCEPTed**. The departure was never a
step, so nothing checked it.

Under this ruling it is **REJECTed**. Containment is checked at every step, and the
chain passes through a position that is outside the environment on its way back in.

**What this does and does not change.** A chain entirely inside the root is still
ACCEPTed. A single link whose target is outside is still refused, for the reason it
always was (ruling K.1a). What changes is only the out-and-back shape: it was the
one case where the walk left the root and nothing noticed.

**Recorded as ruling K.1a — a dangling link's target counts, whether or not the
target exists.** A link whose target is outside and *absent* is refused on the same
ground: the departure is judged where the target lands, and lands outside.

**The ancestor clarification (same ruling, 18 Sep 2026).** Spelling one target
string may pass through the root's ancestors — `/`, `/tmp`, the directory the root
sits in — because that is how an absolute address is written. A target that points
inside the root is spelled `<root>/home/documents`, and the host paths above the
root are part of the spelling, not a departure.

But **where a hop ENDS must be inside the root or be the root.** A link whose target
resolves to an ancestor — `<root>/a -> /home`, or the root's own parent — has left
the root, and the path is **REJECTed even if later components lead back in**. The
two are different questions: passing through an ancestor while spelling an
inside-pointing target is ordinary; *landing* on one is leaving.

**Test home.** `crates/resolver/tests/regression_chain_hops.rs` — the out-and-back
shapes (`out_and_back_chain_is_refused`, `hop_out_in_the_middle_of_a_longer_chain_
is_refused`), the ancestor shapes (`a_hop_that_lands_on_an_ancestor_of_the_root_is_
refused`, `an_absolute_target_spelled_through_ancestors_and_landing_inside_is_
accepted`), and the controls that must not change. One existing assertion was
flipped under this ruling: `blind_textual.rs` case 22, which was adjudicated ACCEPT
when written (its comment there records the reasoning and the flip).

---

## Phase 2d — the watcher

### The watcher reaches inotify through its own `extern "C"` declarations, and that is not a dependency

**Decision (15 Sep 2026).** `crates/watcher/` declares the three inotify entry points it
needs — `inotify_init1`, `inotify_add_watch`, `inotify_rm_watch` — and `read` as
`extern "C"` symbols in its own source, and calls them directly. `Cargo.toml` has an
empty `[dependencies]` and stays that way.

**This does not require a stop-and-ask under `AGENT-RULES.md` §5** ("you are about
to add a dependency"), and the reasoning is the same as the recorded path-dependency
ruling: **nothing is fetched.** Rust already links libc on
`x86_64-unknown-linux-gnu`, so these symbols resolve at link time against code the
build was already carrying. No registry crate, no git remote, no version pin, no
supply chain. §5 exists because a dependency imports code nobody in this project
reviews; a declaration of symbols the binary already links imports nothing.

**The alternatives, and why each was rejected.**

1. **The `libc` crate** — the conventional way to call these. It is a registry
   dependency, so it is a genuine stop-and-ask, and it buys three function
   signatures that fit on one screen.
2. **`syscall(SYS_inotify_init1, …)` directly** — `std` exposes no raw syscall, so
   this needs `libc` too (the `syscall` function itself). Same objection, worse
   ergonomics.
3. **Polling** — walk the root on a timer and compare. Needs no FFI at all and works
   on any filesystem, and it was the closest call. **Rejected because it cannot do
   this phase's job:** it cannot see a file created and deleted between two passes,
   and it cannot say *which* change happened — only "something differs". This
   phase's entire value is knowing what a command **did**; `DESIGN.md` §6.2 assigns
   exactly that to the watcher and distinguishes it from the `ran` effect, which
   covers the command and its output. Polling is recorded in `crates/watcher/README.md` as
   the known fallback that was not taken.

**The boundary, stated so it cannot be stretched.** This covers **symbols the
platform's own libc already provides, declared in our source, for a Linux-only
crate.** Any crate fetched from a registry or git remote is still a stop-and-ask.
`crates/watcher/` is `std`-only plus these four declarations.

### The watcher's reported path is virtual, and the root's real path never appears in the change stream — but does appear in a refusal

**Decision (15 Sep 2026).** Two rules, and they look contradictory on purpose:

- **The change stream names virtual paths only** — `/home/documents/x.txt`, never
  `<root>/home/documents/x.txt`. `DESIGN.md` §3.1: the agent never learns the real
  path exists. `--show-real` prints real paths and exists for the person reading the
  tool, the same exception `crates/shell/` makes. This is enforced rather than intended: the
  harness checks **every** line of a run for the root's real path, as bytes, and the
  crate's own `resize`/reconstruction path refuses to build a record from a path
  that is not under the canonical root.
- **A refusal names the real path**, because the reader is the user, who must know
  *which* directory was refused.

**Why this is not a contradiction, and the sentence a later session will want to
"fix".** The two are different audiences with different needs, and this is the
`crates/snapshot/` precedent arriving in a second crate. `DECISIONS.md` already records it
for `crates/snapshot/`: a tool driven by the **user** may name the path it refused; the
prohibition is on output that can **reach an agent**. The watcher's refusals are its
startup errors, read from a terminal; its change stream is the thing Phase 3 will
turn into agent-visible effects. **Do not unify them.** The harness's `!! REAL PATH
LEAKED` check is scoped to the change stream for exactly this reason, and
`attack-list-2d.md` §H.2 is the line that says a refusal is allowed to name it.

**Observed, and this is why the rule needed enforcing rather than stating:** the
first version of the CLI printed `# watching /tmp/atrium-2d-root (5 directories)` —
the root's real path, in the header of the default output. The harness caught it on
its first run. Fixed by printing `/` as the root in the default output.

### Overflow is reported as a gap, and the watcher does not guess

**Decision (15 Sep 2026).** When the kernel's event queue fills, it **drops events**
and sends one record with `wd = -1`. The watcher turns that into an `OVERFLOW`
record, **fails the run** (exit 1), and says how many events had been drained before
the gap — and nothing more.

**Rejected: a resync that re-walks the tree and reports "what must have changed".**
It is the obvious repair and it invents events. A resync cannot tell "changed while
we were blind" from "was already like that", so every entry it reports is a guess
wearing the same clothes as an observation. It would also introduce four mechanisms
the blind list raised and this phase deliberately has none of — mtime-based
comparison, replace-shown-as-create, debounce net-state, inode-reuse identity — each
of which is only reachable *because* a resync exists. `DESIGN.md` §6.5's principle is
the one that applies: **generic is vague but never lies.**

**What is owed, so this is a measured posture rather than a shrug.** The gap is
reported; a real resync is a separate piece of work with its own attack list, and
`attack-list-2d.md` §L.2 carries it. The overflow's shape is measured, not assumed:
300,000 creations with nothing drained produced 16,385 records — 8,192 `CREATE`s,
then the overflow record, then nothing.

### The watcher re-derives its paths from the tree, because a watch follows an inode

**Decision (15 Sep 2026).** Every `wd` (kernel watch descriptor) is mapped **by
inode**, and after any move the affected watches have their paths **re-derived from
the tree** rather than trusted across the move. A watch whose inode is nowhere inside
the root is **removed and reported**, not left in place.

**Why the obvious design is wrong, measured.** The conventional approach is a
`wd → path` table, updated from `MOVED_FROM`/`MOVED_TO` cookies. It fails in the case
that matters: **a watched directory moved *out of the root* keeps its watch and its
children's watches, and keeps reporting.** Observed: after moving `root/victim` to
`outside/victim`, writes inside `outside/victim/inner/` arrived on the old wds as
`wd=3(inner) CREATE name="after.txt"`. A watcher that trusted its table would report
`CREATE /home/…/inner/after.txt` — **an invented change, inside the environment, for
a file that lives entirely outside it.** That is the worst class of defect this phase
can have: not a missed event, a fabricated one, in the sandbox's own voice.

**Rejected: re-keying purely from cookies.** It handles an in-root rename and is
blind to a move out of the root, which is precisely the observed case — and the two
are indistinguishable from the event stream alone. Re-deriving from the tree answers
both, and answers them from ground truth rather than from bookkeeping.

**Consequence, and the honest limit of it.** A watch whose directory has left the
root, or been deleted, is reported as `GONE … no longer inside the environment
(deleted, or moved out)` — the watcher does **not** claim to know which, because the
kernel does not say. `MOVE_SELF` on its own is treated as "this path is no longer
known", never as "this is gone", because it arrives for an in-root rename too.

---

## Hand-test harness: the binary is the workspace-root artefact

**Decision (25 Sep 2026, topic #69 T6).** Every `hand-test-2*.sh` resolves its
binary as `$REPO_ROOT/target/debug/<bin>` — the workspace root, computed from the
script's own location — and builds with `cargo build -p <crate>` from `$REPO_ROOT`.
The scripts also print the binary's path, mtime and sha256 before running, so a
stale artefact is visible in the output rather than silent.

**Rejected: `$HERE/target/debug/<bin>`, the crate-relative path.** This is what
`2a`, `2b`, `2c` and `2d` used, and it is what `hand-test-1b.sh` was already fixed
away from. `cargo build` in a cargo workspace writes to the **workspace root's**
`target/debug` whatever directory it runs from, so the crate-relative path is a
location the build never writes.

**Why it stayed hidden, measured.** The path worked only because a stale binary
from an earlier session happened to sit there. Observed 25 Sep 2026:
`crates/watcher/target/debug/atrium-watcher` was dated `2026-09-18 19:12:12`,
while `crates/watcher/src/lib.rs` was `19:34:51` — the script was testing a binary
22 minutes older than the code it claimed to test. Worse than testing nothing: it
reported a verdict, and the verdict was about superseded code.

**The mechanism, demonstrated rather than argued.** The crate-relative staleness
check was itself correct — with a stale binary present, `find src tests -newer
"$BIN"` correctly says "stale, rebuild". The rebuild then ran and wrote to the
workspace root, leaving the crate-relative file untouched and still executable. So
the failure was not "fails to notice staleness"; it was "notices, rebuilds to the
wrong place, and runs the stale file anyway". A detector that only checked the
staleness verdict would have proved nothing — the first version of this topic's own
detector made exactly that mistake.

**Consequence.** The stale `crates/*/target/` directories were deleted. That is the
load-bearing part: with them gone the old scripts fail their `[ -x ]` check
outright, so the defect cannot silently return. The path fix turns a silent wrong
result into either a correct one or a loud failure.

**And the check the topic asked for is a REFUSAL, not a report.** T6 required "a
check that fails if the binary is older than any file in the crate's `src/`". An
earlier revision of this change printed the binary's path, mtime and sha256 and
called that the protection — which says a stale artefact is *visible*, not that one
is *rejected*, and left a stale binary still executable. Each script now asserts
freshness after building and **exits 2** if the artefact is still older than a
source, with the failing path named. Both are kept: the printed identity is how a
human reads what was tested, the assertion is what stops a stale one being used.

**Two defects in this change found by review, recorded because they were mine:**

1. `if ! ( cd … && cargo build … 2>&1 | tail -3 )` tests the status of `tail`, the
   last command in the pipeline — which succeeds even when `cargo` fails (no
   `pipefail` in these scripts). The `BUILD FAILED` branch was unreachable, so a
   failed build fell through to a stale binary: precisely the failure this work
   exists to prevent. Output is now captured and `$?` read directly from the build.
   Confirmed by a failing test before the fix (both in isolation and with a real
   failing `cargo build`), and by a stub `cargo` that exits non-zero.
2. The requirement above was met only in its weaker form. Fixed as described.

Both were demonstrated failing first: with `cargo` shadowed by a stub that reports
success while building nothing, the pre-fix script proceeds and exercises a binary
dated 2000-01-01 (exit 0), while the fixed script refuses (exit 2). A fix without a
shown failure path is a claim, not a repair.

## Hand-tests run in CI; a scripted phase is signed off on the evidence

**Decided 25 Sep 2026 by Muffin.** This supersedes the hand-run half of "Every phase
ends with a human-performed verification" above, for scripted gates only.

**Decision.** Every `crates/*/hand-test-*.sh` runs automatically on every PR and every
push, in `.github/workflows/hand-tests.yml`. The job fails if any script exits non-zero.
**Sign-off for a scripted phase is Muffin's approval of the evidence — the output quoted
in `STATUS.md` plus that CI run — not his re-run of the script.**

**Why — his reason, which is the whole argument.** A re-run of a deterministic script
by the person who did not write it adds nothing: same script, same binary, same output.
It was never a second *test*. Calling it one would be dressing ceremony up as
verification.

**What this does not concede, and it matters.** The hand-run was **never a second
designer**. The agent writes both the code and the harness that tests it, so a
misunderstanding is encoded in both and passes both — a re-run by Muffin reproduced the
agent's own assumptions exactly. The only thing the hand-run added was a second
**execution** on a different machine. The blind attack lists (`TOPICS.md` §4) are the
mechanism that tests the shared assumption, and they are untouched by this change. So
the sign-off was **already** an approval of evidence in everything but name; this change
says so plainly. It is not a reduction in what is tested.

**Rejected — dropping the scripted gate entirely.** The scripted hand-tests remain the
gate; what changed is *who runs them*. CI runs them, on every PR, where a hand-run
happened once per phase and not at all in between. **The rule change makes the scripted
gate run more often, not less.**

**Rejected — Muffin's hands-on testing going away altogether.** It returns where it is
the only instrument: **visible behaviour, from Phase 6 (the first UI) onward.** A
terminal transcript can be read; a window rendering wrong cannot. Nothing about a
scripted transcript covers "does it look right", "does it feel wrong", "does the
animation stutter" — so those phases keep the hand-run, and the corollary above ("if a
phase cannot be verified hands-on, the phase is not defined properly") keeps its force
for them.

**What it costs, stated rather than glossed.** A green CI run is an artefact of this
repo's own scripts, so a wrong script is green in CI too. The guard (`scripts/guard.sh`)
is the counterweight and is unaffected: it separately refuses a PR that **removes or
weakens** a case, which is a different question from whether the remaining cases pass.
Both are needed. Neither substitutes for the other.

**When it takes effect, stated exactly.** **Phase 2d is the last hand-run.** Muffin ran
that gate himself on 25 Sep 2026 and it passed, so its `STATUS.md` entry under "Verified
hands-on" is a literal recount of a run he performed — nothing about it changes. The new
rule applies to the **next** scripted phase onward: from there, an entry in that section
records his **approval of the evidence** rather than a run he performed, and must say so
in as many words, so a later session cannot read an approval as a re-run.

## The `approvals.deny` relative-path gap (accepted as known, 26 Sep 2026)

The 17 deny patterns in `~/.hermes/config.yaml` match the path only when the command
string spells it out — `~/vibecodeprojects/atrium` or `/home/*`. A command that `cd`s
first and then names a relative path (`rm -rf crates/watcher/target`) is **not** denied;
it asks for approval instead. Measured, not inferred, with `hermes approvals test`:
that spelling returns ask-approval (exit 2), the same command with `~` or
`/home/muffin/...` returns user-deny (exit 3).

**Muffin's decision, 26 Sep 2026: accepted as known, not fixed.** The ask-approval it
falls back to is still a gate — nothing ran without approval — so the gap narrows a
convenience rather than opening a hole. Recorded here so a later session does not
rediscover it and treat it as new, and does not "fix" the patterns by adding relative
spellings, which would not help: a relative path is only meaningful next to the `cd`
that precedes it, which a pattern cannot see.

Reported in full in issue #76 §2.

## The Pipeline v2 budget was raised from 60M to 300M mid-run (Muffin, 25 Sep 2026)

**Decision, recorded here because it exists nowhere in `docs/` otherwise.** Topic #69
(Pipeline v2) was planned and approved on 24 Sep 2026 with a **60M** token budget and a
**25M early stop**. Both were superseded during the run: on 25 Sep 2026 Muffin raised the
budget to **300M**. The figure the topic therefore ended against is **107.6M of 300M**, in
the closing issue #76 §7.

**The evidence, and its one honest limit.** The raise reaches the record as the
`constraints:` line of the goal-continuation instruction — *"Budget raised to 300M by
Muffin."* — which exists **only as a user message in the session store** for
`20260924_212614_70af61` (25 Sep 2026 19:10:23 IDT, first occurrence). The originating chat
message was not in this session and was not recoverable, so the exact time of the decision
is not pinned; what is established is that it was in force by 19:10 on 25 Sep. Stated
rather than smoothed over.

**Ordering, because it changes what the raise means.** It was **not** a response to the
missed checkpoint firing:

| when (IDT) | what |
|---|---|
| 24 Sep 22:11 | T3 merged, `ee5cd16` |
| 25 Sep ~19:0x | the 25M checkpoint was found crossed, and the miss self-reported |
| 25 Sep 19:10 | the raise reached the session |
| 25 Sep 21:48 | *"Spend: 107.6M of 300M"* first reported |

The 25M checkpoint had already been reported as crossed before the raise arrived. So the
existing checkpoint was **not** what the raise addressed; the budget was lifted during the
continuation, on the same instruction that carried T5–T7 and the closing issue. The
distinction is recorded because it is exactly the kind of thing a later session would get
backwards, and the record previously supported both readings.

**Why it is written down here rather than left in the issues.** `TOPICS.md` §7 makes spend
accounting part of the topic routine, and `AGENT-RULES.md` §9 makes `STATUS.md` the file a
session reads first. Both the 60M plan and the 300M raise lived only in issue #69 (open and
readable) and issue #76 (closed). A later session reading the documents in the order the
rules prescribe would find no budget figure at all.

**No behaviour changed.** A budget is not a product decision and no code was touched. The
figure bounds how long a `/goal` loop may run before it stops and reports.

`AGENT-RULES.md` §6 is unaffected: it governs whether a phase's checks passed, not what a
topic was allowed to spend.

