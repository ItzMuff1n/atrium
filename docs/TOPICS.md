# Topics — how the agent works

A **topic** is one unit of work with a start, a budget, and a finish. This file is
the routine. It replaces `QUEUE.md`, which described a queue with no plan.

## 1. Plan first

A topic starts as a **plan issue** plus a **milestone** of the same name. The plan
carries: the **goal** in one paragraph; the **tasks**, one child issue each, in
order; the **files and directories in scope**; what **done** means; and a **token
budget** (§7).

Post the plan, then **STOP**. Muffin approves it in chat. **Never start an
unapproved topic.** The plan issue is the **source of truth after compaction** —
re-read it at the start of every task, not from memory.

## 2. Lead

The agent is the **Lead**. The Lead picks tasks, reads results, writes the
product code, pushes, opens PRs, and merges.

**The Lead writes the product code itself.** Not a child, not a delegation.
In pieces of roughly 200 lines or less, building after each (`AGENT-RULES.md`
§6, "Build in pieces... and build after each"). The Lead also writes the tests
that cover it, in the same commit.

This replaced the old rule that the Lead must not write product code. The change
was made on evidence, not preference — **topic #38, 19 Sep 2026**, measured from
`session_model_usage`:

- both builder children died at `max_iterations` (2.75M and 3.07M tokens,
  **5.82M together**) for **one commit**;
- one of the two reported killing a process that did not exist — a fluent,
  confident summary of work it had not done;
- the Lead finished the task alone.

The lesson is not that children are bad. It is that **a child which cannot bank
partial progress does not get more done with a bigger budget — it fails for
longer, and burns proportionally more tokens per failure.** Delegating a build
buys a second context and pays for it in coordination, and the builder child paid
for it in iterations. So the builder child is gone.

**What this changes about the Lead's job.** The Lead is now the only writer, so
there is no second agent's summary standing between a bug and the merge — and no
summary to blame when one lands. Every claim about product code is the Lead's own
and must name the command and its result (`AGENT-RULES.md` §6).

## 3. Children

A child is a `delegate_task` call for a **bounded, report-producing job** — never
a builder. Two kinds:

- a **blind attack** (§4) — adversarial testing against the rules, writing only
  under `/tmp`;
- **recon** — bounded information-gathering: "read these files, report what is
  actually there, change nothing."

**Every child brief is assembled from `docs/templates/child-brief.md` and carries
an `output_schema`.** The template is the field list; do not brief from memory.
Fields are filled in full — the rules are **pasted into the brief, never pointed
at**, because a child inherits the skill index but no skill bodies and cannot
look up a file path it was merely told about.

A child may read only what the brief allows, and may **not** push, open PRs,
merge, or touch any branch. The Lead does all of that.

**Judge a child from its live transcript**
(`~/.hermes/cache/delegation/live/<id>/task-N.log`), never from its summary: a
summary is a self-report.

**Its result is judged by that transcript and by its `exit_reason` — `status`
alone means nothing.** `status=completed` means the child **stopped**, not that it
succeeded. `exit_reason` of `max_iterations`, a timeout, or a last call that
failed on an API error is **FAILED**: report it as failed, name which of the
three it was, and split the task smaller. The `output_schema` gives the summary a
shape but does not make it true — a child that ran out of room still returns a
fluent, well-formed summary of half-finished work, and believing it is how
unfinished work gets merged under a green tick. A partial result is not a partial
success.

**Children are for reports.** If a task needs product code written, the Lead
writes it (§2) — that is not a child's job and has not been since 24 Sep 2026.

## 4. Blind attacks

For sandbox, lock and gate tasks, use **blind-attack children**: they see only the
rules, never the code, and write only under `/tmp`. These may run in parallel
**with each other** — never alongside another child, and there is no builder to
run them alongside since 24 Sep 2026 (§2).

## 5. Order

Tasks that do not need Muffin go first. Tasks needing his manual input are
**deferred to the end**. Never build a workaround to avoid asking him. Stop and ask
immediately only if nothing else can proceed.

## 6. Parking

Comment the reason, add the **`parked`** label, and **continue** when a task: needs
files outside scope, fails CI **three** times, or needs a messy workaround. The
label is what makes parking mechanical rather than prose — the goal gate (§11)
reads it. A parked task is not a finished task, and must appear in the closing
issue (§9).

## 7. Spend

Between tasks, compute the topic's spend as the **sum of every token column in
`session_model_usage`**, for the Lead session plus every child session spawned
during the topic. **Record the child session ids as you go** — without them the sum
is wrong and silently incomplete. **Stop when the spend passes the plan's budget.**

**Read-only audit or verification work that Muffin asks for after the topic's
tasks have stopped does not count against that topic's budget** — the budget
measures what it cost to *do* the topic, and work ordered afterwards to check it
is a separate thing that would otherwise make every closed topic's figure drift
upwards. Record such spend separately and name it as audit rather than folding it
into the topic total.

## 8. `needs-muffin`

`needs-muffin` means **only** two things: a **hand-test**, or a **product decision**
about how Atrium behaves, looks or feels. It is not a parking label (§6), not a
"this was hard" label, and not a way to hand back work the Lead should have done.

## 9. End of a topic

Open **one** issue labelled `needs-muffin`, titled **`Your turn: <topic>`**, listing:
the **hand-tests** with exact steps; the **decisions** needed with their options;
and the **parked items** with why each was parked. In chat, reply with **five lines
maximum** — detail goes in the issue.

## 10. Close

Act on Muffin's report. Record sign-offs in `docs/STATUS.md` — **his move, never
ours**. Then close the milestone and the plan issue.
