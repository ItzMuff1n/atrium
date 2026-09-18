# Delegation brief — Phase 1: sandbox path resolution

**Prepared by:** glm-5.3 (Sep 2026), for dispatch to a delegated subagent
(kimi-k3, leaf role).
**Status:** DISCHARGED — this brief was dispatched on 11 Sep 2026
(`deleg_7b0c9983`), the subagent built `resolver/` from it, and the parent
re-ran the build, the tests, the demo, an independent attack sweep and the §12
blind list. Phase 1 is built and verified agent-side; Muffin's hands-on pass is
the outstanding step.

*(Corrected 12 Sep 2026: the status line read "APPROVED for dispatch ... when
Phase 0 completes and the user says go." The dispatch had already happened, so
it read as pending work that was in fact finished. Kept in the project because
it is the spec the resolver was built from, and still the authority on what
Phase 1 was supposed to do.)*

Earlier review resolved the three decisions formerly marked ⚠: relative-path
rejection accepted, crate layout accepted, non-existent-path handling deferred
to Phase 1b (see §8.4a).

This file is working documentation. It is not code and not a plan change.
Everything verbatim below is quoted from the project docs unchanged.

---

## 1. What you are building (the whole job)

Phase 1 of Atrium: **the sandbox path resolver**. One library function plus
a thin command-line harness to exercise it, plus tests. **Rust only. No UI.
No agent. No filesystem CRUD operations.** Nothing else. This is the single
most safety-critical piece of the entire project.

You create files ONLY under `/home/muffin/Desktop/Nexus project/resolver/`.
You do not touch `DESIGN.md`, `DECISIONS.md`, `BUILD-PLAN.md`, `STATUS.md`,
`OPEN-QUESTIONS.md`, `AGENT-RULES.md`, or `spikes/`.

## 2. Who you are working for (AGENT-RULES.md §1, verbatim)

> The person you are working with **does not read code and will not review it**.
> This project is built entirely by AI agents.
>
> That fact changes what your job is. You are not writing code for a reviewer to
> check. **You are the only thing standing between a bug and production**, and
> the compiler is the only other reviewer this codebase gets.
>
> Consequences you must internalise:
>
> - "It should work" is not a status. Run it.
> - A test you wrote is weak evidence. It shares the assumptions of the code you
>   wrote, including the wrong ones.
> - Do not describe what you intended. Describe what you observed.
> - If you did not run it, say you did not run it.

## 3. How to report (AGENT-RULES.md §3, verbatim)

> **Report what happened, not what you meant to happen.**
>
> Every report separates:
>
> - **Observed** — you ran it and saw this. Include the actual output.
> - **Changed** — files you created or modified, and what changed in them.
> - **Not done** — anything you skipped, stubbed, or left incomplete.
> - **Uncertain** — anything you are guessing about.
>
> Never write "fixed", "works", or "done" for something you did not execute.
>
> When you hit an error, **paste the error verbatim**. Do not summarise it. Do
> not paraphrase it. The user relays your output to another model that needs the
> exact text.

## 4. The design source (DESIGN.md §3.1–3.2, verbatim)

> ### 3.1 Shape
>
> A **closed environment with its own filesystem**. One directory on the host is
> the environment root. Everything an agent sees lives inside it. Paths are
> translated: the agent asks for `/home/documents`, the app resolves it to
> `<root>/home/documents`. The agent never learns the real path exists.
>
> Specific host folders can be **mounted** into the environment at a chosen path
> (`/mnt/project` → a real directory). Same mechanic as a VM shared folder.
>
> **v1: host access is OFF entirely.** Environment root only. Not a denied
> request — the option does not exist.
>
> ### 3.2 Path resolution — the most important code in the project
>
> Every path an agent supplies is resolved and checked against the allowed roots
> **before anything runs**. Without this check, `../../..` walks straight out of
> the sandbox.
>
> This is a known bug class (path traversal). It is the single place where a
> mistake is dangerous rather than annoying. It is built and verified first, alone,
> before any other subsystem exists. See `BUILD-PLAN.md` Phase 1.

> **Superseded quotation, flagged 12 Sep 2026.** The "allowed roots" sentence
> above is a verbatim quote of `DESIGN.md` §3.2 *as it stood when this brief was
> written*. It has since changed to **"the environment root"** — `resolve()` takes
> exactly one root permanently and mounts resolve to a `(root, subpath)` pair in a
> layer above it. See `DESIGN.md` §3.2 and `DECISIONS.md`. **Nothing in Phase 1
> changes as a result** (host access is off in v1, so one root is all v1 has), and
> this brief is discharged, so its quotation is left verbatim rather than edited.
> The Phase 1 requirements quoted in §5 below are unaffected — they come from
> `BUILD-PLAN.md`, which already said "the environment root" in the singular.

## 5. The build plan (BUILD-PLAN.md PHASE 1, verbatim)

> **Rust only. No UI. No agent. No filesystem operations yet.**
>
> One function: agent-supplied path in, resolved real path out, or a rejection.
>
> This is the only part of Atrium where a mistake is *dangerous* rather than
> annoying. It is built alone, first, and hardened before anything else exists.
>
> **Requirements:**
>
> - Resolve a virtual path (`/home/documents`) against the environment root.
> - Reject anything that escapes the root, after full canonicalisation.
> - Symlinks are resolved before the check, not after.
> - Rejections return a reason, not a bare failure.
> - The function is the *only* way any other code turns a virtual path into a real
>   one. Nothing else in the codebase may construct a real path.
>
> **Verification (user) — the most important verification in the project:**
>
> The user writes the attack list. Not the agent. At minimum:
>
> - `../../..` and deeper
> - `/../..` from the root
> - absolute host paths (`/etc/passwd`, `/home/muffin`)
> - a symlink inside the environment pointing outside it
> - a symlink pointing to another symlink pointing outside
> - trailing dots and spaces
> - null bytes in the path
> - unicode lookalike separators
> - very long paths
> - `.` and `..` interleaved (`a/./../../..`)
> - a path that resolves inside the root but passes through outside it
>
> Every one must be rejected, and the user must watch each rejection happen.
>
> **Do not proceed to Phase 2 until this passes.** Everything after this point
> assumes the sandbox holds.

## 6. Hard rules and paranoia for this subsystem (AGENT-RULES.md §6–7, verbatim)

> **The sandbox path resolver is the only way to turn a virtual path into a real
> one.** No other code constructs a real path. Not for convenience, not for a
> special case, not temporarily.
>
> **Never write code that reaches outside the environment root.** Host access does
> not exist in v1. If a task seems to require it, the task is wrong — stop and ask.
>
> **Never disable, bypass, or add an exception to a gate rule, a permission, or a
> lock to make something work.** If a rule blocks legitimate work, that is a
> finding to report, not an obstacle to route around.
>
> **Path resolution (`DESIGN.md` §3.2).** Path traversal is a known bug class with
> known tricks — symlinks, canonicalisation order, unicode separators, null bytes,
> trailing dots. Resolve fully, then check. Never check, then resolve.
>
> **The lock manager (`DESIGN.md` §10).** Concurrency bugs are invisible until they
> are catastrophic, and they do not reproduce reliably. Prefer the boring, obvious
> implementation over the clever one.
>
> For both: **if you find yourself writing something clever, stop.** Clever is how
> these fail.

(The lock-manager paragraph is quoted for the general principle; you are not
building the lock manager.)

## 7. Stop and ask (AGENT-RULES.md §5, verbatim — apply via your report)

> **Stop and ask the user** when any of these are true:
>
> - The design document does not cover the case you have hit.
> - Following the design would produce something that clearly does not work.
> - You need to change a decision recorded in `DECISIONS.md`.
> - You are about to touch the sandbox path resolver (`DESIGN.md` §3.2) for any
>   reason other than the phase that builds it.
> - You are about to add a dependency.
> - You would need to weaken a gate rule, a permission, or a lock to make
>   something work.
> - You have tried the same fix twice and it has not held.
>
> **Asking is cheap. A wrong assumption in a codebase nobody reads is not.**

You are building §3.2 in its own phase, so building it is sanctioned. If
anything else on that list triggers, say so in your report instead of
proceeding.

---

## 8. Exact build spec

### 8.1 Crate

- New standalone cargo project at
  `/home/muffin/Desktop/Nexus project/resolver/`.
- Crate name `atrium-resolver`, edition 2021. Rust toolchain 1.92.0.
- **Dependencies: NONE.** `std` only. Do not add a path-handling crate
  (path-clean, etc.) — a hand-rolled paranoid resolver is the point, and a
  third-party crate is exactly the wrong place for supply-chain risk.
  If you believe you need a dependency, that is a stop-and-ask.
- Library target (the resolver) + binary target (the harness) in one crate.

### 8.2 The function

The public API is one function plus a structured error type:

- Input: the environment root (real path, given by the caller) and the
  agent-supplied virtual path.
- Output on success: the resolved real `PathBuf`.
- Output on failure: a structured error whose `Display` is a reason a human
  reads — naming the offending component and the rule violated, e.g.
  "Rejected: `/home/documents/../../..` escapes the environment root
  (reaches `/`)."
- The lib function must never create, modify, or delete anything on the
  filesystem. Resolving requires reading (canonicalising) metadata; that is
  allowed. Mutation is not.

### 8.3 Behaviour requirements

1. Virtual paths are absolute within the environment (`/home/documents` →
   `<root>/home/documents`). **Relative virtual paths are rejected with a
   reason, not silently rooted** — strictness over convenience in the
   dangerous subsystem; loosening later is easy, tightening later breaks
   agents.
2. **Resolve fully, then check — never check, then resolve.** Walk and
   canonicalise the path component by component; the containment check must
   hold at every intermediate step, not just against the final target. This
   is what catches "resolves inside the root but passes through outside it"
   from the attack list.
3. **Symlinks must be resolved (canonicalised) before the containment
   check**, including symlink chains.
4. Lexical attacks the filesystem never sees must still be rejected:
   `..` at/above the root, interleaved `.`/`..`, empty path, doubled
   separators normalised, trailing slashes handled.
5. Rejection reasons must be per-case and specific. A single "invalid path"
   string for everything is a failure of this requirement.
6. Positive cases must work (a resolver that rejects everything is useless):
   plain nested paths, `.`/`..` that stay inside the root, spaces and
   unicode in filenames, trailing slash.
7. Mounts do not exist in v1 (`DESIGN.md` §3.1: "v1: host access is OFF
   entirely. Environment root only."). Do not build any mount mechanism.

### 8.4 Known non-goals (document, do not attempt to solve)

- **Hardlinks** inside the root pointing at outside inodes: path resolution
  cannot distinguish these from ordinary files. Out of scope; note it in the
  README.
- **Bind mounts** inside the root: same problem class. Out of scope; note it.
- **TOCTOU** (time-of-check-to-time-of-use): a symlink inside the root could
  be swapped to point outside *after* resolution but before the caller acts
  on the resolved path. No path resolver can prevent that; the real defence
  belongs to the file-operations layer (Phase 2, e.g. opening files with
  `O_NOFOLLOW`). Out of scope here; note it in the README so it is not
  silently forgotten when Phase 2 is briefed.
- These are filesystem-level, not path-level; no path resolver can catch
  them. They are recorded honestly rather than pretended away.

### 8.4a Deferred to Phase 1b — do not build

Resolving paths that do not exist yet is NOT part of Phase 1. std::fs::canonicalize fails on a path whose target is absent, so this resolver only handles paths that already exist on disk. That is intentional and sufficient for Phase 1 verification.

Phase 1b will add it: canonicalise the longest existing prefix, then validate the non-existing remainder so it cannot traverse. It is split out because it is the only part of the resolver that reasons about something the filesystem cannot confirm, and it gets its own isolated verification round against its own attack list. Phase 2 depends on it, so it is postponed rather than dropped.

Do not build it, do not stub it, do not add a flag for it. If a test case requires it, that test case belongs to Phase 1b.

### 8.5 The command-line harness

Three modes, all boring:

1. **Single path:** `atrium-resolver resolve --root <dir> <virtual-path>` →
   prints `ACCEPT` + the resolved real path, or `REJECT` + the reason.
   This is what the user will drive by hand against attack-list.md.
2. **Demo suite:** `atrium-resolver demo` → builds a fixture environment in
   a fresh temp dir (creating the symlink attack fixtures), runs every
   applicable line from attack-list.md, prints each attack and its result, and
   exits non-zero if any attack is wrongly accepted. One command the user
   can watch.

A third mode, `atrium-resolver fixtures --root <dir>`, creates every directory, file and symlink referenced in sections D and I of attack-list.md so the single-path mode can be driven by hand. It creates nothing outside the given root.

### 8.6 Tests

`cargo test` must cover, at minimum, every item in the BUILD-PLAN attack
list (§5 above) plus the positive cases (§8.3 item 6). Test fixtures are
created inside `std::env::temp_dir()` subdirectories and cleaned up. Pointing
a test symlink at a real outside path (e.g. `/etc/passwd`) is allowed —
that is a read-only stat proving rejection; **never write outside your temp
fixture dirs.**

The authoritative attack list for this phase is "/home/muffin/Desktop/Nexus project/attack-list.md". Read it. Your tests must cover every line in sections A through H (all must be rejected) and every line in section I (all must be accepted), except any line that depends on resolving a non-existent path — those belong to Phase 1b. Rejections must carry a reason specific to that path; a single generic reason across many cases fails §8.3 item 5.

Null bytes: NUL (U+0000) is **valid UTF-8 and can appear in a Rust `&str`** —
do not assume the type system blocks it. The OS layer does: `std::fs` APIs
reject paths with interior NULs (`InvalidInput`, because libc strings are
NUL-terminated). The resolver must **additionally** reject NUL explicitly
with its own named reason — defense in depth, and the rejection reason must
be the resolver's, not a leaked OS error. Test both layers.

Unicode lookalike separators (U+2044, U+2215, fullwidth solidus): on Linux,
Rust treats these as ordinary filename characters — contained by design.
Test and prove it, do not assume it.

Very long paths: expect `ENAMETOOLONG`-class failures; a clean rejection
with a reason is the correct behaviour. Test it.

### 8.7 Tooling notes (real, from Phase 0 experience)

- Known false positive: the standalone file linter reports E0670 "async fn
  not permitted in Rust 2015" on edition-2021 files — it ignores
  Cargo.toml. **`cargo build` is the arbiter.**
- The project directory contains a space in its name
  (`Nexus project`); quote paths in shell commands.

## 9. What NOT to do

- No UI, no agent loop, no filesystem CRUD, no watchers, no gate, no locks,
  no mounts, no config files, no "while I'm here" features. Phase 1 is one
  function and its verification surfaces. If something later needs more,
  write it down in your report instead of building it.
- No edits to any project file outside `resolver/`.
- No dependencies.
- No clever implementations. Boring and obvious, every time.

## 10. Required report shape (output_schema for the delegation call)

Your final answer must validate against this schema — plain, honest strings:

```json
{
  "what_was_built": "one paragraph: the function, harness, and tests as actually implemented",
  "files_changed": ["every file path created or modified"],
  "commands_run_with_output": "verbatim transcript: cargo build, cargo test, and the demo run — real output, unedited",
  "what_was_not_done": "anything skipped, stubbed, or left incomplete",
  "what_is_uncertain": "anything you are guessing about, including any spec point you found ambiguous"
}
```

## 11. Dispatch parameters (for the parent agent, not the subagent)

- delegate_task, role: leaf (orchestrator is force-downgraded anyway).
- goal: "Build Phase 1 of Atrium per the brief below — verbatim."
- context: this entire file, pasted.
- output_schema: §10 above.
- One subsystem at a time: this is the only task in the batch.
- After return, the parent must independently re-run build/test/demo and its
  own spot attacks before reporting to the user; the user then performs the
  hands-on verification with his own attack list (BUILD-PLAN: "The user
  writes the attack list. Not the agent.").

**Deviation from the above, 11 Sep 2026 (parent's own decision):** the dispatch
did **not** paste this file as context. Instead the child was told to
`read_file` this file at its absolute path and read it in full. Reason: this
file is 335 lines of dense spec and pasting it means the parent retypes it by
hand; a single transcription error silently builds the wrong thing. A file read
is byte-exact and cannot drift. The child was also told, explicitly, that if
the dispatch text appeared to conflict with the file, the file wins.
The `skill_view(name='atrium')` instruction required by HANDOFF §7 item 3 was
included. Everything else above was followed.

## 12. Independence requirement (for the parent agent)

BUILD-PLAN requires the attack list come from outside the head that built the resolver. The list at attack-list.md was written by the reviewing model, not the user, so it shares some assumptions with this brief.

Before Phase 1 is signed off, a second attack list must be produced by a subagent that has seen none of the following: the resolver source, this brief, or attack-list.md. It is given only a plain description of what the sandbox is meant to guarantee, and asked what paths it would try. Every line it produces that attack-list.md does not already cover must be run against the resolver.

This is a gate on accepting the phase, not an optional extra.

**§12 satisfied 11 Sep 2026** (`deleg_c6716023`, model kimi-k3, 1 API call, pure
reasoning). The list is stored at `blind-attack-list.md` — it had existed only
in `/tmp/blind.json` and a delegation cache file, both outside the project.
Every novel line was run against the resolver with its fixtures rebuilt; zero
escapes, zero hangs. Prefix-sibling and middle-component symlink cases were
re-run independently on 12 Sep 2026 with the same result
(`resolver/probe-blind.sh`, output in `phase-1-evidence.txt` §5).

**Note on the child's delivery:** the delegation harness captured 2427 of the
child's characters, so its section 8 is incomplete. Sections 1–7 are complete as
delivered and are what was run. This is recorded in `blind-attack-list.md`
rather than passed off as the whole list.