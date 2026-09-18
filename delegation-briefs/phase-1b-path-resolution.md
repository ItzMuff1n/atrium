# Delegation brief — Phase 1b: paths that do not exist yet

**Prepared by:** the orchestrating Hermes session (Sep 2026), for dispatch to a
delegated subagent (leaf role).
**Status:** DISPATCHED 13 Sep 2026 (`deleg_a1e84631`), at Muffin's go, to a
delegated subagent (leaf role, kimi-k3). This file is the specification the
child is building from; it was approved for dispatch on 13 Sep 2026 and the
dispatch followed `delegation-briefs/phase-1-path-resolution.md` §11's
deviation — the child reads this file from disk rather than being given pasted
text, and was told the file wins over the dispatch message. Awaiting the child's
return; nothing it reports counts as a result until the parent has re-run the
build, the tests, the demo, `hand-test-1b.sh` and its own spot attacks.
**Source of the requirement:** `BUILD-PLAN.md` PHASE 1b, which quotes
`delegation-briefs/phase-1-path-resolution.md` §8.4a verbatim as the origin.
**Shape:** mirrors `delegation-briefs/phase-1-path-resolution.md` — Phase 1's
brief, which built `resolver/`. Read that brief too; this one assumes the same
conventions.

This file is working documentation. It is not code and not a plan change.
Everything marked verbatim is quoted from the project docs unchanged.

---

## 1. What you are building (the whole job)

Phase 1b of Atrium: **non-existent-path resolution**, added to the resolver that
already exists at `/home/muffin/Desktop/Nexus project/resolver/`. Phase 1
resolves paths that already exist on disk. It rejects everything else, including
paths a caller is entitled to create. Phase 1b closes that gap.

**Rust only. No UI. No agent. No filesystem operations.** You add no ability to
create, modify, or delete anything. Resolution stays a read-only operation.

You modify files ONLY under `/home/muffin/Desktop/Nexus project/resolver/`. You
do not touch `DESIGN.md`, `DECISIONS.md`, `BUILD-PLAN.md`, `STATUS.md`,
`attack-list.md`, `attack-list-1b.md`, `OPEN-QUESTIONS.md`, `AGENT-RULES.md`, or
`spikes/`.

**Phase 1's code is not yours to rewrite.** Extend `src/lib.rs` where the
resolution walk lives; keep `resolve(root, virtual_path)`'s signature exactly as
it is (see §8.2). If you think the existing walk is wrong, that is a
stop-and-ask (§7), not an edit.

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

## 4. The design source (DESIGN.md §3.1–3.2, verbatim, current text)

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
> Every path an agent supplies is resolved and checked against the environment root
> **before anything runs**. Without this check, `../../..` walks straight out of
> the sandbox.
>
> **Clarification, 12 Sep 2026 — mounts do not widen the resolver.** This sentence
> previously read *"the allowed roots"*, plural, which read as though a path could
> be checked against more than one root; §3.1's mounted host folders reinforced
> that reading. Resolved by design decision (Muffin, 12 Sep 2026): **the resolver
> always takes exactly one root, and that is permanent.**
>
> When mounts arrive they get their own layer *above* the resolver. That layer maps
> a virtual path to a `(root, subpath)` pair and then calls `resolve()` unchanged.
> Mounts are real in the design but host access is off entirely in v1, so v1 has
> exactly one root and no mount table.
>
> Reason for keeping the signature narrow: `resolve()` is the one function where a
> mistake is dangerous. Widening it to accept a set of roots would mean
> re-verifying it — and its whole attack list — every time the mount logic changed.
> Keeping it single-root means the mount layer can be wrong without the sandbox
> being wrong.
>
> Phase 2 onward builds on the current signature, as built and verified.
>
> This is a known bug class (path traversal). It is the single place where a
> mistake is dangerous rather than annoying. It is built and verified first, alone,
> before any other subsystem exists. See `BUILD-PLAN.md` Phase 1.

## 5. The build plan (BUILD-PLAN.md PHASE 1b, verbatim, in full)

> # PHASE 1b — Paths that do not exist yet
>
> **Rust only. No UI. No agent. No filesystem operations.**
>
> **Requirement (verbatim from `delegation-briefs/phase-1-path-resolution.md`
> §8.4a, which remains the origin of this requirement):**
>
> > Resolving paths that do not exist yet is NOT part of Phase 1. std::fs::canonicalize fails on a path whose target is absent, so this resolver only handles paths that already exist on disk. That is intentional and sufficient for Phase 1 verification.
> >
> > Phase 1b will add it: canonicalise the longest existing prefix, then validate the non-existing remainder so it cannot traverse. It is split out because it is the only part of the resolver that reasons about something the filesystem cannot confirm, and it gets its own isolated verification round against its own attack list. Phase 2 depends on it, so it is postponed rather than dropped.
> >
> > Do not build it, do not stub it, do not add a flag for it. If a test case requires it, that test case belongs to Phase 1b.
>
> *(End of verbatim quote. The paragraph below is new.)*
>
> **Why it is its own phase and not part of Phase 2a.** Non-existent-path handling
> is the only part of the resolver that reasons about something the filesystem
> cannot confirm. If it shipped alongside Phase 2a's file-operation plumbing and
> the attack list found a hole, there would be no way to tell which half was
> wrong. The split exists precisely so the two can be verified in isolation.
>
> **Verification — its own attack list, written before the code:**
>
> Phase 1b needs an attack list covering **non-existent paths** specifically:
> paths whose target is absent, paths whose target is absent *and* whose parent
> chain walks outside the root, a non-existent remainder attached to a symlinked
> existing prefix, and the same for a path that creates a traversal only after the
> non-existent tail is appended. Written before the code, in the same shape as
> Phase 1's two lists, and watched by Muffin line by line like the first one.
>
> **Phase 2 does not start until Phase 1b passes.** Resolution must hold for paths
> that do not exist before anything creates files.
>
> **Not built, not stubbed, no flag exists for it today.** Phase 1 deliberately
> contains none of this; any Phase 1 test case covering it belongs here instead.

The four coverage shapes BUILD-PLAN names, for your report:

1. paths whose target is absent → §8.3 requirements, attack-list-1b sections I and J
2. paths whose target is absent *and* whose parent chain walks outside the root
   → attack-list-1b sections A, B, F, G
3. a non-existent remainder attached to a symlinked existing prefix
   → attack-list-1b section C
4. a traversal that appears only after the non-existent tail is appended
   → attack-list-1b section D

## 6. The environment namespace — decided, and it constrains you

`DECISIONS.md`, "The environment namespace follows the Linux tree" (Muffin,
13 Sep 2026), verbatim:

> **Decision (Muffin, 13 Sep 2026).** Atrium's environment root uses the standard
> Linux directory names and structure — `/home`, `/etc`, `/tmp`, `/var`, `/usr`,
> `/opt`, `/srv`, `/mnt`, `/root`. **Naming and structure are what matter**, so an
> agent or a human landing in the environment finds the layout they already know.
>
> **Scope limit, in his words: only necessary or created files. Anything that
> does not make sense in the long run is kept out.** The tree is not populated to
> look complete. A directory exists because something needs it or something made
> it.
>
> **Excluded: `/proc`, `/sys`, `/dev`.** On real Linux the kernel generates these
> live. Atrium has no kernel, so reproducing them means empty directories that
> carry the name of something real and hold nothing. The terminal surface runs a
> real shell, so commands reading them would fail in ways that read as breakage.
> Leaving them out makes them resolve as absent — honest — rather than as empty
> lies. This is the comprehensibility principle applied to the filesystem: a
> directory that looks real and is not is worse than one that is not there.
> Revisit only if a later surface genuinely needs them.
>
> **Rejected: reserving host-looking names so they can never exist.** It buys a
> less alarming hand-test and costs a special case in the resolver plus a list to
> maintain. The resolver already contains everything by construction — a leading
> `/` is the environment root — so a name is only ever a name. Nothing needs
> forbidding.
>
> **Consequence for Phase 1b.** `/etc/passwd`, `/root/newfile` and similar must
> come back ACCEPT once non-existent paths resolve. By hand they read as
> catastrophic escapes and are not. Recorded in `attack-list.md` §B — read it
> before driving the 1b pass.

Two things follow that you must not get wrong:

- **You build no namespace.** You do not create `/home`, `/etc` or anything
  else, and you do not special-case any name. The decision says the tree is not
  populated to look complete — a directory exists because something needs it.
  Your code must be correct whether or not `<root>/etc` happens to exist.
- **`/etc/passwd` is not a host path. It is the name `etc/passwd` inside the
  root.** Once it does not exist, it must resolve to `<root>/etc/passwd` and be
  **accepted**. So must `/root/newfile`, `/proc/self/newfile`, `/sys/class/newfile`
  and `/dev/newfile`. `/proc`, `/sys` and `/dev` are excluded from the namespace,
  so those always resolve as absent — which is the honest outcome the decision
  asks for, and still an ACCEPT.

## 7. Hard rules and paranoia for this subsystem (AGENT-RULES.md §6–7, verbatim)

> **The sandbox path resolver is the only way to turn a virtual path into a real
> one.** No other code constructs a real path. Not for convenience, not for a
> special case, not temporarily.
>
> **Never write code that reaches outside the environment root.** Host access does
> not exist in v1. If a task seems to require it, the task is wrong — stop and ask.
>
> **Never disable, bypass, or add an exception to a gate rule to make something
> work.** If a rule blocks legitimate work, that is a finding to report, not an
> obstacle to route around.
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

## 8. Stop and ask (AGENT-RULES.md §5, verbatim — apply via your report)

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

You are building §3.2 in its own phase, so extending the resolver is sanctioned.
If anything else on that list triggers, say so in your report instead of
proceeding. In particular: **do not "fix" Phase 1 behaviour you believe is
wrong.** Report it.

---

## 9. Exact build spec

### 9.1 Where the code goes

Same crate, same package: `/home/muffin/Desktop/Nexus project/resolver/`, crate
`atrium-resolver`, edition 2021, Rust 1.92.0. **Dependencies: NONE.** `std`
only. Adding a path-handling crate is a stop-and-ask — a hand-rolled paranoid
resolver is the point.

### 9.2 The model you are implementing

A virtual path's leading `/` is the **environment root**, not the host root.
`/etc/passwd` means `<env-root>/etc/passwd`. Source:
`resolver/README.md:14-15`. Confirmed empirically and recorded in
`DECISIONS.md` §"The environment namespace follows the Linux tree".

**A name is only ever a name.** Exactly two things escape the environment root:

1. a `..` step that walks above the root, or
2. a symlink whose *target* resolves outside the root.

Nothing else. There is no path spelling that is itself an escape. Your job is to
keep those two true when part of the path does not exist.

### 9.3 The signature does not change

    pub fn resolve(root: &Path, virtual_path: &str) -> Result<PathBuf, ResolveError>

Exactly one root, permanently (`DECISIONS.md`, 12 Sep 2026). Same
`ResolveError`, extended with new named reasons as needed. The function still
never creates, modifies, or deletes anything.

### 9.4 Behaviour requirements

1. **An absent path inside the root is ACCEPTed.** This is the whole phase.
   `resolve(root, "/newfile")` must return `<root>/newfile` even though nothing
   is there. "Does not exist" stops being a rejection reason for a path that
   stays inside.
2. **The step rule is unchanged.** Containment is checked at every step, never
   only at the end. `/link-to-etc/../home/documents` still rejects because the
   step through `/etc` left the root.
3. **The absent remainder is walked as text.** Nothing on disk confirms it, so
   normalise it yourself: `//` collapses, `.` is elided, `x/..` removes `x`, and
   a `..` that pops above the root is a rejection naming that step.
4. **The walk has two modes and it must not confuse them.** While components
   canonicalise on disk, keep Phase 1's behaviour exactly. The first component
   that canonicalises as *not found* switches the rest of the walk to textual
   mode. In textual mode nothing is stat-ed; the remainder is normalised per
   item 3 and the final result is checked against the canonical root.
5. **But a component that exists and is not a directory is not "not found".**
   `/home/documents/notes.txt/..` and `/home/documents/notes.txt/newfile` must
   stay rejected with the operating system's own named reason
   (`Not a directory`), exactly as Phase 1 rejects them today. Do not fall into
   textual mode for `ENOTDIR` — only for `NotFound`. See §9.5 ruling 2.
6. **A symlink whose target is absent is still a symlink.** If canonicalising a
   component fails as not-found, check whether that component is itself a
   symlink before treating it as an ordinary absent name. If it is, its target
   is what matters: a target outside the root is a rejection, whatever the
   target's existence. This is the one place where "absent" does not mean
   "untraversable", and it is why this phase exists separately.
7. **Rejection reasons stay per-case and specific.** A single "invalid path"
   string for everything fails Phase 1's §8.3 item 5 and fails here. For a
   traversal, the reason names the step that left the root — not merely that
   something does not exist.
8. **Empty path still rejected; relative paths still rejected** with their own
   reasons; NUL still rejected with the resolver's own named reason, not a
   leaked OS error.
9. **Positive cases must keep working**, including Phase 1's whole accept
   section. A resolver that rejects every `..`, or every path through a symlink,
   breaks ordinary agent work.
10. **Component length is checked by the resolver, in bytes.** Any single
    component over **255 bytes** is rejected by the resolver's own check, not by
    waiting for the filesystem to raise `ENAMETOOLONG`. **Bytes, not
    characters** — `NAME_MAX` is 255 bytes, so a 255-character ASCII name is
    fine and a 128-character Hebrew name is not. Measure the UTF-8 encoded
    length. **Per component, not per path**: a long path of short names is fine,
    and `PATH_MAX` (4096) is deliberately not enforced — it is a libc buffer
    convention, not a filesystem rule. The reason the verdict must come from the
    resolver is that it has to be identical whether the path exists or not: a
    name that can never be created must not resolve cleanly just because nobody
    has tried yet. Decided by Muffin, 13 Sep 2026; `DECISIONS.md` "Phase 1b —
    three resolver rulings" is the authority.

    The trap this closes: a character-counting implementation agrees with the
    filesystem on English names and disagrees on everything else, and would pass
    every test written in English. Observed on this host 13 Sep 2026 —
    255 × `a` created fine, 256 × `a` failed `File name too long`; 127 Hebrew
    characters (254 bytes) created fine, 128 (256 bytes) failed; 63 emoji (252
    bytes) created fine, 64 (256 bytes) failed.

### 9.5 Rulings already made — implement these, do not re-open them

Muffin's instruction was that these are implementation calls: make them, record
the reasoning, do not surface them. They are already recorded in `DECISIONS.md`.
Implement all three as stated.

**Ruling 1 — a dangling symlink inside the root is ACCEPTed.**
`/link-to-nothing` exists and points at `/home/documents/absent.txt`, which does
not. Resolving `/link-to-nothing` **accepts** and returns the target's location.
Rationale: Phase 2 must be able to create files through such a path, and the
resolved location is inside the root. Its counterpart, a dangling link pointing
outside (`/link-to-nothing-out` → `/etc/absent.txt`), **rejects** — the target is
outside whether or not it exists, which is the distinction requirement 6
protects.

**Ruling 2 — a file followed by `..` is rejected, and the operating system
decides.**
`/home/documents/notes.txt/..` normalises lexically to `/home/documents`, inside
the root — but on a real filesystem `notes.txt/..` is `ENOTDIR`. The existing
prefix does exist, so the filesystem gets a say: rejected with the OS reason. Do
not normalise textually over a prefix that exists as a non-directory. This is
Phase 1's current behaviour and it is deliberate. Observed 13 Sep 2026:

    REJECT Rejected: the operating system refused `/home/documents/notes.txt/..`
    while resolving (`Not a directory (os error 20)`)

**Related, and also deliberate — do not "fix" it.** A *trailing slash* on a file
(`/home/documents/notes.txt/`) is ACCEPTed and resolves to the file, where POSIX
says it should fail with `ENOTDIR`. This is a recorded Phase 1 deviation
(`DECISIONS.md`, attack-list corrections pass, 12 Sep 2026): contained, so not a
security issue, recorded because it is real. Observed 13 Sep 2026:

    ACCEPT /tmp/atrium-rcheck/home/documents/notes.txt

The two coexist: a trailing slash has nothing after it, so there is no component
to fail against; `/file/..` and `/file/name` do have one, and it fails. Keep both
behaviours as they are. If you think either is wrong, report it — §10.

**Ruling 3 — backslash is an ordinary filename character, never a separator.**
`/home\..\..\newfile` is one filename on Linux. It is not traversal and must
never be rejected as traversal. If it is absent it **accepts** like any other
absent name inside the root. Phase 1's attack-list section E already states the
principle for existing paths ("either resolve inside the root or reject as
not-found — never as traversal"); this is the same rule for absent names.

### 9.6 Known non-goals (document, do not attempt to solve)

Inherit Phase 1's, unchanged — restate them in the README if you edit it:
hardlinks to outside inodes, bind mounts, and TOCTOU are filesystem-level, not
path-level, and no path resolver can catch them. The real TOCTOU defence belongs
to Phase 2 (`O_NOFOLLOW`).

Add one: **you do not populate or reserve any part of the namespace.** No
special case for `/etc`, `/proc`, `/root` or anything else. `DECISIONS.md`
rejected reserving host-looking names; the resolver contains everything by
construction.

### 9.7 The harness

`resolver/` already has `resolve`, `demo` and `fixtures` modes, plus
`hand-test.sh` (sections A–I of `attack-list.md`) and two probe scripts.

- `resolve --root <dir> <virtual-path>` — unchanged interface. Same
  ACCEPT/REJECT output shape.
- `fixtures --root <dir>` — extend it to create the Phase 1b fixtures as well:
  the dangling symlinks `/link-to-nothing` (target inside the root, absent) and
  `/link-to-nothing-out` (target outside the root, absent). Creating nothing
  outside the given root, as now.
- **Add `hand-test-1b.sh`**, in the same shape as `hand-test.sh`: run every line
  of `attack-list-1b.md` against a throwaway root, print each path and its
  verdict, check independently that every ACCEPT lands inside the root, exit 0
  only if every line behaved.
- **Do not change `hand-test.sh` or `attack-list.md`.** They are Phase 1's
  record. Several of their lines change verdict once your code lands — see
  §9.9. Leave them alone and say so in your report.

### 9.8 Tests

`cargo test` must keep every existing test passing, and add coverage for every
line of `attack-list-1b.md` — sections A–H all rejected with a reason naming the
escape, section I and section K all accepted and verified contained, and section
J (length) one accept and three rejects.

**Three length tests are required by name**, because item 10 decides them and
Phase 1's suite currently asserts the opposite (`tests/resolver_tests.rs`
`:304-307` reads "300-char single component → ENAMETOOLONG-class clean
rejection"; the 200-nested-components line at `:308-311` rejects for the same
reason). Both **must be updated**:

- a **255-byte** ASCII name → **ACCEPT**;
- a **256-byte** ASCII name → **REJECT**;
- a name **under 255 characters but over 255 bytes** — e.g. 200 Hebrew
  characters, 400 bytes — → **REJECT**. *This is the one a character-counting
  implementation gets wrong.* If your code counts characters, this test is the
  only one that will fail; that is why it is non-optional.

**Build them so they can only pass for the right reason.** Item 10 says the
resolver measures length *itself* rather than waiting for the filesystem to
raise `ENAMETOOLONG`, so that the verdict is identical whether the path exists
or not. A test that creates a 256-byte fixture and checks it rejects proves
nothing about that — the filesystem would have raised `ENAMETOOLONG` anyway.
Test the **absent** cases, so the only possible source of the rejection is your
own check, and assert the reason is the resolver's own rather than a leaked
operating-system error — the same standard `attack-list.md` §F sets for NUL.

Observed today against the current binary, for contrast: an absent 256-byte name
rejects with the **operating system's** message, not the resolver's —

    REJECT Rejected: the operating system refused `/aaaa…` while resolving (`File name too long (os error 36)`)

Under item 10 that reason must become the resolver's own. Also test the
**255-byte** name both ways: as a real fixture on disk (filesystem agrees) and
absent (your own check accepts it).

The 200-nested-components line is a long path of short names and **ACCEPTs**
under item 10. Its Phase 1 rejection was an absence, not a length verdict.

Fixtures go in `std::env::temp_dir()` subdirectories, cleaned up after. Never
write outside them. **The 255-byte accept needs a real fixture on disk**,
otherwise the accept is indistinguishable from the absent-path path and proves
nothing.

The three NUL lines in `attack-list.md` §F still cannot pass through the
command-line harness; keep them covered in-process. The same applies to any NUL
line in the 1b list.

Also test the two things that are easy to get wrong and would look fine:

- an absent path whose **parent chain leaves the root** (e.g. `/../newfile`),
  rejected with the `..` step named;
- an absent tail appended to a **symlink pointing outside** (e.g.
  `/link-to-etc/newfile`). Checking only that the prefix exists accepts this.

### 9.9 A consequence you must report, not fix

Phase 1's list contains lines that reject *because the target is absent*. Once
your code lands, those same paths resolve. Observed against the current binary:

    resolve --root <fixture> /aaaa…(300 chars)   REJECT  File name too long (os error 36)
    resolve --root <fixture> /home⁄documents     REJECT  does not exist inside the environment root

Under Phase 1b, `/home⁄documents` (U+2044) is an ordinary absent filename inside
the root — requirement 1 says it **accepts**.

The length lines are now **decided, not open**: item 10. The 300-character
component rejects, on the resolver's own 255-byte check rather than on
`ENAMETOOLONG`; the 200-nested-components line accepts. Do not present the
300-character case as an open choice — it was open in a previous draft and
Muffin closed it.

This is a **verdict change on existing lines, not a regression.** `attack-list.md`
and `hand-test.sh` are Phase 1's record and must not be edited to hide it. List
in your report every existing line whose verdict your change inverts.

### 9.10 Tooling notes (real, from Phase 0 and Phase 1)

- Known false positive: the standalone file linter reports E0670 "async fn not
  permitted in Rust 2015" on edition-2021 files — it ignores `Cargo.toml`.
  **`cargo build` is the arbiter.**
- The project directory contains a space in its name (`Nexus project`); quote
  paths in shell commands.
- `find` on this host is a wrapper that does not support `-print` and has
  returned empty for files `ls` shows. Distrust it; cross-check with `ls`.

## 10. What NOT to do

- No UI, no agent loop, no filesystem CRUD, no watchers, no gate, no locks, no
  mounts, no config files, no namespace population, no "while I'm here"
  features.
- No edits to any project file outside `resolver/`.
- No dependencies.
- No rewriting Phase 1's walk beyond extending it. No "fixing" Phase 1
  behaviour you disagree with — report it.
- No clever implementations. Boring and obvious, every time.

## 11. Required report shape (output_schema for the delegation call)

```json
{
  "what_was_built": "one paragraph: how the walk was extended, in what modes, and how the absent remainder is normalised",
  "files_changed": ["every file path created or modified"],
  "commands_run_with_output": "verbatim transcript: cargo build, cargo test, the demo run, and hand-test-1b.sh — real output, unedited",
  "verdicts_inverted": "every existing attack-list.md line whose verdict this change flips, with the old and new verdict",
  "length_check": "how component length is measured (bytes, per component) and the four observed results: 255-byte accept, 256-byte reject, over-255-byte multi-byte reject, 200-nested accept",
  "phase1_tests_updated": "the Phase 1 tests you had to change because item 10 decided their lines the other way, with the old and new assertion",
  "what_was_not_done": "anything skipped, stubbed, or left incomplete",
  "what_is_uncertain": "anything you are guessing about, including any spec point you found ambiguous"
}
```

## 12. Dispatch parameters (for the parent agent, not the subagent)

- `delegate_task`, role: leaf.
- goal: "Build Phase 1b of Atrium per the brief below — verbatim."
- context: this entire file. Follow Phase 1's deviation (11 Sep 2026): the file
  is read from disk by the child rather than pasted, because a byte-exact read
  cannot drift and a hand-pasted spec silently builds the wrong thing. Tell the
  child explicitly that if the dispatch text appears to conflict with the file,
  **the file wins**.
- Include the `skill_view(name='atrium')` instruction, as Phase 1's dispatch did.
- output_schema: §11 above.
- One subsystem at a time: this is the only task in the batch.
- After return, the parent must independently re-run build/test/demo/hand-test-1b
  and its own spot attacks before reporting. Muffin then drives
  `attack-list-1b.md` by hand — that is the gate.

## 13. Independence requirement (for the parent agent)

BUILD-PLAN requires the attack list come from outside the head that built the
resolver. `attack-list-1b.md` was written by the reviewing model, so it shares
assumptions with this brief.

Before Phase 1b is signed off, a second attack list must be produced by a
subagent that has seen **none** of: the resolver source, this brief,
`attack-list.md`, or `attack-list-1b.md`. It is given only a plain description
of what the sandbox is meant to guarantee and asked what paths it would try.
Every line it produces that `attack-list-1b.md` does not already cover must be
run. **This is a gate on accepting the phase, not an optional extra.** Phase 1's
equivalent is `blind-attack-list.md` (`deleg_c6716023`).

## 14. Open items before dispatch — for the parent, not the subagent

*(Updated 13 Sep 2026 after Claude's edits to `DECISIONS.md` and
`attack-list.md`.)*

1. **Muffin's go.** Nothing is dispatched until he says so.
2. ~~`attack-list-1b.md` must be re-derived against the namespace decision.~~
   **Done 13 Sep 2026** — the bad section was removed, its coverage moved to §K
   as must-ACCEPT, the length decision added as §J, sections renumbered, and the
   list re-walked mechanically afterward.
3. ~~`DECISIONS.md` needs the three rulings recorded.~~ **Done** — Claude added
   "Phase 1b — three resolver rulings", which is now the single home. The brief
   and the list are the working copies. It also carries the length decision, the
   trailing-slash warning and the same-spelling trap.
4. **`DECISIONS.md` is now the authority for the rulings; this brief and the
   list must not diverge from it.** If any of the three changes, all three change
   together.
5. **The blind second list (§13) is not written.** It needs its own go.
