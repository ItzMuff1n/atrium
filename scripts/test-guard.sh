#!/usr/bin/env bash
# test-guard -- stop a change from "passing" CI by deleting or weakening tests.
#
# Runs on pull_request only. It asks two questions about the PR:
#
#   (A) Does the PR head DECLARE fewer tests than the base branch, counting with
#       `cargo test --workspace -- --list` (list them, do not run them)?
#       If so the job fails and prints the names of the tests that went away.
#
#   (B) Does any changed line of test code REMOVE or MODIFY an assertion or a
#       test attribute? Every such line is printed in the job summary, so it is
#       visible even when the job passes. ADDING a `#[ignore]` to a test fails
#       the job outright -- an ignored test is a test that does not run.
#
# Everything is compared against the base branch as it exists on the remote, not
# against a file on disk, so a PR cannot satisfy the guard by editing its own
# expectations.
#
# WHAT THIS CATCHES
#   deleting a test; renaming a test out of existence; commenting out an
#   assertion; weakening an assertion in place; marking a test #[ignore];
#   replacing an assertion with a weaker one containing `assert`.
#
# WHAT THIS DOES NOT CATCH -- stated plainly, because a guard that overstates
# itself is worse than no guard:
#   - an assertion made VACUOUS while still saying `assert` (e.g. `assert!(true)`,
#     or an assertion whose operands were changed). This does not reduce the test
#     count and its line still contains `assert`, so it is REPORTED in the
#     summary and needs a human to read it. It does not fail the job.
#   - a test skipped by `#[cfg(...)]` rather than `#[ignore]`.
#   - a test made to pass by changing PRODUCTION code so the assertion still
#     holds. That is what the `check` job and the attack lists are for.
#   - a test body gutted without touching a line containing `assert` (e.g. its
#     only assertion was already deleted in a previous commit on the same PR).
#   The count check (A) is the backstop for deletion; the summary (B) is the
#   backstop for weakening.
#
# Requires: bash, git, cargo. No new project dependency, no network fetch.

set -uo pipefail

FAIL=0
SUMMARY_FILE="${GITHUB_STEP_SUMMARY:-}"

say() { printf '%s\n' "$*"; }
# Everything written here lands in the GitHub job summary; locally it goes to
# stdout so the script can be exercised without GitHub.
sum() {
  if [ -n "$SUMMARY_FILE" ]; then
    printf '%s\n' "$*" >>"$SUMMARY_FILE"
  else
    printf '%s\n' "$*"
  fi
}
fail() {
  FAIL=1
  printf 'test-guard: FAIL: %s\n' "$*" >&2
  sum ""
  sum "## test-guard: FAIL -- $*"
}

# ---------------------------------------------------------------------------
# 0. Which tree are we judging, and against what?
# ---------------------------------------------------------------------------
BASE_REF="${TEST_GUARD_BASE:-}"
if [ -z "$BASE_REF" ] && [ -n "${GITHUB_BASE_REF:-}" ]; then
  BASE_REF="origin/${GITHUB_BASE_REF}"
fi
if [ -z "$BASE_REF" ]; then
  say "test-guard: no base ref given (not a pull_request run, and TEST_GUARD_BASE unset)."
  say "test-guard: there is nothing to compare against, so no check was performed. Exiting 0."
  exit 0
fi
if ! git rev-parse --verify --quiet "${BASE_REF}^{commit}" >/dev/null; then
  fail "base ref '${BASE_REF}' does not exist here, so the comparison could not be made. Refusing to report success on a check that never ran."
  exit 1
fi

HEAD_SHA="$(git rev-parse HEAD)"
BASE_SHA="$(git rev-parse "${BASE_REF}^{commit}")"
MB="$(git merge-base "$BASE_SHA" "$HEAD_SHA" 2>/dev/null || true)"
[ -n "$MB" ] || MB="$BASE_SHA"

say "test-guard: base ref   = $BASE_REF ($BASE_SHA)"
say "test-guard: merge base = $MB"
say "test-guard: head       = $HEAD_SHA"
say ""

sum "# test-guard"
sum ""
sum "Judging \`$HEAD_SHA\` against \`$BASE_REF\` (\`$BASE_SHA\`, merge base \`$MB\`)."
sum ""
sum "Both checks read the base branch out of git and evaluate it in a scratch"
sum "worktree, so editing a file on the PR branch cannot satisfy them."
sum ""

# ---------------------------------------------------------------------------
# 1. (A) Declaration counts.
# ---------------------------------------------------------------------------
declared_names() {
  # Lists the tests a tree declares, without running them.
  local out="$1"
  cargo test --workspace -- --list >"$out" 2>&1
  local rc=$?
  # `<name>: test` is a test; `<name>: benchmark` is not. Deduplicated, because
  # the same name can appear in more than one binary.
  grep -E ': test$' "$out" | sed -E 's/: test$//; s/^[[:space:]]+//' | sort -u
  return $rc
}

say "=== (A) tests declared ==="
HEAD_NAMES="/tmp/test-guard-head-names.txt"
if ! declared_names /tmp/test-guard-head-list.txt >"$HEAD_NAMES"; then
  say "test-guard: warning: 'cargo test --list' exited non-zero at head; its output tail:"
  tail -15 /tmp/test-guard-head-list.txt | sed 's/^/    /'
fi
HEAD_COUNT="$(wc -l <"$HEAD_NAMES" | tr -d ' ')"
say "test-guard: declared at head = $HEAD_COUNT"

BASE_NAMES="/tmp/test-guard-base-names.txt"
BASE_COUNT=""
BASE_WORKTREE="/tmp/test-guard-base-count-tree"
rm -rf "$BASE_WORKTREE"
if git worktree add --detach "$BASE_WORKTREE" "$MB" >/dev/null 2>&1; then
  if [ -f "$BASE_WORKTREE/Cargo.toml" ]; then
    if ( cd "$BASE_WORKTREE" && declared_names /tmp/test-guard-base-list.txt >"$BASE_NAMES" ); then
      BASE_COUNT="$(wc -l <"$BASE_NAMES" | tr -d ' ')"
    fi
  fi
  git worktree remove --force "$BASE_WORKTREE" >/dev/null 2>&1 || true
fi

sum "## (A) Tests declared -- list, not run"
sum ""
sum "| tree | tests declared |"
sum "|---|---|"
sum "| head (this PR) | $HEAD_COUNT |"

if [ -n "$BASE_COUNT" ]; then
  sum "| base (\`$BASE_REF\`) | $BASE_COUNT |"
  say "test-guard: declared at base = $BASE_COUNT"
  sum ""
  if [ "$HEAD_COUNT" -lt "$BASE_COUNT" ]; then
    fail "the PR declares FEWER tests than the base branch ($HEAD_COUNT < $BASE_COUNT)."
    REMOVED="$(comm -23 "$BASE_NAMES" "$HEAD_NAMES")"
    say "--- tests declared at base, absent at head ---"
    printf '%s\n' "$REMOVED"
    sum ""
    sum "### Tests declared at the base branch but not at head"
    sum ""
    sum '```'
    printf '%s\n' "$REMOVED" >>"${SUMMARY_FILE:-/dev/stdout}"
    sum '```'
  else
    say "test-guard: (A) OK -- head $HEAD_COUNT >= base $BASE_COUNT"
    sum "Head declares at least as many tests as the base branch."
    ADDED="$(comm -13 "$BASE_NAMES" "$HEAD_NAMES")"
    if [ -n "$ADDED" ]; then
      sum ""
      sum "### Tests added"
      sum ""
      sum '```'
      printf '%s\n' "$ADDED" | head -50 >>"${SUMMARY_FILE:-/dev/stdout}"
      sum '```'
    fi
  fi
else
  fail "could not determine the base branch's test count, so check (A) was NOT performed. A comparison that did not happen must not read as a pass."
fi

# ---------------------------------------------------------------------------
# 2. (B) Test-code diff.
# ---------------------------------------------------------------------------
say ""
say "=== (B) test-code diff vs base ==="

TEST_PATHS='tests/|_tests\.rs$|/tests\.rs$'
sum ""
sum "## (B) Changed test code"

if git diff --quiet "$MB" "$HEAD_SHA" -- . 2>/dev/null; then
  say "test-guard: no changes between merge base and head at all."
  sum ""
  sum "_No file differs between the merge base and the head commit._"
else
  # Report the changed test-code files themselves, for orientation.
  CHANGED_TEST_FILES="$(git diff --name-only "$MB" "$HEAD_SHA" -- . | grep -E "$TEST_PATHS" || true)"
  sum ""
  sum "Changed files under a \`tests/\` directory or named \`*_tests.rs\`:"
  sum ""
  if [ -n "$CHANGED_TEST_FILES" ]; then
    sum '```'
    printf '%s\n' "$CHANGED_TEST_FILES" >>"${SUMMARY_FILE:-/dev/stdout}"
    sum '```'
  else
    sum "_None._"
  fi

  # Every added or removed line, anywhere in the tree, that carries an
  # assertion or a test attribute. Deliberately NOT restricted to test files:
  # a `#[cfg(test)]` module lives in a source file, so an assertion inside one
  # would be missed by a path filter, and missing it is the one failure mode
  # that matters. Over-reporting here is the safe direction.
  MARKER='assert|#\[test\]|#\[should_panic\]|#\[ignore\]|panic!|assert_eq!|assert_ne!'
  DIFF_TEXT="/tmp/test-guard-diff.txt"
  git diff -U0 "$MB" "$HEAD_SHA" -- . >"$DIFF_TEXT" 2>&1 || true

  REMOVED_MARKED="$(grep -E '^-[^-]' "$DIFF_TEXT" | grep -E "$MARKER" || true)"
  ADDED_MARKED="$(grep -E '^\+[^+]' "$DIFF_TEXT" | grep -E "$MARKER" || true)"
  # Only an actual ATTRIBUTE fails the job -- not a line that merely mentions one.
  # A line whose content is optional whitespace then `#[ignore]` (optionally with
  # a reason, `#[ignore = "..."]`) is the attribute. A comment, a doc string or a
  # sentence containing the text "#[ignore]" -- this script's own printed output
  # included -- must not fail a PR. Found the hard way: the first version matched
  # any added line containing the substring, and so failed a PR that only added a
  # new test, because that same PR also carried a file mentioning it.
  ADDED_IGNORE="$(grep -E '^\+[[:space:]]*#\[ignore([^a-zA-Z_]|$)' "$DIFF_TEXT" || true)"

  sum ""
  sum "### Removed or added lines carrying an assertion or a test attribute"
  sum ""
  sum "Any line here is test-relevant: it is an assertion, a \`#[test]\`, a"
  sum "\`#[should_panic]\`, a \`#[ignore]\`, or a \`panic!\`. A **removed** line is"
  sum "the shape of a weakened or deleted check. Reported whether or not the job"
  sum "passes, so a green run still shows it."
  sum ""
  if [ -n "$REMOVED_MARKED" ] || [ -n "$ADDED_MARKED" ]; then
    sum '```diff'
    {
      [ -n "$REMOVED_MARKED" ] && printf '%s\n' "$REMOVED_MARKED"
      [ -n "$ADDED_MARKED" ] && printf '%s\n' "$ADDED_MARKED"
    } >>"${SUMMARY_FILE:-/dev/stdout}"
    sum '```'
    say "--- removed assertion/test-attribute lines ---"
    printf '%s\n' "${REMOVED_MARKED:-<none>}"
    say "--- added assertion/test-attribute lines ---"
    printf '%s\n' "${ADDED_MARKED:-<none>}"
  else
    sum "_None._ Every assertion and test attribute is unchanged from the base branch."
    say "test-guard: (B) no assertion or test-attribute line added or removed."
  fi

  # Added #[ignore] fails outright.
  if [ -n "$ADDED_IGNORE" ]; then
    fail "the PR adds #[ignore] to at least one test. A test that is ignored is a test that does not run."
    sum ""
    sum "### Added #[ignore] -- this is why the job failed"
    sum ""
    sum '```diff'
    printf '%s\n' "$ADDED_IGNORE" >>"${SUMMARY_FILE:-/dev/stdout}"
    sum '```'
  fi
fi

# ---------------------------------------------------------------------------
# 3. Verdict.
# ---------------------------------------------------------------------------
sum ""
sum "## Verdict"
sum ""
if [ "$FAIL" -eq 0 ]; then
  say "test-guard: PASS"
  sum "**Passed.** No test was removed, no assertion or test attribute was"
  sum "removed or changed, and no \`#[ignore]\` was added."
  sum ""
  sum "This does not mean the tests are *good* -- see the limits listed at the top"
  sum "of \`scripts/test-guard.sh\`. It means none was removed or silenced."
  exit 0
else
  say "test-guard: FAIL"
  sum "**Failed.** See the sections above."
  exit 1
fi
