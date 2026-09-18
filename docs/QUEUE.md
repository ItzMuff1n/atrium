# Working the queue

Triggered by **"work the queue"**. Order: `p1`, then `p2`, then `p3`. Within one
priority, **fuzz before mutation**: a fuzz crash is a real bug in the product, a
missed mutant is a hole in the tests.

**Skip anything labelled `needs-muffin`** — those need a hand-test or a product
decision, which is not the agent's to make. Comment what is needed and move on.

## One issue, one PR

- One issue = one branch = one PR. The PR body says `Fixes #N`.
- Merge only on green (branch → PR → checks green → squash-merge); never push a
  queue fix straight to `main`.
- **Never close an issue without a merged PR or a comment explaining why.**

## Mutation issues

A missed mutant means no test noticed a change, so the fix is to add or
strengthen **tests**. Never change the source to kill a mutant — that inverts
the tool, which exists to find untested behaviour, not to be satisfied.

If a mutant looks unkillable (the mutated code is equivalent) or the code is
dead, do not force a test: comment the reasoning and label it `needs-muffin`,
because deleting behaviour is a product decision.

## Stop when

- the queue is empty, or five issues are done in this session; or
- a standing stop rule fires (dangerous code, a design disagreement, a rule in
  `AGENT-RULES.md`); or
- one issue fails CI **three** times — comment what was tried, label it
  `needs-muffin`, move to the next.

## End of session

Write `/home/muffin/VibeCodeProjects/atrium-queue-log/<date>-<n>.md`: each issue
worked, its PR, the result, and anything left `needs-muffin` with the reason.
`<n>` increments if a log exists for that date. In chat: five lines max.
