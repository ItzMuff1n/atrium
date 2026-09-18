# Atrium — orchestrator handoff

**Written:** 11 Sep 2026, by the Claude session that coordinated this project.
That session has ended. **For:** the Hermes main model, taking over as sole
orchestrator.

**Superseded in part, 13 Sep 2026.** The Claude session came back and edited
project files directly between 12 and 13 Sep, so the "no outside reviewer"
statement below was briefly untrue in practice. **Muffin has now handed the
project back to the Hermes session as sole orchestrator, effective 13 Sep 2026.
There is again no outside reviewer.** The Claude edits made this session (the
rule-2 rewrite, the Phase 1b rulings in `DECISIONS.md`, `attack-list-1b.md` §M)
were verified by this session against the files themselves rather than accepted
on report — one earlier claim, that an `attack-list.md` §G note was "already
written", was false when made and had to be checked to be caught. Continue to
treat every third-party summary as a claim, not evidence.

Read this file, then read WORKING-WITH-MUFFIN.md, REASONING.md, DESIGN.md,
DECISIONS.md, BUILD-PLAN.md, AGENT-RULES.md, STATUS.md, OPEN-QUESTIONS.md,
attack-list.md, pipeline-check.md, and
delegation-briefs/phase-1-path-resolution.md.

- **WORKING-WITH-MUFFIN.md** — how to communicate with him, and what is yours
  to decide versus his.
- **REASONING.md** — why each design decision was made and what was rejected.
  Read it before proposing any change to DESIGN.md.
- **pipeline-check.md** — how delegation, skills and hooks actually behave in
  this install, observed rather than documented. §6 lists 15 ways this
  pipeline fails silently. Read it before trusting any delegation result.
- The rest are the project's source of truth.

This file is only the layer on top of them: decisions, reasoning and habits
that live in conversation and were never written down.

---

## 1. What changed about your role

You were the worker. You are now the orchestrator AND the worker: you plan,
you delegate to subagents, you verify their output, you report to Muffin.

**There is no outside reviewer.** The Claude session that coordinated this
project up to now has ended. Nothing external checks your briefs, your
subagents' output, or your own reasoning. Muffin relays nothing to anyone
else. You are the last check before his hands-on testing.

This changes what your reports have to be. Previously a report could be
imperfect because another model read it and caught the gaps. Now a gap in
your report is a gap in what anyone knows. Consequences:

- Never soften a failure into a partial success.
- When you are uncertain, say so as a headline, not a footnote.
- When you have claimed something is fixed twice and it has not held, stop
  fixing and say plainly that you are stuck. Muffin can act on "I am stuck".
  He cannot act on a third confident fix that fails.
- Volunteer the thing you are hoping he does not ask about.

**Phase 1 still needs independent verification, and it now comes from two
places instead of a reviewer:**

1. Muffin drives attack-list.md by hand through the resolve harness and
   watches each line. This is the primary verification and does not depend on
   any model's judgment — ACCEPT or REJECT is readable by anyone.
2. The blind second attack list (§4, "The independence replacement"). This
   was always required. With no outside reviewer it is now the ONLY
   independent check on the thinking, not just the code. Do not skip it, do
   not shorten it, and do not generate it yourself from a context that has
   already seen the resolver.

If Muffin hits a result he cannot interpret — a rejection whose reason is
vague, an accept he is unsure about — that is a stop-and-ask, and the answer
has to come from you in plain language with no jargon. Do not tell him to
check with anyone else. There is no one else.

---

## 2. Who Muffin is, operationally

- He does not read code and will not review it. Stated from day one, not a
  preference you can work around.
- He verifies by hand-testing your claims. He is good at this. He caught the
  Excalidraw browser-only limitation by running it.
- He decides **front-end and behaviour**: what Atrium does, how it looks, how
  it feels. Coding decisions are not his and should not be surfaced to him.
- Two coding things you DO surface, in plain language with no jargon:
  anything that changes what Atrium can do, and anything he will have to test
  by hand.
- He has no security background. Do not ask him to author security tests. The
  attack list was written for him for exactly this reason.
- He wants an objective advisor, not agreement. Say "no" and explain why when
  a request is flawed. He has pushed back on overstated risk before and was
  right to.
- Extremely concise answers. Front-load the conclusion. Never re-explain
  something he just told you.
- When explaining anything technical, drop the terminology or gloss it in a
  few plain words. He is self-taught and learns by doing. He will ask.

Full detail in WORKING-WITH-MUFFIN.md.

---

## 3. Current state — verified vs claimed

**Confirmed by Muffin hands-on or read from disk directly:**

- Flutter 3.47.3 at ~/flutter. Perf test passed on his hardware: 200 animated
  squares at 372fps/2.69ms, 1000 squares at ~155fps/6.4ms, target is 16.7ms.
  Native Wayland, Impeller on OpenGL ES. Zero rendering errors.
- Stack locked: Flutter frontend + Rust backend + local JSON socket bridge.
- Spikes 0a and 0b built and confirmed. **0c verified hands-on 13 Sep 2026 —
  Phase 0 is closed.**
- Excalidraw MCP canvas is browser-only — pixels require an open browser tab.
  It is a GENERIC surface in v1, not native.
- Hermes: model.default is deepseek-v4.1-flash, delegation.model is kimi-k3,
  both on ollama-cloud. Verified 11 Sep 2026 by live request and by reading
  sessions.model in state.db, not by reading config.yaml.
- The `atrium` skill exists at
  ~/.hermes/skills/software-development/atrium/SKILL.md, and a second skill
  `muffin-style` alongside it. **Since 12 Sep 2026 each body is a pointer to
  its source document, not a copy — see §6.** **Both are inherited as index
  entries only** — a name and a one-line description. Neither body loads unless
  someone calls `skill_view`.

**Believed verified, then disproved — the correction matters more than the
fact:**

- **The `atrium-check.sh` hook does not run. It never has.** An earlier
  version of this file listed it here as registered, allowlisted and working.
  That was wrong, and it was wrong because an agent report was filed as a
  verified fact without anyone watching the hook actually fire. The audit in
  pipeline-check.md §5 found three independent reasons it is a no-op: the TUI
  process never calls `register_from_config`, `post_tool_call` return values
  are discarded by the emitter, and `{"context": ...}` is the payload shape
  for a different event. `hermes hooks doctor` reports it healthy, because it
  checks the config and the script rather than the live registry.

  Treat this as the template for the failure it represents: a green check
  that is green for a reason other than the one you want.

**Not independently verified — treat as claims:**

- Anything in a previous agent's NOTES.md or self-report that is not in the
  list above.

**No Atrium code exists apart from the resolver.** The spikes are
throwaway. Phase 1 is built (`resolver/`, crate `atrium-resolver`) and verified
agent-side, and is **not signed off** — Muffin's hands-on pass is the
outstanding step. No Phase 2+ code exists. See §7 for the ordered next steps.

*(Superseded 13 Sep 2026. Both Phase 1 and Phase 1b were signed off hands-on by
Muffin that day, and `resolver/` now resolves paths that do not exist yet as
well as existing ones. Still true: no Phase 2 code exists. Live state is in
`STATUS.md`, "Current phase".)*

*(Corrected 12 Sep 2026: this line read "No Atrium code exists. Phase 1 has not
started." It was written before Phase 1 was dispatched on 11 Sep 2026 and never
updated, so it contradicted §7 of this same file and STATUS.md. The
contradiction is recorded here rather than silently overwritten — a document
that disagrees with itself is the failure mode this project is structured to
catch.)*

**Live config values live in STATUS.md and nowhere else.** The model id above
is a snapshot of a verification, not a second source of truth. On 11 Sep 2026
the same value appeared in three documents with three different answers, and
the wrong one sat in this section. When a live value changes, update STATUS.md
and reference it from elsewhere rather than restating it.

---

## 4. Decisions made in conversation, not in the docs

**Phase 1 relative paths.** Relative virtual paths are REJECTED with a reason,
never silently rooted. Strictness over convenience in the dangerous subsystem:
loosening later is easy, tightening later breaks every agent written against
it. Muffin accepted this.

**Phase 1b split.** Resolving paths that do not exist yet was pulled out of
Phase 1 into its own phase. Reason: it is the only part of the resolver that
reasons about something the filesystem cannot confirm, so it gets isolated
verification. If it shipped in the same batch and the attack list found a
hole, you would not know which half was wrong. Phase 2 needs it, so it is
postponed, not dropped.

**Attack list authorship.** BUILD-PLAN says the user writes the attack list.
He cannot — no security background — and should not have to. Claude wrote
attack-list.md instead.

**The independence replacement.** Claude's list does not satisfy BUILD-PLAN's
requirement, because Claude reviewed the brief and would have reviewed the
resolver, so it shares assumptions with the implementation. Before Phase 1 is
signed off, a second attack list must come from a subagent that has seen none
of: the resolver source, the Phase 1 brief, attack-list.md. Give it only a
plain description of what the sandbox guarantees and ask what paths it would
try. Run everything it finds that the existing list missed. **This is a gate,
not an extra**, and with no outside reviewer it is now the only independent
check on the reasoning.

**The Board is dropped from v1.** Muffin's decision, 11 Sep 2026. DESIGN
required it to auto-open on startup and no phase built it. Almost everything
it would show — tasks, pending approvals, blocked agents — does not exist
until Phase 10 and later. v1 opens to the desktop and the island. See
DESIGN §5.2 and §5.3.

**Model switched before Phase 1.** Main model is deepseek-v4.1-flash (DeepSeek
V4.1 Flash, released 10 Sep 2026, on ollama-cloud). That exact id is verified
working by live request; "deepseek-flash" is DeepSeek's own first-party API
name and returns 404 on ollama-cloud. Chosen over glm-5.3-flash: 74.2 vs 63.4
on DeepSWE, KV cache cut to roughly a quarter, which matters because agent
loops re-read the same context constantly. Switched before Phase 1 dispatch
rather than during it, so no work is split across two models. delegation.model
remains kimi-k3, deliberately, so that only one variable moved. Changing it to
deepseek-v4.1-flash is the more consequential swap, since that is the model
actually writing code, and it should be done on its own, after Phase 1, so the
effect is attributable.

Counter-argument recorded, because it may outweigh the upgrade: running main
and delegation on the same model costs you the separation that makes
verification meaningful. Shared training means shared blind spots — the main
model will nod at mistakes it would have made itself. That is the same
reasoning behind the blind second attack list. Keeping kimi-k3 preserves it
for free.

Note DeepSeek V4 Pro retires 14 Sep 2026; do not build around it.

---

## 5. Hard-won gotchas

### Delegation

**A delegation's `summary` is the child's LAST message, not its best one.**
Observed 11 Sep 2026: a child produced a complete, correct answer, then a
verification nudge fired, the child answered the nudge, and the nudge response
became the summary the parent received. The delegation was recorded
`status=completed`, `exit_reason=completed`, with no marker of any kind. The
real answer survived only in the live transcript and in state.db.

**Therefore: never accept a delegation summary as the child's answer.** Read
the transcript at `~/.hermes/cache/delegation/live/<delegation_id>/task-N.log`
— it is written live and can be tailed while the child runs. If the summary
and the transcript disagree, the transcript is the work. `output_schema` does
not protect against this; the schema validates the final message, which may be
the nudge response.

**`status=completed` means the child stopped, not that it succeeded.**

**A subagent gets skill names, not skill bodies, and none of your context.**
No memory, no user profile, no project context, no AGENT-RULES, no
WORKING-WITH-MUFFIN. It is a competent stranger with your tools and your
filesystem access. If you want it to follow the project rules, the briefing
text must tell it to call `skill_view` on `atrium` — it will not think to.

**A subagent cannot ask you anything.** `clarify` is stripped from every
child. Facing ambiguity it guesses or stalls, and the delegation still returns
completed with no record that it had a question.

**A subagent's dangerous commands are auto-denied silently.**
`subagent_auto_approve` defaults to false, because a worker thread cannot
reach the approval prompt. The child sees a refusal, the refusal is a WARNING
in agent.log, and if the child does not mention it you never learn its command
was blocked.

**Toolset restriction is not enforceable.** `delegate_task` has no `toolsets`
parameter in this version. Any constraint in the goal or context is a request
to a language model, not a boundary. The only real limits are the fixed
blocklist: `delegate_task`, `clarify`, `memory`, `send_message`, `cronjob`.

**Children run in this same process, on a thread, as the same user, with
identical filesystem write access.** There is no sandbox between you and a
subagent.

### Hooks

**The hook is dead — see §3.** Do not rely on it, and do not assume a compile
error will surface by itself. If you want `cargo check` run after an edit, run
it yourself.

**Every hook failure is swallowed.** `post_tool_call` fails open; errors go to
`logger.debug`, below the default log level. A hook can be absent, crash, time
out, or return garbage, and the write proceeds looking fine.

**Hooks register at startup only, in processes that register them at all.**

### Config and models

**Config edits.** Agent write/patch tools refuse ~/.hermes/config.yaml — a
built-in security guard. Use `hermes config set`. It strips comments from the
file. It does NOT reliably create a backup — a change on 11 Sep 2026 left
none, despite an earlier report claiming otherwise. Copy config.yaml yourself
before any change. A refused patch can appear in the file-mutation verifier as
a failure while the CLI path actually succeeded — check the file on disk
before reporting either way.

**Session model overrides.** config.yaml's model.default is NOT what a running
session uses. A /model switch persists per-session in state.db, and precedence
is session override > channel > global default. A session was observed running
glm-5.3-flash while config said glm-5.3. Check state.db, not the config file.
Note also that one session id carries across restarts and model changes, so
`session_model_usage` holds rows for several models — the question is only
answerable per-row, by latest `last_seen`.

**The local model cache lags the live listing.**
~/.hermes/models_dev_cache.json omitted deepseek-v4.1-flash while the live
ollama-cloud endpoint served it. A model id missing from the cache does not
mean it does not exist. Probe the endpoint with a real one-token request
before concluding anything about a model id — and note that `hermes config
set` accepts a model id without validating it, so a wrong value looks like
success until the first call fails.

### General

**Long output corrupts when copied out of Warp.** Terminal output copied from
Warp into a chat window arrives duplicated and truncated mid-word — reproduced
many times on 11 Sep 2026, and identified as a Warp copy bug, not a Hermes or
model fault. Nothing on this side can fix it. If a report is long, write_file
it to disk and give Muffin the path instead of printing it; a file read
directly off disk arrives intact.

**Long pastes can arrive truncated to a placeholder.** Observed 11 Sep 2026: an
instruction reached the session as a bracketed stub rather than its text; the
real content was recoverable from `~/.hermes/pastes/paste_<n>_<timestamp>.txt`.
This is the reverse of the Warp corruption above — that one damages what you
receive and looks damaged; this one damages what Muffin sends and looks like a
short instruction rather than a truncated one. If a message
reads as unusually vague, references something it does not contain, or looks
like a summary of an instruction rather than the instruction, check
`~/.hermes/pastes/` for the original before acting. Say plainly that you did.

**Archived MCP servers.** 14 reference servers (GitHub, Postgres, SQLite,
Slack, Google Drive and others) were archived May 2025 with no security
guarantees, and listicles still recommend them. Seven remain active:
Everything, Fetch, Filesystem, Git, Memory, Sequential Thinking, Time. Always
take the vendor-maintained version.

**`write_file`'s lint block is a formatter, not a build check.** It produces
rustfmt diffs. It is not `cargo check` and it is not a hook. A subagent
mistook it for one during the audit.

**rmcp's `auth_header` must NOT carry the `Bearer ` prefix.** *(Sep 2026)*
`StreamableHttpClientTransportConfig::auth_header` takes the bare token — rmcp
3.3.0 adds `Bearer ` itself, as its own doc comment states. Spike 0c passed
`format!("Bearer {}", token)`, producing `Bearer Bearer <token>` on the wire,
and GitHub answered HTTP 400 `bad request: Authorization header is badly
formatted`. Note the shape of that failure: 400 means the header is malformed,
401 means the credential was read and rejected. A 400 points at your own code,
not at the token — check the token is clean (length and character set, never
printing it), then read the library's own doc comment for the field rather than
assuming the conventional format.

**rmcp's HTTP transports have no TLS unless you ask for it.** *(Sep 2026)*
`transport-streamable-http-client-reqwest` enables the reqwest crate but no TLS
backend, so a crate with only that feature compiles clean, has no
`rustls`/`native-tls` anywhere in `Cargo.lock`, and fails every `https://`
request at the transport layer with an error that never mentions TLS
(`error sending request for url …`). Spike 0c shipped in exactly that state.
Add the `"reqwest"` feature (which pulls `reqwest?/rustls`) or
`reqwest-native-tls`. If a remote MCP call fails at the request layer, run
`curl` against the same URL: if curl succeeds the fault is local, and the first
thing to check is whether any TLS crate is in the dependency tree at all.

**Linter false positive.** The standalone file linter reports E0670 "async fn
not permitted in Rust 2015" on edition-2021 files because it ignores
Cargo.toml. `cargo build` is the arbiter.

**`hermes verify` assumes a server and invents port 8000.** *(Sep 2026)*
For any Rust crate it writes `.hermes/environment.json` with `start: cargo run`
and `port: null`, then polls `http://127.0.0.1:8000/` for readiness. `port:
null` does **not** mean "no server" — `_run_start_phase` does
`port = port_override or recipe.port or 8000`, so null becomes 8000. A CLI
crate therefore always fails the readiness phase with `Connection refused`,
even when build and test pass. Use `hermes verify --skip-start` for
command-line binaries; that is the sanctioned path and returns `ok: true`.
Phases 1, 2 and 5 all produce CLIs, so this will recur.

**Partially fixed 12 Sep 2026.** `resolver/.hermes/environment.json` was
written with `start: null` and `readinessPath: null`, which stops the bogus
poll for this crate — `hermes verify` now returns `ok: true` with no flags
needed. **This is a per-project workaround, not a fix to the tool:**
regenerating the recipe (`--save`) or letting a fresh clone auto-detect will
reintroduce `start: cargo run`. Every new CLI crate needs the same edit. The
`--skip-start` advice above still applies as the general rule.

**The project path contains a space** — "Nexus project". Quote it everywhere.

**Muffin uses ZSH**, not bash.

---

## 6. Working rules that produced good results

**Report observed, not intended.** Already in AGENT-RULES.md §3. It is there
because it was violated and caught.

**Distrust green.** The single structural finding of the pipeline audit:
`hermes hooks doctor` checks the config rather than the live registry; the
delegation `summary` looks like the answer and is the last message;
`status=completed` looks like success and means the child stopped;
`write_file`'s lint looks like a build check and is a formatter. Each is green
for a reason other than the one you want. Before trusting any check, ask what
it actually measures.

**Re-read before you act on a file.** Reading a file once and working from
that copy for the rest of a session is how you end up acting on something that
no longer exists. Files change under you — a subagent writes one, a hook
rewrites one, another session edits one. Nothing notifies you.

Before you edit a file, dispatch work based on its contents, or report its
state to Muffin, read it again. Not the whole folder — the specific file you
are about to rely on. If what you find differs from what you remembered, say
so plainly rather than quietly reconciling it; a difference you did not expect
is information.

**Read back after every edit.** Verify the change landed as intended. An edit
whose match spanned a line break once left a duplicated fragment in this very
file; reading back caught it. Grep for a count when the change is small enough
to count.

**Sweep for stale references after every edit.** A scoped edit changes a fact,
and the sentences elsewhere that described the old fact stay behind. This
happened repeatedly on 11 Sep 2026: renumbering the brief's §8.3 left two
cross-references pointing at the wrong item; adding a third harness mode left
the heading saying "Two modes"; approving the brief left STATUS.md still
telling Muffin to review it, in three separate places.

After any edit, grep the whole project for the thing you just changed — the
old number, the old wording, the old status — and report what still refers to
it. Flag rather than guess: proposing a fix is right, silently rewriting
something outside the requested scope is not.

**When you have claimed something is fixed twice and it has not held, stop
fixing.** Explain your exact reasoning verbatim instead, or take a harder
scenario, so the logic can be judged before another fix is trusted.

**Info-gathering and action are separate dispatches.** Label which one a
prompt is. Do not mix "report back" with "change something".

**Check the actual state on disk rather than inferring it from a previous
agent's report.** Every discrepancy found in this project came from doing
this, including the discovery that the hook this file once called verified had
never fired.

**Flag before building.** If a request smells off — scope mismatch, a cleaner
path exists, ambiguous intent — ask one sharp question before starting.

**Keep a running log.** With no second model holding context, the docs are the
only memory this project has. When a decision is made or a gotcha is found,
write it into the relevant file the same turn. STATUS.md is for current state;
DECISIONS.md is for anything settled and its reasoning; REASONING.md is for
why a design choice was made and what was rejected. A fact that exists only in
a chat window is a fact that will be lost.

**One fact, one home.** When the same value appears in several documents it
will eventually disagree with itself. Put it in the document that owns it and
reference it from the others.

**Two Hermes skills used to duplicate two project documents. Resolved 12 Sep
2026 — they are now pointers.** `AGENT-RULES.md` was the body of
`~/.hermes/skills/software-development/atrium/SKILL.md`, and
`WORKING-WITH-MUFFIN.md` was the body of
`~/.hermes/skills/software-development/muffin-style/SKILL.md`. Skills are what
subagents can load; the docs are what you read. **Editing one did not update the
other**, so a subagent loading the skill would follow a stale copy of the rules —
and it would drift silently, because nothing warned on divergence.

By decision (Muffin, 12 Sep 2026) **the copying is abolished.** Each SKILL.md now
keeps its frontmatter and its body is a single instruction: read the source
document at its absolute path and treat that file as the authority.

- `atrium` → `/home/muffin/Desktop/Nexus project/AGENT-RULES.md` (192 lines)
- `muffin-style` → `/home/muffin/Desktop/Nexus project/WORKING-WITH-MUFFIN.md` (351 lines)

**The old `diff` checks are dead.** They compared a copy against its source; there
is no copy to compare any more. Do not re-add one. If a skill's body ever grows
again beyond a pointer, that is the drift coming back — fix the skill, not the
document.

Reason it is safe: subagents inherit skill *names* only, so the body matters just
when something calls `skill_view` — at which point reading the real document is
strictly better than reading a frozen copy of it.

---

## 7. Immediate next steps

Order matters here, and it was corrected once already: BUILD-PLAN rule 2 used
to read as an absolute — no phase starts until the previous phase's
verification passed — so Phase 0 being unfinished appeared to block Phase 1.
*That rule was then overridden for Phase 1 on 11 Sep 2026 — see "Amended"
below.*

**Updated 13 Sep 2026: the override is now part of rule 2 itself.** BUILD-PLAN
rule 2 reads "no phase starts on an unverified foundation it actually depends
on", with the 0c case as its worked example and the narrow limits on when the
dependency clause applies. Read the rule there rather than reconstructing the
argument from this section. 0c blocks Phase 9; it does not block Phase 1b or
Phase 2.

**Amended 11 Sep 2026 — the parent (this session) overrode that and dispatched
Phase 1 anyway, on this reasoning:** 0c tests MCP remote transport. Phase 1 is
Rust-only with no MCP, no network, no transport. Nothing Phase 1 depends on is
settled by 0c, so holding it behind a deliberately-deferred credential was the
ordering rule tripping over itself, not safety. 0b already settled the Rust SDK
question, which is what the "spikes first" rationale (BUILD-PLAN, Sequencing
rationale) exists to answer. BUILD-PLAN rule 2 is about not building on an
*unverified foundation*; a spike that cannot run is not a foundation. Phase 1
now exists and is verified agent-side. **Phase 2 still waits for Phase 1 to be
signed off hands-on.**
*(Updated 13 Sep 2026: Phase 1 was signed off hands-on that day, and Phase 0
closed the same day. Phase 2 now waits on Phase 1b. Updated again later the same
day: Phase 1b's gate was passed, so nothing now waits — Phase 2 may start.)*

1. ~~Muffin generates a fine-grained GitHub PAT~~ — **done 13 Sep 2026. 0c
   passed hands-on and Phase 0 is closed.** 27 read-only tools listed,
   `get_me` returned, exit 0. One bug fixed on the way: the `Bearer ` prefix
   was being sent twice — see §5, "rmcp's `auth_header`". Details in STATUS.md,
   "Verified hands-on".
2. ~~0c passes. Phase 0 closes.~~ **Done.**
3. ~~Dispatch Phase 1~~ — **done 11 Sep 2026** (`deleg_7b0c9983`). Brief is
   `delegation-briefs/phase-1-path-resolution.md` §11; read the deviation note
   there first (the child was told to read the brief from disk, not have it
   pasted). The child was instructed to `skill_view` on `atrium` first, as
   required.
4. ~~Read the live transcript~~ — **done.** Child's transcript read; build,
   test and demo re-run by the parent; independent attack sweep and the §12
   blind list both run. Zero escapes. Two spec deviations found and recorded in
   STATUS.md.
5. ~~Muffin drives attack-list.md by hand through the resolve harness~~ —
   **done 13 Sep 2026. Phase 1 is SIGNED OFF.** He ran the 91-line list plus
   his own paths; everything held. Details in STATUS.md, "Verified hands-on".
   Kept for the record: `phase-1-hands-on.md` has the four steps; the list runs
   via `resolver/hand-test.sh`, which also checks independently that every
   ACCEPT lands inside the root; `probe-parent.sh` and `probe-blind.sh` add the
   parent's probes and the §12 blind list's novel lines. The section E /
   section I contradiction was fixed before the pass. The three NUL lines in
   section F cannot go through the command-line harness and are covered by the
   test suite only.
6. The blind second attack list — **done** 11 Sep 2026 (`deleg_c6716023`),
   zero escapes. Kept for the record; the brief's §12 gate is satisfied.
7. ~~Then, and only then, Phase 1 is signed off.~~ **Done 13 Sep 2026.**

**All seven steps above are complete. This section is history, not a to-do
list.** *(Recorded 13 Sep 2026: steps 5 and 7 were left live for a day after
they were finished, and this section kept being re-read as the current plan.)*

**Do not read the live plan out of this file.** It has been stale every time
anyone did. Live state is `STATUS.md`, "Current phase".

*(Updated 14 Sep 2026: the two paragraphs that used to sit here said "the live
next step is Phase 2" and "do not start Phase 2 until Phase 1b holds". Both became
wrong the same day — Phase 2a, 2b and 2c have since been built and signed off
hands-on by Muffin, and 2d is the next build. They are replaced rather than
appended to, because this section's whole failure mode is old ordering text
outliving its condition. The four crates that exist and are signed off:
`resolver/`, `fileops/`, `shell/`, `snapshot/`.)*

*(Updated 15 Sep 2026: **2d is built and awaits its gate**, so "2d is the next
build" is no longer current — the crate is `watcher/` and the gate is
`bash watcher/hand-test-2d.sh`; see `STATUS.md` "Agent-reported". **2e is the next
build** and the last gate before Phase 5. Five crates now exist: the four signed-off
ones above plus `watcher/`, built but **not yet signed off**.)*
