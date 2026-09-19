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

The agent is the **Lead**. The Lead picks tasks, briefs children, reads results,
pushes, opens PRs, and merges. **The Lead does not write product code.**

## 3. Builders

A builder is a `delegate_task` child. **One at a time — never in parallel.**

Each brief carries: the **full text of the issue**, the relevant rules, the file
paths, and the branch name. A child may edit, `cargo test`, and `git commit` **on
its own branch only**. It may **not** push, open PRs, merge, or touch other
branches — the Lead does all of that.

**Judge a child from its live transcript**
(`~/.hermes/cache/delegation/live/<id>/task-N.log`), never from its summary: a
summary is a self-report.

**Its result is judged by that transcript and by its `exit_reason` — `status`
alone means nothing.** `status=completed` means the child **stopped**, not that it
succeeded. `exit_reason` of `max_iterations`, a timeout, or a last call that
failed on an API error is **FAILED**: split the task smaller and retry, even when
the summary reads as complete and confident. A child that ran out of room writes
a fluent summary of half-finished work, and believing it is how unfinished work
gets merged under a green tick. A partial result is not a partial success.

## 4. Blind attacks

For sandbox, lock and gate tasks, use **blind-attack children**: they see only the
rules, never the code, and write only under `/tmp`. These may run in parallel
**with each other** — never alongside a builder.

## 5. Order

Tasks that do not need Muffin go first. Tasks needing his manual input are
**deferred to the end**. Never build a workaround to avoid asking him. Stop and ask
immediately only if nothing else can proceed.

## 6. Parking

Comment the reason and **continue** when a task: needs files outside scope, fails
CI **three** times, or needs a messy workaround. A parked task is not a finished
task, and must appear in the closing issue (§9).

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
