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

### A known bug that is safe to leave for now: the snapshot store's textual fallback

`store_is_inside_root` (`crates/snapshot/src/lib.rs`, the fallback match arm below)
compares the two paths **textually** whenever either one does not exist:

```rust
_ => store.starts_with(root),   // component-wise, but NOT normalised
```

The comparison is component-wise, so it is not fooled by a name that merely shares
a string prefix — but it does not normalise, so a store that really sits *outside*
the root is judged by its spelling. A legitimate store at `<root>/../elsewhere/store`,
spelled that way and not yet created, is refused as being "inside the environment
root"; create the same store first and it is accepted. That was probed and recorded
in `atrium-fix-1b-report.md` §6, and it is the same *shape* as the resolver bug fixed
on 18 Sep 2026 — a spelling-based containment decision — which is why it is written
down rather than left to be rediscovered.

**Why it is safe to defer: it refuses a store it should have allowed, and never
allows one it should have refused** — the whole error is in the harmless direction,
and the containment decision is the one that has to be right, not the convenience
of the spelling. No reachable escape exists: with the store absent,
`validate_store` refuses first ("the store does not exist; refusing rather than
creating it") and `StoreInsideRoot` is reported after it, so the wrong verdict is
only ever reached in the over-refusal direction.

Fix it when the store's spelling is next touched — normalising before the textual
compare is the whole change, and it wants the same "record the failure first" the
resolver fix got.
