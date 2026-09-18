# Atrium — Open Questions

Genuinely undecided. **None of these block a v1 build.**

Do not invent answers to these while building. If a phase forces one of them,
stop and ask.

---

## Design

**Child workspace notes.** Child workspaces are removed at task end, which would
destroy anything the child learned. Likely fix: a child's notes are handed up to
the parent during wind-down. Not designed yet.
*(Needed by: Phase 11)*

**Scope slider defaults beyond the five points.** The five levels are settled;
what each one means in edge cases (mounted paths, read access outside the write
scope) is not fully specified.
*(Needed by: Phase 12)*

**What "officially supported" promises.** A store label implies code review,
which is an ongoing commitment. A label applied without review is worse than no
label — it converts a warning people would heed into trust they should not have.
Decide what it promises before anything carries it.
*(Needed by: the store, far later)*

**Undo for the user's own actions.** The gate does not apply to the human, and
snapshots are taken per-task, so the user's own bulk deletions are unprotected.
A scheduled snapshot or an explicit undo would cover it.
*(Needed by: nothing yet)*

**Host access and its permission model.** Off entirely in v1, not requestable.
When it ships it needs a permission model and a snapshot story, since snapshots
protect the environment root only.
*(Needed by: whenever host access ships)*

---

## Implementation

**Which component emits each agent state.** Some come from the agent loop
(`thinking`, `acting`), some from Atrium (`waiting-on-lock`, `blocked-by-gate`,
`held`), some from the watchdog (`stuck`, `looping`). This decides where the
code lives. It is an implementation question, not a design one — nothing else
depends on it.
*(Needed by: Phase 11)*

**The model scoring ruleset.** Atrium ships a ruleset that queries the
provider's model list at runtime and ranks by context size, cost per token,
tool-calling reliability and latency. The weights and thresholds are not
designed.
*(Needed by: Phase 12)*

**Lock timing numbers.** Heartbeat interval, expiry window (~60s), countdown
threshold (final 20s). The *rule* is settled; the numbers are tuning.
*(Needed by: Phase 11)*

**Watchdog thresholds.** How long without a tool call counts as `stuck`; how
many identical calls count as `looping`.
*(Needed by: Phase 4, with placeholder values)*

**Destructive rate limits.** How many deletions per minute triggers a pause.
*(Needed by: Phase 4, with placeholder values)*

---

## Deliberately deferred

Not open questions — decided to postpone.

- Agent vision (letting agents see images) — the media viewer is human-side only
- The browser surface — external Playwright process, past v1
- Native external surfaces (Figma etc.)
- Process isolation between surfaces
- Drag-and-drop surface install
- Real-time co-editing of a file — genuinely expensive, out of scope
- Docker-per-workspace isolation (AgentGUI's approach) — worth revisiting later
