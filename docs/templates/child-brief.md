# Child brief — template

**Copy this file into the dispatch. Fill every field. Delete nothing.**

A child inherits the skill **index** but no skill bodies, and knows nothing about
this conversation. Everything it needs must be in the brief. A brief that says
"see `docs/TOPICS.md` §3" has told the child nothing.

A child under this routine is one of two things: a **blind attack** (adversarial
testing, per `TOPICS.md` §4) or **recon** (bounded information-gathering). It is
never a builder. The Lead writes product code.

---

## 1. Kind — pick exactly one

- [ ] **Info-gathering** — "report back, change nothing"
- [ ] **Action** — "change something"

**Never mix them.** A child given both does the action and skims the reporting.
A blind attack is info-gathering: it writes only under `/tmp` and reports.

---

## 2. Fields

| field | what goes here |
|---|---|
| `kind` | `info-gathering` or `action` |
| `goal` | one sentence, the deliverable |
| `scope_allow` | the exact paths the child may read, and (for `action`) write |
| `scope_deny` | the paths it must not touch, named explicitly |
| `rules` | the relevant rule text **pasted in full**, never a pointer to a file |
| `background` | everything it needs that it cannot look up: why this task exists, what was already tried, what a wrong answer looks like |
| `output_schema` | the JSON schema below, always |
| `evidence_required` | what counts as proof — commands and their raw output, not prose |
| `if_stuck` | what to do on a blocker: report it, do not invent a workaround |
| `forbidden` | never push, never open or merge a PR, never edit `main`, never touch another branch |

### `output_schema` — required in every brief

The proven field set. `what_was_not_done` and `what_is_uncertain` are the
load-bearing ones: they are what stops a fluent summary of half-finished work
from reading as a complete one.

```json
{
  "type": "object",
  "required": ["what_was_built", "files_changed", "commands_run_with_output",
               "what_was_not_done", "what_is_uncertain"],
  "properties": {
    "what_was_built":            {"type": "string"},
    "files_changed":             {"type": "array", "items": {"type": "string"}},
    "commands_run_with_output":  {"type": "array", "items": {"type": "object"}},
    "what_was_not_done":         {"type": "string"},
    "what_is_uncertain":         {"type": "string"}
  }
}
```

**Honest limit:** the schema is applied to the child's **final** answer. It does
not protect against the wrong-summary trap — a child that ran out of room still
writes a well-formed summary of unfinished work, and the schema just gives that
summary a shape. **The transcript is the evidence; the schema is not a substitute
for reading it.** Judge from `exit_reason` and the live transcript
(`~/.hermes/cache/delegation/live/<id>/task-N.log`), never from the summary.

---

## 3. Judging the child — not part of the brief

`status=completed` means the child **stopped**, not that it succeeded. Report
`exit_reason`. `max_iterations`, a timeout, or a failed API call is **FAILED**
whatever the summary says — report it as failed, name which of the three it was,
and split the task smaller.

---

## 4. Skeleton

```
kind: info-gathering
goal: <one sentence>
scope_allow: <paths>
scope_deny: <paths>
rules (pasted in full):
  <the rule text — not "see AGENT-RULES.md">
background:
  <why this exists; what was tried; what a wrong answer looks like>
evidence_required:
  <the commands whose raw output proves the claim>
if_stuck:
  Report the blocker and stop. Do not invent a workaround.
forbidden:
  Never push. Never open or merge a PR. Never edit main or any branch but your own.
output_schema: <the JSON above>
```
