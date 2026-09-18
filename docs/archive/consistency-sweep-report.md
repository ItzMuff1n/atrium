# Consistency sweep — report for the original agent

**Written:** 12 Sep 2026, by the Hermes session now acting as sole orchestrator.
**For:** the Claude session that coordinated Atrium up to 11 Sep 2026. Muffin is
handing this to you because you may know intent that is not written down.
**Trigger:** Muffin read the project, saw "a lot of inconsistencies", and asked
for everything that collides to be found, reasoned about, and adjusted. Scoped
explicitly to cleanup — "do not imagine anything new regarding the overall
project's trajectory."

**Read this before the project docs.** It records what I changed and, more
importantly, the four judgment calls I made that you may want to overturn, and
the two things I could not decide.

> **Update, 12 Sep 2026.** Muffin answered both questions in §4 and decided §5.
> `resolve()` stays single-root permanently; Phase 1b is now its own numbered
> phase in `BUILD-PLAN.md`; both skills are pointers, not copies. **§4 and §5
> below are therefore historical** — they record what was asked, not what is
> still open. The decisions are recorded in `DECISIONS.md` under "Post-sweep
> decisions", and `STATUS.md` Known-broken is empty again. Everything in §2 and
> §3 stands as written.

> **Update, 13 Sep 2026.** Two things in this report have since changed, and both
> are stated here so the report is not read as current where it is not.
> **Phase 1b was built and passed its gate**, so §3's note that non-existent-path
> handling "does not exist yet" is now historical — the resolver resolves absent
> paths, and `STATUS.md` "Verified hands-on" records the pass. And the
> **"Muffin's hands-on pass has not happened"** caveat below is spent: Phase 1 was
> signed off hands-on on 13 Sep 2026, then Phase 1b. Everything else in §2 and §3
> — what was changed, and the judgment calls recorded there — stands as written
> and as this session's record.

---

## 1. The short version

Thirteen collisions. Eleven were one document contradicting another document or
the code — fixed by making them agree. Two cannot be fixed by documentation and
are recorded, not resolved.

Every one of them was a **stale statement left behind by a change**, not a
disagreement about design. That is the pattern, and it matters: nothing here
suggests anyone misunderstood the project. It suggests nobody swept after the
edits, and HANDOFF.md §6 already predicted exactly this ("Sweep for stale
references after every edit").

**No design decision was changed.** Nothing about what Atrium is or where it is
going was touched.

---

## 2. What I changed, file by file

Line numbers are approximate — read the files, not this list.

### `HANDOFF.md`
- **§3** said *"No Atrium code exists. Phase 1 has not started."* This
  contradicted §7 of the same file and STATUS.md, both of which said Phase 1 was
  dispatched, built and verified agent-side. Corrected, with a note recording
  that the line was stale rather than silently overwritten.
- **§1** said *"Phase 1 and Phase 1b still need independent verification."*
  Phase 1b does not exist. Corrected to Phase 1 only.
- **§5**, the `hermes verify` gotcha: said the only recourse for a CLI crate is
  `--skip-start`. Still true in general, but `resolver/.hermes/environment.json`
  was written with `start: null`, which stops the bogus port-8000 poll for this
  crate. Added, flagged as a per-project workaround that a recipe regeneration
  will silently undo.
- **§6** gained a new entry: the two Hermes skills are byte-copies of two
  project documents and **nothing syncs them**. See §5 below.

### `STATUS.md`
- **"Current phase"** still read *Phase 0 — Spikes … when it passes, Phase 1
  begins.* Rewritten: Phase 1 built and verified agent-side, not signed off;
  Phase 0 not closed; the 11 Sep ordering override named; Phase 1b flagged as
  non-existent.
- **"Notes for the next session"** said *"Nothing is built."* Corrected.
- **The brief entry's closing "User action"** line still said *"Dispatch happens
  after 0c passes"* — four lines below a *Superseded* note saying the dispatch
  had already happened. This is the single most dangerous one in the set: it
  told Muffin to wait on a blocker that no longer existed. Corrected.
- One *"Not done"* line inside the superseded brief entry now reads
  *"as of that moment"* so it cannot be read as present tense.
- **"Known-broken / blocked"** was empty. Now holds the two unresolved items.
- Added a sweep entry to "agent-reported, unverified" (newest-first, per the
  ledger rule).
- Marked the model-config line as the single home for live config values.

### `attack-list.md`
- Header said a second independent list was *"required before Phase 1 is signed
  off"* — it was produced on 11 Sep (`deleg_c6716023`). Marked satisfied.
- The **Independence** section said the same. Updated with the outcome.
- Section E: five lines moved out (see §3, judgment call 1).
- Section G: the bare `/` line removed (see §3, judgment call 2).
- Section F: added a note that the three NUL-byte lines **cannot be passed
  through the command-line harness at all** — argv is NUL-terminated, so the
  shell refuses them before the resolver sees them. They are covered by
  `cargo test` only. This gap was previously implicit.

### `DECISIONS.md`
- The **Path normalisation** entry said *"Action required … Not done yet — the
  file is the user's to correct."* It was done this session. Updated to done,
  and the separate G/I `/` contradiction added to the same entry.

### `DESIGN.md`
- **§3.2** — a note added recording that the sentence says *"allowed roots"*
  (plural) while the built resolver takes one root. **The sentence itself is
  unchanged.** See §4, question 1.

### `BUILD-PLAN.md`
- **Phase 1** — a note added that the original requirement *"The user writes the
  attack list"* was superseded by the two-list mechanism, and that **Phase 1b
  has no home in this plan** while Phase 2a routes file operations through the
  resolver. See §4, question 2. The requirement text itself is unchanged.

### `delegation-briefs/phase-1-path-resolution.md`
- Header said *"APPROVED for dispatch … when Phase 0 completes and the user says
  go."* It had been dispatched and the resolver built from it. Marked discharged,
  with the deviation note preserved.
- §12 (independence) gained the outcome, plus a note that the delegation harness
  captured only 2427 characters of the child's answer, so its section 8 is
  incomplete.

### `hermes-recon.md`
- Had **no date at all** and asserted `model.default: glm-5.3` with a
  `glm-5.3-flash` session override. Both are stale — the project moved to
  `deepseek-v4.1-flash` (delegation `kimi-k3`) on 11 Sep, confirmed live this
  session with `hermes config get`. Added a dated banner marking §1–2 stale,
  and left the findings as written because the *method* in the file (values live
  in `state.db`, not `config.yaml`; the precedence rules) is still correct.
  Flagged it against the project's own "one fact, one home" rule.

### `resolver/`
- **`src/lib.rs` — NOT TOUCHED.** The resolver itself is byte-identical to what
  the subagent produced (mtime 11 Sep 21:43, before this session). Everything I
  changed in `resolver/` is the demo harness, the tests, the scripts and the
  README.
- **`src/main.rs`** — the demo's five "informational" spec-conflict lines are now
  real section-I pass/fail cases; the informational loop and its comment block
  deleted; `/` removed from the G reject list. `cargo build` and `demo` exit 0.
- **`tests/resolver_tests.rs`** — four of the five moved lines were tested; the
  fifth (`/home/documents//`) was not. Added. Comments describing them as a
  "spec conflict" replaced with the resolution.
- **`README.md`** — the test-coverage sentence did not mention the NUL
  exception. Added.
- **`.hermes/environment.json`** — created with `start: null`, see §2 HANDOFF.
- **`hand-test.sh`, `probe-parent.sh`, `probe-blind.sh`** — created (previous
  turn, referenced here for completeness).

### New files
- `blind-attack-list.md` — the §12 blind list existed **only in `/tmp/blind.json`
  and a delegation cache file**, both outside the project, and `/tmp` does not
  survive a reboot. Copied in verbatim.
- `phase-1-hands-on.md`, `phase-1-evidence.txt` — previous turn.

---

## 3. The four judgment calls you may want to overturn

These are the places I decided something rather than transcribing a fix. Each is
defensible; none is beyond argument.

**1. The E/I contradiction: I ruled for section I.**
Five lines in section E were listed as reject-cases while being lexically
identical (after normalisation) to section-I accepts. I *moved them into section
I* rather than deleting them, so they are still tested.
Grounds: `DECISIONS.md` had already ruled that classification depends on the
resolved location, never on spelling — so I was applying an existing decision,
not making one. But you may have intended section I to drop its two lines
instead. If so, say which two and I will invert it.

**2. The bare `/` in section G: I ruled for section I and removed the G line.**
`/` was listed as a reject-case in G and a must-accept in I. A resolver that
rejects the environment root is broken rather than secure, so I is right.
Low-risk, but it is a second judgment in the same direction, so it is worth your
eye.

**3. I annotated rather than deleted.**
Every stale claim was corrected **and** kept, with a note saying what it had
said and when it changed. This makes the files longer and noisier. The
alternative — clean replacement — is shorter but loses the record that the
project spent a day disagreeing with itself.
Reasoning: `AGENT-RULES.md` §3 wants observed history, and `STATUS.md`'s rules
require superseded entries to be *kept as written*. If you would rather these be
clean, that is a legitimate call and it is yours.

**4. I added notes to `DESIGN.md` and `BUILD-PLAN.md` without changing their
text.**
`AGENT-RULES.md` §2 is explicit: *"If code and DESIGN.md disagree, stop and say
so. Do not silently 'fix' either one."* I did not silently fix either — I added
a clearly-marked block and left the original sentence intact, and both are
flagged in STATUS.md. But adding commentary to the settled design and to the
plan is invasive by the standards of those two files, and you may want the notes
moved out into a separate list.

---

## 4. The two things I could not decide — what I need from you

Both are in `STATUS.md` under **Known-broken / blocked**, and marked as *not* a
blocker for Phase 1 sign-off.

### Question 1 — is "allowed roots" plural on purpose?

`DESIGN.md` §3.2: *"Every path an agent supplies is resolved and checked against
the allowed **roots**."*

Meanwhile §3.1 describes host folders **mounted** into the environment at a
chosen path (`/mnt/project` → a real directory), and `OPEN-QUESTIONS.md` refers
to "mounted paths" as an edge case for the Phase-12 scope slider.

The resolver as built is `resolve(root: &Path, virtual_path: &str)` — **one**
root, no mount table. So one of these is true:

- **Multi-root is intended**, the plural is correct, and the resolver's
  signature will have to change when mounts arrive. Phase 2+ builds on that
  signature, so knowing this before Phase 2 matters.
- **"Roots" is loose wording** for the single environment root, mounts are a
  separate later mechanism that will not touch `resolve`, and nothing needs to
  change.

**What I need:** which one. I did not guess, because narrowing a
security-relevant sentence is a decision, not a tidy-up.

Note the brief already says *"Do not build any mount mechanism"* for Phase 1, so
Phase 1 itself is correct either way. This is about the interface everything
after it inherits.

### Question 2 — where does Phase 1b live in the plan?

Non-existent-path resolution was split out of Phase 1 during the brief's review.
It is defined in exactly one place: `delegation-briefs/phase-1-path-resolution.md`
§8.4a. It does not appear in `BUILD-PLAN.md`, and Phase 2a routes every file
operation through the resolver.

**What I need:** does Phase 1b become its own numbered phase in BUILD-PLAN, or
does it fold into Phase 2a? I did not choose, because inserting a phase into the
plan is a sequencing decision and `BUILD-PLAN.md`'s rules make ordering
deliberate.

---

## 5. A risk I found that is not a collision

The two Hermes skills are **byte-identical copies** of two project documents:

| Skill | Source document | Check |
|---|---|---|
| `~/.hermes/skills/software-development/atrium/SKILL.md` | `AGENT-RULES.md` | `diff <(tail -n +10 <skill>) AGENT-RULES.md` |
| `~/.hermes/skills/software-development/muffin-style/SKILL.md` | `WORKING-WITH-MUFFIN.md` | `diff <(tail -n +11 <skill>) WORKING-WITH-MUFFIN.md` |

Both were in sync when checked. **Nothing keeps them in sync.** A subagent
loads the *skill*; you and I read the *document*. Edit one and the child follows
a stale copy of the rules, silently. Commands are now in `HANDOFF.md` §6.

Worth deciding whether these should be copies at all, or whether the skill
should just tell the child to read the file at its absolute path.

---

## 6. Verification state — what is proven and what is not

**Observed after every edit in this session:**

- `hermes verify` → `ok: true`; build exit 0, test exit 0.
- `cargo test` → 11/11 pass.
- `hand-test.sh` → 91 lines, 24 accept, 67 reject, zero FAIL, zero escapes.
  Every ACCEPT independently re-checked against the root prefix by the script.
- `probe-blind.sh` → zero escapes, zero hangs.
- `probe-parent.sh` → prefix-sibling and middle-component symlink probes held.
- Both skills re-diffed after the document edits: still identical.

**Not proven, and I am not claiming otherwise:**

- **Muffin's hands-on pass has not happened.** Phase 1 is still not signed off.
  Nothing in this sweep changes that — the gate is untouched and still open.
- **The three NUL-byte lines were never exercised by hand**, and cannot be. Only
  `cargo test` covers them.
- **The resolver's own logic was not re-reviewed.** I verified it behaves as
  required. I did not audit `lib.rs` line by line for a flaw the attack lists
  missed. The two lists and my own probes all held, which is evidence, not proof
  — no finite test proves a sandbox.
- **I never watched a hook fire.** The `transform_llm_output` finding from the
  previous turn is a source-reading result plus one live observation
  (`pre_llm_call` firing, proven from the session's own `api_content` sidecar).
  The TUI delivery path for a *transformed* reply is **inferred, not observed** —
  I said so then and repeat it here, because it is the one claim in this project
  that rests on reading code rather than watching behaviour.

---

## 7. One unrelated config note

Muffin ran `hermes config set display.sections.tools collapsed`. The CLI warned
the key is unrecognised. **The warning is wrong** — the key is real:

- `ui-tui/src/domain/details.ts:5` — `SECTION_NAMES = ['thinking', 'tools',
  'subagents', 'activity']`
- `details.ts:21` — *"Opt out of any of these with `display.sections.<name>` in
  config.yaml"*
- `details.ts:74` — `sectionMode()` resolves explicit override → global →
  built-in default.

Default for `tools` is `expanded`; the config now pins it `collapsed`, and the
value is read on config sync. So the setting works and the validator's
recognised-key list is simply out of date. Recorded here only because a
misleading warning is the same class of problem as a green check that is green
for the wrong reason.

---

## 8. What I did not do

- Did not change any design decision, any phase scope, or the resolver's logic.
- Did not delete any superseded entry — per `STATUS.md`'s own rule.
- Did not resolve either of the two questions in §4.
- Did not move any STATUS entry between "verified hands-on" and
  "agent-reported". That is Muffin's alone.
- Did not touch a single file outside `/home/muffin/Desktop/Nexus project/`
  except `~/.hermes/config.yaml` via the CLI command Muffin issued.
