# Atrium — Rules for the coding agent

Read this before every session. Read it again if the session has been long.

---

## 1. Who you are working with

The person you are working with **does not read code and will not review it**.
This project is built entirely by AI agents.

That fact changes what your job is. You are not writing code for a reviewer to
check. **You are the only thing standing between a bug and production**, and the
compiler is the only other reviewer this codebase gets.

Consequences you must internalise:

- "It should work" is not a status. Run it.
- A test you wrote is weak evidence. It shares the assumptions of the code you
  wrote, including the wrong ones.
- Do not describe what you intended. Describe what you observed.
- If you did not run it, say you did not run it.

---

## 2. The documents

| File | What it is |
|---|---|
| `DESIGN.md` | The settled design. Source of truth for behaviour. |
| `DECISIONS.md` | What was rejected and why. Read before proposing changes. |
| `BUILD-PLAN.md` | Phases, in order, with verification gates. |
| `STATUS.md` | What is verified vs merely claimed. **You may only append.** |
| `OPEN-QUESTIONS.md` | Genuinely undecided items. |
| `AGENT-RULES.md` | This file. |
| `TOPICS.md` | The topic routine: plan first, the Lead/builder split, spend, parking, close. |

**If code and `DESIGN.md` disagree, stop and say so.** Do not silently
"fix" either one. One of them is wrong and the user decides which.

**If you want to do something `DECISIONS.md` rejected**, you may — but say
explicitly which rejection you are overturning and what new evidence justifies
it. Silently re-introducing a rejected approach is the single most damaging
thing you can do to this project, because nobody will read the code and notice.

---

## 3. Reporting

**Report what happened, not what you meant to happen.**

Every report separates:

- **Observed** — you ran it and saw this. Include the actual output.
- **Changed** — files you created or modified, and what changed in them.
- **Not done** — anything you skipped, stubbed, or left incomplete.
- **Uncertain** — anything you are guessing about.

Never write "fixed", "works", or "done" for something you did not execute.

When you hit an error, **paste the error verbatim**. Do not summarise it. Do not
paraphrase it. The user relays your output to another model that needs the exact
text.

If you claim a phase is complete, list exactly what the user should do to verify
it themselves. Do not verify it for them.

---

## 4. Scope discipline

**Do only the phase you were asked to do.**

`BUILD-PLAN.md` is ordered deliberately. The rationale for the order is at the
bottom of that file. Building ahead is not helpfulness — it creates code that
depends on foundations that have not been verified yet, and that is exactly the
failure mode this project is structured to avoid.

If you notice something a later phase needs, **write it down and say so**. Do
not build it.

If the phase seems too small, it is the right size. Small verified steps are the
point.

---

## 5. Stop and ask

**Stop and ask the user** when any of these are true:

- The design document does not cover the case you have hit.
- Following the design would produce something that clearly does not work.
- You need to change a decision recorded in `DECISIONS.md`.
- You are about to touch the sandbox path resolver (`DESIGN.md` §3.2) for any
  reason other than the phase that builds it.
- You are about to add a dependency.
- You would need to weaken a gate rule, a permission, or a lock to make
  something work.
- You have tried the same fix twice and it has not held.

**Asking is cheap. A wrong assumption in a codebase nobody reads is not.**

---

## 6. Hard rules

These are not preferences.

**The sandbox path resolver is the only way to turn a virtual path into a real
one.** No other code constructs a real path. Not for convenience, not for a
special case, not temporarily.

**Never write code that reaches outside the environment root.** Host access does
not exist in v1. If a task seems to require it, the task is wrong — stop and ask.

**Never disable, bypass, or add an exception to a gate rule to make something
work.** If a rule blocks legitimate work, that is a finding to report, not an
obstacle to route around.

**Never auto-apply an effect classification.** Pending means pending.

**The effect vocabulary is defined in exactly one place in code.** Do not spell
effect names out elsewhere.

**No core file is edited to add a surface.** If adding a surface requires
touching the core, the plugin architecture is broken — report it.

**Do not add a permission, a state, or an effect kind without saying so
explicitly.** These are small fixed vocabularies by design.

**"Done" means the checks passed — not that you said they did.** A task is done
only when `scripts/check.sh` passes locally **and** the CI run on GitHub is green
for that commit. Both, for that commit: a green run from an earlier commit is not
evidence about this one. **A self-report is never evidence of done** — not your own
summary, not a tick you did not read, not a run you are recalling rather than
quoting. If either check has not been run against the commit in question, the
answer to "is it done" is no, and that is the whole answer. This is the automatic
gate. It does not replace §10: `check.sh` and CI say the code passes, and only
Muffin's end-of-phase run says a phase is signed off.

**Nothing is pushed to `main` directly. Every change goes through a branch and a
pull request.** `main` is protected by a repository ruleset that refuses direct
pushes, force pushes and deletions, and will not merge a pull request until the
required check is green. The workflow is: cut a branch, commit on it, push it,
open a pull request, wait for `gh pr checks --watch` to report green, then merge
with squash and delete the branch. **A direct `git push origin main` will be
refused by GitHub** — if it is not, the protection has been lost and that is a
finding to report, not a thing to work around. This rule is the mechanical form of
the rule above: the branch-and-PR flow is what makes "both checks green for that
commit" something GitHub enforces rather than something you remember to do.

**Reporting a delegation means reading `exit_reason`, never `status` alone.**
`status=completed` means the child **stopped** — not that it succeeded. An
`exit_reason` of `max_iterations`, a timeout, or an API error (a 429, a rate
limit, any failed call) means **FAILED**, whatever the summary says. A child that
ran out of room returns a fluent, well-formed summary of half-finished work, and
that summary is a self-report like any other. Report it as failed, name which of
the three it was, and **split the task smaller** — do not treat what it produced
as a result to build on.

**The same applies to your own turn.** If you are cut off, hit an iteration
limit, or fail on an API error, the next session's first report says so, at the
top, in those words. An interruption that goes unmentioned reads as a clean
finish, and whoever picks it up has no way to tell.

**A claim of done names the evidence: the command and its result.** "It
compiles" is never evidence that it works. Neither is "the tests pass" without
the run that printed it. Quote the command, quote what it returned. If you did
not run it, say you did not run it.

**Build in pieces of roughly 200 lines or less, and build after each.** Never
write a large file in one shot and then debug it — errors a build would have
caught immediately instead surface one at a time, each fix obscuring the next.
A file written whole and repaired afterwards costs many turns to reach the state
that building after every piece reaches in a few.

**Tests and the code they cover go in the same commit.** The pre-commit hook runs
`scripts/check.sh`, so a commit whose tests do not yet match its code fails and
is refused. Writing both together is not tidiness — it is what keeps every commit
in the history green on its own.

---

## 7. Code that must be treated as dangerous

Two subsystems where "seems to work" is not good enough, and where you should be
slower and more paranoid than feels necessary:

**Path resolution (`DESIGN.md` §3.2).** Path traversal is a known bug class with
known tricks — symlinks, canonicalisation order, unicode separators, null bytes,
trailing dots. Resolve fully, then check. Never check, then resolve.

**The lock manager (`DESIGN.md` §10).** Concurrency bugs are invisible until they
are catastrophic, and they do not reproduce reliably. Prefer the boring, obvious
implementation over the clever one.

For both: **if you find yourself writing something clever, stop.** Clever is how
these fail.

---

## 8. When something is going wrong

If you have tried the same fix twice and it has not held, **stop fixing and start
explaining.** Write out, in detail:

- exactly what you did
- exactly what you observed
- what you believe is happening and why
- what you are uncertain about

The user will relay that to another model for a second opinion. That is faster
and cheaper than a third attempt.

**Do not oscillate.** If a fix for problem A causes problem B, and the fix for B
brings back A, you have misunderstood the underlying cause. Say so instead of
alternating.

---

## 9. Session start

At the start of a session:

1. Read `STATUS.md`. It tells you where the project actually is.
2. **Trust the "verified hands-on" section. Treat the "agent-reported" section
   as unconfirmed** — including entries you wrote yourself in a previous
   session.
3. Read the phase you are on in `BUILD-PLAN.md`.
4. Do not assume the codebase matches what a previous session's report claimed.
   Check.

## 10. Session end

Append to `STATUS.md`:

- What you did.
- What you actually ran, and its output.
- What the user needs to verify by hand.
- Anything you left incomplete.

**Everything you write goes in the "agent-reported, unverified" section.** Only
the user moves an item to "verified hands-on". Never move an entry yourself,
including your own.

### When to ask the user to run something

**Do not ask the user to run a command you can run yourself.** Run it, and report
what it printed. This is Muffin's rule, given 14 Sep 2026:

> "just dont ask me to run commands you can yourself, only ask me whenever youre
> done with a phase"

So the only hands-on request is **the end of a phase** — the verification the phase
is signed off by. Nothing in between. Mid-phase checks, builds, tests, harness runs,
probes and re-runs are the agent's own work and must be done without asking.

**Changed 25 Sep 2026, for scripted gates.** A scripted hand-test is no longer run by
Muffin at all. It runs in CI (`.github/workflows/hand-tests.yml`, on every PR and every
push, failing if any script exits non-zero), and **sign-off for a scripted phase is his
approval of the evidence** — the quoted output plus that CI run — **not his re-run.**
His reasoning, which is the whole argument: a re-run of a deterministic script adds
nothing, because the agent has already run it. His hands-on pass returns where it is the
only instrument — **visible behaviour, from Phase 6 (the first UI) onward.**

**What "evidence" must contain**, so it can actually be approved rather than taken on
trust: the exact command, the script's own final line quoted verbatim, the CI run that
carries it, and — where the harness has independent checks (the outside directory, host
`/etc/passwd`, real-path leaks) — the numbers those produced.

**Phase 2d is the last hand-run** (Muffin ran it, 25 Sep 2026). Entries in that section
from the next scripted phase on record his **approval of the evidence** and must say so
in as many words, so a later session cannot read an approval as a re-run.

**What his pass was for, and still is where it applies.** For a scripted gate,
mechanically it added little: the same script on the same binary is deterministic, and
the agent had already run it. But it was never a second *designer* — the agent writes
both the code and the harness that tests it, so a misunderstanding is encoded in both
and passes both; his run reproduced the agent's own assumptions exactly. The thing that
tests the shared assumption is the **blind attack list** (`TOPICS.md` §4), and that is
where the independent check lives. The agent's own harness is not the authority gate for
a scripted phase — the blind list and CI are.

**Corollary:** the agent must be able to say it has already run the gate itself, and
must not present an unrun command as a request he has to satisfy.
