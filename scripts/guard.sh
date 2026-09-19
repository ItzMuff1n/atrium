#!/usr/bin/env bash
# guard.sh -- stop a change from "passing" CI by weakening the things that catch it.
#
# Runs on pull_request only. Every check compares the PR head against the BASE
# BRANCH (the merge base), read out of git -- never against a file on disk -- so a
# PR cannot satisfy any of them by editing its own expectations.
#
# ## What it checks
#
#   (A) Test counts. Does the PR head DECLARE fewer tests than the base branch?
#       Counted with `cargo test --workspace -- --list` (list them, do not run).
#   (B) Rust test code. Does any changed line of test code REMOVE or MODIFY an
#       assertion or a test attribute? Adding `#[ignore]` fails outright.
#
#   Then the gates -- the things that catch the agent. Each of these fails the
#   job and names exactly what changed:
#
#   (a) HAND-TEST SCRIPTS (crates/*/hand-test-*.sh). A check line, an
#       expected-result line or an expected total that is removed or weakened, or
#       a script whose case count drops.
#   (b) THE FUZZ ORACLE (fuzz/fuzz_targets/). An assertion or a panic-on-escape
#       that is removed or loosened.
#   (c) MUTATION EXCLUSIONS (.cargo/mutants.toml). An exclusion added without a
#       comment on an adjacent line giving a reason.
#   (d) WORKFLOWS (.github/workflows/). A deleted step, an added
#       `continue-on-error` / always-true exit, or a loosened timeout or retry.
#   (e) THE GATES THEMSELVES (scripts/check.sh, scripts/guard.sh). A removed step.
#
# ## The one escape hatch
#
#   These things may still change legitimately, so there is exactly one way to
#   say so: a line in the PR body reading
#
#       GUARD-OVERRIDE: <category>: <reason>
#
#   where <category> is one of  hand-tests  fuzz-oracle  mutants-toml
#   workflows  gates  and <reason> is at least 20 characters. That waives that
#   ONE category: the job passes for it, the reason is printed in the summary,
#   and the summary says plainly which protection was waived.
#
#   A single override line waives a single category -- it can never waive
#   everything. A `GUARD-OVERRIDE:` line naming no recognised category waives
#   NOTHING and is reported as malformed: failing closed, because the dangerous
#   reading of a typo is that it silently disabled the guard.
#
# ## WHAT THIS DOES NOT CATCH -- stated plainly, because a guard that overstates
# ## itself is worse than no guard
#
#   - an assertion made VACUOUS while still saying `assert` (e.g. `assert!(true)`,
#     or an assertion whose operands were changed). It does not reduce the test
#     count and its line still contains `assert`, so it is REPORTED in the
#     summary and needs a human to read it. It does not fail the job.
#   - a test skipped by `#[cfg(...)]` rather than `#[ignore]`.
#   - a test made to pass by changing PRODUCTION code so the assertion still
#     holds. That is what the `check` job and the attack lists are for.
#   - a test body gutted without touching a line containing `assert`.
#   - a hand-test case whose call spans more than one line (all six scripts call
#     their case helpers on a single line today; a multi-line call would be
#     counted as neither added nor removed).
#   - a comment added next to a mutants.toml exclusion that gives a reason in
#     form only. The guard checks that a reason is PRESENT and non-trivial, not
#     that it is true.
#   - semantic equivalence the guard cannot see: it compares text, not behaviour.
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

# ---------------------------------------------------------------------------
# Overrides. Parsed once, from the PR body.
# ---------------------------------------------------------------------------
CATEGORIES="hand-tests fuzz-oracle mutants-toml workflows gates"
declare -A OVERRIDES=()

pr_body() {
  # 1. an explicit file (local testing), 2. the Actions event payload, 3. gh.
  if [ -n "${GUARD_BODY_FILE:-}" ] && [ -f "${GUARD_BODY_FILE}" ]; then
    cat "$GUARD_BODY_FILE"; return 0
  fi
  if [ -n "${GITHUB_EVENT_PATH:-}" ] && [ -f "${GITHUB_EVENT_PATH}" ]; then
    python3 -c 'import json,sys;d=json.load(open(sys.argv[1]));print((d.get("pull_request") or {}).get("body") or "")' \
      "$GITHUB_EVENT_PATH" 2>/dev/null && return 0
  fi
  if [ -n "${PR_NUMBER:-}" ] && command -v gh >/dev/null 2>&1; then
    gh pr view "$PR_NUMBER" --json body --jq '.body // ""' 2>/dev/null && return 0
  fi
  return 1
}

parse_overrides() {
  local body line cat reason infence=0
  body="$(pr_body || true)"
  [ -n "$body" ] || return 0
  while IFS= read -r line; do
    # Documentation must not read as a use of the hatch. Three cheap rules, each
    # added because something real tripped over it:
    #
    #   1. Skip fenced code blocks -- that is where the syntax gets documented.
    #   2. The token must start at COLUMN 0. A real override is a plain
    #      paragraph line; an indented example (a markdown code block) is not.
    #   3. A placeholder in the category slot (`<category>`) is documentation,
    #      not a mistake, so it is skipped silently rather than reported
    #      malformed. A real typo (`handtest`) still gets reported.
    #
    # Rule 1 and 2 came from PR #41's own body, which documents the hatch and was
    # reported as two malformed overrides before this.
    case "$line" in
      '```'*) [ "$infence" -eq 0 ] && infence=1 || infence=0; continue ;;
    esac
    [ "$infence" -eq 0 ] || continue

    case "$line" in
      GUARD-OVERRIDE:*) : ;;
      *) continue ;;
    esac
    line="${line#GUARD-OVERRIDE:}"
    line="$(printf '%s' "$line" | sed 's/^[[:space:]]*//; s/[[:space:]]*$//')"
    cat_name="${line%%:*}"
    cat_name="$(printf '%s' "$cat_name" | tr -d '[:space:]')"
    reason=""
    case "$line" in
      *:*) reason="$(printf '%s' "${line#*:}" | sed 's/^[[:space:]]*//; s/[[:space:]]*$//')" ;;
    esac
    # An angle-bracket placeholder is documentation, not a malformed override.
    case "$cat_name" in
      *'<'*|*'>'*) continue ;;
    esac
    if ! printf '%s' " $CATEGORIES " | grep -q " ${cat_name} "; then
      # Fail closed: a malformed override waives nothing, and says so.
      say "guard: MALFORMED OVERRIDE -- 'GUARD-OVERRIDE: ${line}' names no known category."
      say "guard:   known categories: $CATEGORIES"
      say "guard:   it waives NOTHING. Failing closed rather than guessing what it meant."
      MALFORMED_OVERRIDE=1
      continue
    fi
    if [ "${#reason}" -lt 20 ]; then
      say "guard: OVERRIDE TOO SHORT for '${cat_name}' -- the reason must be at least 20 characters, got ${#reason}."
      say "guard:   it waives NOTHING."
      MALFORMED_OVERRIDE=1
      continue
    fi
    OVERRIDES["$cat_name"]="$reason"
  done <<<"$body"
}

# Is CATEGORY overridden? Prints the reason on stdout if so.
override_for() { printf '%s' "${OVERRIDES[$1]:-}"; }

# guard_fail <category> <message...>
# Fails the job unless that one category carries a valid override.
guard_fail() {
  local cat="$1"; shift
  local reason
  reason="$(override_for "$cat")"
  if [ -n "$reason" ]; then
    WAIVED+=("$cat|$reason")
    say "guard: WAIVED (${cat}) -- $(printf '%s' "$*")"
    sum ""
    sum "### WAIVED by GUARD-OVERRIDE -- category \`$cat\`"
    sum ""
    sum "The following would have failed this check and was waived by a line in the PR body:"
    sum ""
    sum '```'
    sum "$(printf '%s' "$*")"
    sum '```'
    sum ""
    sum "**Reason given:** $reason"
    sum ""
    sum "> This protection is NOT active for this pull request. Whoever reviews it"
    sum "> is the only thing standing where \`$cat\` would have been."
    return 0
  fi
  FAIL=1
  printf 'guard: FAIL (%s): %s\n' "$cat" "$*" >&2
  sum ""
  sum "## guard: FAIL -- $*"
  sum ""
  sum "_Category: \`$cat\`. If this change is legitimate, add a line to the PR body"
  sum "reading \`GUARD-OVERRIDE: $cat: <reason>\` (reason at least 20 characters)._"
  sum ""
}

MALFORMED_OVERRIDE=0
declare -a WAIVED=()
parse_overrides

# hard_fail <message...>
# For the pre-existing Rust test checks (A) and (B). These have no override:
# the escape hatch exists for the gates added on top, not for the original
# "this PR deletes tests" rule, which has never had an exception and should not
# gain one.
hard_fail() {
  FAIL=1
  printf 'guard: FAIL: %s\n' "$*" >&2
  sum ""
  sum "## guard: FAIL -- $*"
  sum ""
}

# ---------------------------------------------------------------------------
# 0. Which tree are we judging, and against what?
# ---------------------------------------------------------------------------
BASE_REF="${GUARD_BASE:-${TEST_GUARD_BASE:-}}"
if [ -z "$BASE_REF" ] && [ -n "${GITHUB_BASE_REF:-}" ]; then
  BASE_REF="origin/${GITHUB_BASE_REF}"
fi
if [ -z "$BASE_REF" ]; then
  say "guard: no base ref given (not a pull_request run, and GUARD_BASE unset)."
  say "guard: there is nothing to compare against, so no check was performed. Exiting 0."
  exit 0
fi
if ! git rev-parse --verify --quiet "${BASE_REF}^{commit}" >/dev/null; then
  FAIL=1
  printf 'guard: FAIL: base ref %s does not exist here, so the comparison could not be made.\n' "$BASE_REF" >&2
  printf 'guard: Refusing to report success on a check that never ran.\n' >&2
  sum ""
  sum "## guard: FAIL -- the base ref \`$BASE_REF\` was not available, so nothing was compared."
  sum ""
  exit 1
fi

HEAD_SHA="$(git rev-parse HEAD)"
BASE_SHA="$(git rev-parse "${BASE_REF}^{commit}")"
MB="$(git merge-base "$BASE_SHA" "$HEAD_SHA" 2>/dev/null || true)"
[ -n "$MB" ] || MB="$BASE_SHA"

say "guard: base ref   = $BASE_REF ($BASE_SHA)"
say "guard: merge base = $MB"
say "guard: head       = $HEAD_SHA"
say ""

sum "# guard"
sum ""
sum "Judging \`$HEAD_SHA\` against \`$BASE_REF\` (\`$BASE_SHA\`, merge base \`$MB\`)."
sum ""
sum "Every check reads the base branch out of git and evaluates it in a scratch"
sum "worktree, so editing a file on the PR branch cannot satisfy them."
if [ "$MALFORMED_OVERRIDE" -eq 1 ]; then
  sum ""
  sum "> **A \`GUARD-OVERRIDE\` line in the PR body was malformed and waived nothing.**"
  sum "> See the job log for the exact line."
fi
sum ""

CHANGED="$(git diff --name-only "$MB" "$HEAD_SHA" 2>/dev/null || true)"

changed_since() { printf '%s\n' "$CHANGED" | grep -E "$1" || true; }

# The base version of a path, or nothing if it did not exist there.
base_version() {
  git show "${MB}:$1" 2>/dev/null || true
}
head_version() {
  git show "${HEAD_SHA}:$1" 2>/dev/null || true
}

# ---------------------------------------------------------------------------
# 1. (A) Declaration counts.
# ---------------------------------------------------------------------------
declared_names() {
  local out="$1"
  cargo test --workspace -- --list >"$out" 2>&1
  local rc=$?
  grep -E ': test$' "$out" | sed -E 's/: test$//; s/^[[:space:]]+//' | sort -u
  return $rc
}

say "=== (A) tests declared ==="
sum "## (A) Tests declared -- list, not run"
sum ""
HEAD_NAMES="/tmp/guard-head-names.txt"
if ! declared_names /tmp/guard-head-list.txt >"$HEAD_NAMES"; then
  say "guard: warning: 'cargo test --list' exited non-zero at head; its output tail:"
  tail -15 /tmp/guard-head-list.txt | sed 's/^/    /'
fi
HEAD_COUNT="$(wc -l <"$HEAD_NAMES" | tr -d ' ')"
say "guard: declared at head = $HEAD_COUNT"

BASE_NAMES="/tmp/guard-base-names.txt"
BASE_COUNT=""
BASE_WORKTREE="/tmp/guard-base-count-tree"
rm -rf "$BASE_WORKTREE"
if git worktree add --detach "$BASE_WORKTREE" "$MB" >/dev/null 2>&1; then
  if [ -f "$BASE_WORKTREE/Cargo.toml" ]; then
    if ( cd "$BASE_WORKTREE" && declared_names /tmp/guard-base-list.txt >"$BASE_NAMES" ); then
      BASE_COUNT="$(wc -l <"$BASE_NAMES" | tr -d ' ')"
    fi
  fi
  git worktree remove --force "$BASE_WORKTREE" >/dev/null 2>&1 || true
fi

sum ""
sum "| tree | tests declared |"
sum "|---|---|"
sum "| head (this PR) | $HEAD_COUNT |"

if [ -n "$BASE_COUNT" ]; then
  sum "| base (\`$BASE_REF\`) | $BASE_COUNT |"
  say "guard: declared at base = $BASE_COUNT"
  sum ""
  if [ "$HEAD_COUNT" -lt "$BASE_COUNT" ]; then
    REMOVED="$(comm -23 "$BASE_NAMES" "$HEAD_NAMES")"
    hard_fail "the PR declares FEWER tests than the base branch ($HEAD_COUNT < $BASE_COUNT)."
    say "--- tests declared at base, absent at head ---"
    printf '%s\n' "$REMOVED"
    sum "### Tests declared at the base branch but not at head"
    sum ""
    sum '```'
    printf '%s\n' "$REMOVED" >>"${SUMMARY_FILE:-/dev/stdout}"
    sum '```'
  else
    say "guard: (A) OK -- head $HEAD_COUNT >= base $BASE_COUNT"
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
  hard_fail "could not determine the base branch's test count, so check (A) was NOT performed. A comparison that did not happen must not read as a pass."
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
  say "guard: no changes between merge base and head at all."
  sum ""
  sum "_No file differs between the merge base and the head commit._"
else
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
  DIFF_TEXT="/tmp/guard-diff.txt"
  git diff -U0 "$MB" "$HEAD_SHA" -- . >"$DIFF_TEXT" 2>&1 || true

  REMOVED_MARKED="$(grep -E '^-[^-]' "$DIFF_TEXT" | grep -E "$MARKER" || true)"
  ADDED_MARKED="$(grep -E '^\+[^+]' "$DIFF_TEXT" | grep -E "$MARKER" || true)"
  # Only an actual ATTRIBUTE fails the job -- not a line that merely mentions one.
  ADDED_IGNORE="$(grep -E '^\+[[:space:]]*#\[ignore([^a-zA-Z_]|$)' "$DIFF_TEXT" || true)"

  sum ""
  sum "### Removed or added lines carrying an assertion or a test attribute"
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
    say "guard: (B) no assertion or test-attribute line added or removed."
  fi

  if [ -n "$ADDED_IGNORE" ]; then
    hard_fail "the PR adds #[ignore] to at least one test. A test that is ignored is a test that does not run."
    sum ""
    sum "### Added #[ignore] -- this is why the job failed"
    sum ""
    sum '```diff'
    printf '%s\n' "$ADDED_IGNORE" >>"${SUMMARY_FILE:-/dev/stdout}"
    sum '```'
  fi
fi

# ---------------------------------------------------------------------------
# 3. (a) Hand-test scripts.
# ---------------------------------------------------------------------------
# A "case" is one line that runs an attack and asserts an outcome. The six
# scripts use six different helpers, so the extractor understands all of them.
# Definition lines (`name() {`) are excluded -- they are the helper, not a case.
handtest_cases() {
  awk '
    /\(\)[[:space:]]*\{/ { next }
    /^[[:space:]]*#/ { next }
    {
      line=$0
      sub(/^[[:space:]]+/,"",line)
      # run <section> <want> <rest>
      if (line ~ /^run[[:space:]]/) {
        r=line; sub(/^run[[:space:]]+/,"",r)
        sec=r; sub(/[[:space:]].*/,"",sec)
        r=substr(r, length(sec)+1); sub(/^[[:space:]]+/,"",r)
        want=r; sub(/[[:space:]].*/,"",want)
        rest=substr(r, length(want)+1); sub(/^[[:space:]]+/,"",rest)
        print sec "|" rest "\t" want
        next
      }
      # named helpers: identity is the whole argument text, expectation is the name
      for (fn in FNS) { }
      n=split("ok expect_ran expect_refused hole_line run_ok run_refuse assert must_exist_inside must_not_exist_anywhere must_not_exist", parts, " ")
      for (i=1;i<=n;i++) {
        f=parts[i]
        if (line ~ ("^" f "[[:space:]]")) {
          rest=line; sub(("^" f "[[:space:]]+"),"",rest)
          gsub(/[[:space:]]+$/,"",rest)
          print f "|" rest "\t" f
          next
        }
      }
    }
  ' "$1"
}

# Expected totals declared in a hand-test script, as "NAME=VALUE" lines.
handtest_totals() {
  grep -E '^[[:space:]]*(EXPECTED|TOTAL|CASES|LINES)[A-Z_]*=[0-9]+' "$1" 2>/dev/null \
    | sed 's/^[[:space:]]*//; s/[[:space:]]*$//' | sort || true
}

sum ""
sum "## (a) Hand-test scripts"
say ""
say "=== (a) hand-test scripts vs base ==="

HANDTEST_CHANGED="$(changed_since '^crates/[^/]+/hand-test-.*\.sh$')"
if [ -z "$HANDTEST_CHANGED" ]; then
  sum ""
  sum "_No hand-test script changed._"
  say "guard: (a) no hand-test script changed."
else
  sum ""
  sum "Changed hand-test scripts:"
  sum ""
  sum '```'
  printf '%s\n' "$HANDTEST_CHANGED" >>"${SUMMARY_FILE:-/dev/stdout}"
  sum '```'

  for f in $HANDTEST_CHANGED; do
    base_v="$(base_version "$f" >/dev/null 2>&1; printf '')"
    if ! git cat-file -e "${MB}:$f" 2>/dev/null; then
      # Entirely new script: nothing removed, so nothing to weaken.
      sum ""
      sum "- \`$f\` is new in this PR (no base version) -- nothing removed."
      continue
    fi
    base_cases="$(git show "${MB}:$f" | handtest_cases /dev/stdin)"
    head_cases="$(handtest_cases <(git show "${HEAD_SHA}:$f") 2>/dev/null || true)"
    if [ -z "$head_cases" ] && [ -n "$base_cases" ]; then
      head_cases="$(git show "${HEAD_SHA}:$f" | handtest_cases /dev/stdin)"
    fi

    base_n="$(printf '%s\n' "$base_cases" | grep -c . || true)"
    head_n="$(printf '%s\n' "$head_cases" | grep -c . || true)"

    # (1) A case removed, or its identity changed.
    removed_cases="$(comm -23 <(printf '%s\n' "$base_cases" | sort -u) \
                              <(printf '%s\n' "$head_cases" | sort -u) || true)"
    added_cases="$(comm -13 <(printf '%s\n' "$base_cases" | sort -u) \
                            <(printf '%s\n' "$head_cases" | sort -u) || true)"

    # (1a) expected-result WEAKENED for a case that still exists: the same
    # identity now expects a weaker outcome. R/REFUSE -> A/OK is the weakening
    # that matters (a rejection case turned into an acceptance case).
    weakened=""
    if [ -n "$removed_cases" ] && [ -n "$added_cases" ]; then
      while IFS= read -r rc_line; do
        [ -n "$rc_line" ] || continue
        ident="${rc_line%%$'\t'*}"
        old_want="${rc_line##*$'\t'}"
        new_want="$(printf '%s\n' "$added_cases" | awk -F'\t' -v i="$ident" '$1==i{print $2}' | head -1)"
        if [ -n "$new_want" ] && [ "$old_want" != "$new_want" ]; then
          weakened="${weakened}${f}: '${ident}' expected-result changed ${old_want} -> ${new_want}"$'\n'
        fi
      done <<<"$removed_cases"
    fi

    # (2) A whole case removed (identity gone entirely).
    truly_removed=""
    if [ -n "$removed_cases" ]; then
      while IFS= read -r rc_line; do
        [ -n "$rc_line" ] || continue
        ident="${rc_line%%$'\t'*}"
        if ! printf '%s\n' "$added_cases" | awk -F'\t' -v i="$ident" '$1==i{f=1}END{exit !f}'; then
          truly_removed="${truly_removed}${f}: [${ident}]"$'\n'
        fi
      done <<<"$removed_cases"
    fi

    # (3) Case count drop.
    count_drop=0
    if [ "$head_n" -lt "$base_n" ]; then
      count_drop=1
    fi

    # (4) Expected totals lowered.
    base_totals="$(git show "${MB}:$f" | handtest_totals /dev/stdin)"
    head_totals="$(git show "${HEAD_SHA}:$f" | handtest_totals /dev/stdin)"
    totals_lowered=""
    while IFS= read -r t; do
      [ -n "$t" ] || continue
      name="${t%%=*}"; old="${t##*=}"
      new="$(printf '%s\n' "$head_totals" | grep -E "^${name}=" | head -1)"
      new="${new##*=}"
      if [ -n "$new" ] && [ "$new" -lt "$old" ] 2>/dev/null; then
        totals_lowered="${totals_lowered}${f}: ${name} lowered ${old} -> ${new}"$'\n'
      fi
    done <<<"$base_totals"

    if [ -n "$truly_removed" ] || [ -n "$weakened" ] || [ "$count_drop" -eq 1 ] || [ -n "$totals_lowered" ]; then
      msg="hand-test script $f:"
      [ -n "$truly_removed" ] && msg="$msg
  cases removed:
$(printf '%s' "$truly_removed" | sed 's/^/    /')"
      [ -n "$weakened" ] && msg="$msg
  expected-results weakened:
$(printf '%s' "$weakened" | sed 's/^/    /')"
      [ "$count_drop" -eq 1 ] && msg="$msg
  case count dropped: $base_n -> $head_n"
      [ -n "$totals_lowered" ] && msg="$msg
  expected totals lowered:
$(printf '%s' "$totals_lowered" | sed 's/^/    /')"
      guard_fail "hand-tests" "$msg"
      sum ""
      sum "**\`$f\`** -- cases $base_n -> $head_n"
      if [ -n "$truly_removed" ]; then
        sum ""
        sum "Removed cases:"
        sum '```'
        printf '%s' "$truly_removed" >>"${SUMMARY_FILE:-/dev/stdout}"
        sum '```'
      fi
      if [ -n "$weakened" ]; then
        sum ""
        sum "Weakened expected-results:"
        sum '```'
        printf '%s' "$weakened" >>"${SUMMARY_FILE:-/dev/stdout}"
        sum '```'
      fi
      if [ -n "$totals_lowered" ]; then
        sum ""
        sum "Lowered expected totals:"
        sum '```'
        printf '%s' "$totals_lowered" >>"${SUMMARY_FILE:-/dev/stdout}"
        sum '```'
      fi
    else
      say "guard: (a) OK -- $f: cases $base_n -> $head_n, no case removed or weakened."
      sum ""
      sum "- \`$f\`: cases $base_n -> $head_n. No case removed or weakened."
    fi
  done
fi

# ---------------------------------------------------------------------------
# 4. (b) The fuzz oracle.
# ---------------------------------------------------------------------------
sum ""
sum "## (b) The fuzz oracle"
say ""
say "=== (b) fuzz oracle vs base ==="

FUZZ_CHANGED="$(changed_since '^fuzz/fuzz_targets/')"
if [ -z "$FUZZ_CHANGED" ]; then
  sum ""
  sum "_No fuzz target changed._"
  say "guard: (b) no fuzz target changed."
else
  ORACLE_MARK='panic!|assert_eq!|assert_ne!|assert!'
  CONTAIN_MARK='starts_with\(&f\.root\)|starts_with\(&self\.root\)'
  for f in $FUZZ_CHANGED; do
    if ! git cat-file -e "${MB}:$f" 2>/dev/null; then
      sum ""
      sum "- \`$f\` is new in this PR (no base version) -- nothing removed."
      continue
    fi
    git diff -U0 "$MB" "$HEAD_SHA" -- "$f" >/tmp/guard-fuzz.diff 2>&1 || true

    removed_asserts="$(grep -E '^-[^-]' /tmp/guard-fuzz.diff | grep -E "$ORACLE_MARK" | sed 's/^/  /' || true)"
    removed_contain="$(grep -E '^-[^-]' /tmp/guard-fuzz.diff | grep -E "$CONTAIN_MARK" | sed 's/^/  /' || true)"

    base_assert_n="$(git show "${MB}:$f" | grep -cE "$ORACLE_MARK" || true)"
    head_assert_n="$(git show "${HEAD_SHA}:$f" | grep -cE "$ORACLE_MARK" || true)"
    base_contain_n="$(git show "${MB}:$f" | grep -cE "$CONTAIN_MARK" || true)"
    head_contain_n="$(git show "${HEAD_SHA}:$f" | grep -cE "$CONTAIN_MARK" || true)"

    bad=0
    msg="fuzz oracle $f:"
    if [ -n "$removed_asserts" ]; then
      bad=1
      msg="$msg
  assertions or panics REMOVED:
$removed_asserts"
    fi
    if [ -n "$removed_contain" ]; then
      bad=1
      msg="$msg
  containment check REMOVED:
$removed_contain"
    fi
    if [ "$head_assert_n" -lt "$base_assert_n" ]; then
      bad=1
      msg="$msg
  assertion/panic count dropped: $base_assert_n -> $head_assert_n"
    fi
    if [ "$head_contain_n" -lt "$base_contain_n" ]; then
      bad=1
      msg="$msg
  containment-check count dropped: $base_contain_n -> $head_contain_n"
    fi

    if [ "$bad" -eq 1 ]; then
      guard_fail "fuzz-oracle" "$msg"
      sum ""
      sum "**\`$f\`** -- assertions/panics $base_assert_n -> $head_assert_n, containment checks $base_contain_n -> $head_contain_n"
      sum ""
      sum '```diff'
      { [ -n "$removed_asserts" ] && printf '%s\n' "$removed_asserts"
        [ -n "$removed_contain" ] && printf '%s\n' "$removed_contain"; } >>"${SUMMARY_FILE:-/dev/stdout}"
      sum '```'
    else
      say "guard: (b) OK -- $f: assertions/panics $base_assert_n -> $head_assert_n, containment $base_contain_n -> $head_contain_n."
      sum ""
      sum "- \`$f\`: assertions/panics $base_assert_n -> $head_assert_n, containment checks $base_contain_n -> $head_contain_n. Nothing removed or loosened."
    fi
  done
fi

# ---------------------------------------------------------------------------
# 5. (c) Mutation exclusions.
# ---------------------------------------------------------------------------
sum ""
sum "## (c) Mutation exclusions"
say ""
say "=== (c) .cargo/mutants.toml vs base ==="

if ! printf '%s\n' "$CHANGED" | grep -qx '.cargo/mutants.toml'; then
  sum ""
  sum "_\\\`.cargo/mutants.toml\\\` not changed._"
  say "guard: (c) mutants.toml not changed."
else
  git diff -U0 "$MB" "$HEAD_SHA" -- .cargo/mutants.toml >/tmp/guard-mutants.diff 2>&1 || true
  # Added lines that look like an exclusion ENTRY: a quoted pattern inside the
  # exclude arrays, or a bare glob line inside exclude_globs.
  added_exclusions="$(grep -nE '^\+[^+]' /tmp/guard-mutants.diff \
      | grep -E '^\s*[0-9]+:\+[[:space:]]*(".*"|\x27.*\x27|[A-Za-z0-9_./*?-]+)[,]?[[:space:]]*$' \
      | grep -vE '^\s*[0-9]+:\+[[:space:]]*#' || true)"

  # The head file's raw lines, so we can look at adjacency around each addition.
  head_toml="/tmp/guard-mutants-head.txt"
  git show "${HEAD_SHA}:.cargo/mutants.toml" >"$head_toml" 2>/dev/null || true
  # Where each exclusion entry sits in the head file.
  entry_lines="$(grep -nE '^[[:space:]]*(".*"|\x27.*\x27)[,]?[[:space:]]*$' "$head_toml" | cut -d: -f1 || true)"

  unreasoned=""
  if [ -n "$added_exclusions" ]; then
    while IFS= read -r al; do
      [ -n "$al" ] || continue
      text="${al#*:}"
      text="${text#+}"
      text="$(printf '%s' "$text" | sed 's/^[[:space:]]*//; s/[[:space:]]*$//')"
      # same-line trailing comment?
      if printf '%s' "$text" | grep -qE '#.{10,}'; then
        continue
      fi
      # a comment on an adjacent line in the head file, at least 10 chars of text?
      hit=0
      for ln in $entry_lines; do
        this="$(sed -n "${ln}p" "$head_toml" | sed 's/^[[:space:]]*//; s/[[:space:]]*$//')"
        [ "$this" = "$text" ] || continue
        for adj in $((ln-1)) $((ln+1)); do
          [ "$adj" -ge 1 ] || continue
          aline="$(sed -n "${adj}p" "$head_toml")"
          if printf '%s' "$aline" | grep -qE '^[[:space:]]*#'; then
            body="$(printf '%s' "$aline" | sed 's/^[[:space:]]*#*[[:space:]]*//; s/[[:space:]]*$//')"
            if [ "${#body}" -ge 10 ]; then hit=1; break 2; fi
          fi
        done
      done
      [ "$hit" -eq 1 ] && continue
      unreasoned="${unreasoned}${text}"$'\n'
    done <<<"$added_exclusions"
  fi

  if [ -n "$unreasoned" ]; then
    guard_fail "mutants-toml" "an exclusion was added to .cargo/mutants.toml with no reason comment on an adjacent line (a comment of at least 10 characters on the line above, below, or trailing).

  added without a reason:
$(printf '%s' "$unreasoned" | sed 's/^/    /')"
    sum ""
    sum "Exclusions added with no adjacent reason comment:"
    sum ""
    sum '```'
    printf '%s' "$unreasoned" >>"${SUMMARY_FILE:-/dev/stdout}"
    sum '```'
  else
    if [ -n "$added_exclusions" ]; then
      n="$(printf '%s\n' "$added_exclusions" | grep -c . || true)"
      say "guard: (c) OK -- $n added exclusion(s), each with an adjacent reason comment."
      sum ""
      sum "- $n added exclusion(s), each carrying a reason comment on an adjacent line."
    else
      say "guard: (c) OK -- mutants.toml changed, no exclusion entry added."
      sum ""
      sum "- \\\`.cargo/mutants.toml\\\` changed; no exclusion entry added."
    fi
  fi
fi

# ---------------------------------------------------------------------------
# 6. (d) Workflows.
# ---------------------------------------------------------------------------
sum ""
sum "## (d) Workflows"
say ""
say "=== (d) workflows vs base ==="

WF_CHANGED="$(changed_since '^\.github/workflows/')"
if [ -z "$WF_CHANGED" ]; then
  sum ""
  sum "_No workflow changed._"
  say "guard: (d) no workflow changed."
else
  for f in $WF_CHANGED; do
    if ! git cat-file -e "${MB}:$f" 2>/dev/null; then
      sum ""
      sum "- \`$f\` is new in this PR (no base version) -- no step removed."
      continue
    fi
    git diff -U0 "$MB" "$HEAD_SHA" -- "$f" >/tmp/guard-wf.diff 2>&1 || true

    # (1) A step deleted.
    #
    # Detected by the COMMANDS, not the labels. A step whose `- name:` was
    # reworded while its `run:`/`uses:` stayed the same is a rename, not a
    # deletion, and must not fire -- that was the first false positive this
    # script produced, on its own commit. What matters is whether the work the
    # step did is still being done.
    #
    # So: collect the set of `uses:` values and `run:` commands. A command that
    # existed at base and is gone at head means a step was removed (or its
    # command was swapped for something else, which is the same risk).
    #
    # A block `run: |` has no command on its own line, so the first non-blank,
    # non-comment line of the block is taken as its identity.
    wf_commands() {
      git show "$1:$2" 2>/dev/null | awk '
        /^[[:space:]]*-[[:space:]]*uses:[[:space:]]*/ {
          v=$0; sub(/^[[:space:]]*-[[:space:]]*uses:[[:space:]]*/,"",v)
          sub(/@.*$/,"",v); gsub(/[[:space:]]/,"",v); print "uses:" v; next }
        /^[[:space:]]*uses:[[:space:]]*/ {
          v=$0; sub(/^[[:space:]]*uses:[[:space:]]*/,"",v)
          sub(/@.*$/,"",v); gsub(/[[:space:]]/,"",v); print "uses:" v; next }
        /^[[:space:]]*run:[[:space:]]*[^|>[:space:]]/ {
          v=$0; sub(/^[[:space:]]*run:[[:space:]]*/,"",v)
          gsub(/[[:space:]]+/," ",v); sub(/[[:space:]]+$/,"",v); print "run:" v; next }
        /^[[:space:]]*run:[[:space:]]*[|>][-+]?[[:space:]]*$/ { inblock=1; next }
        inblock==1 {
          if ($0 ~ /^[[:space:]]*$/) next
          if ($0 ~ /^[[:space:]]*#/) next
          v=$0; gsub(/^[[:space:]]+|[[:space:]]+$/,"",v)
          gsub(/[[:space:]]+/," ",v); print "run:" v; inblock=0; next }
      ' | sort -u
    }
    base_cmds="$(wf_commands "$MB" "$f")"
    head_cmds="$(wf_commands "$HEAD_SHA" "$f")"
    removed_names="$(comm -23 <(printf '%s\n' "$base_cmds" | grep .) \
                              <(printf '%s\n' "$head_cmds" | grep .) || true)"

    # (2) An added continue-on-error, always-true guard, or swallowed failure.
    added_swallow="$(grep -E '^\+[^+]' /tmp/guard-wf.diff \
        | grep -E 'continue-on-error:[[:space:]]*true|if:[[:space:]]*true|if:[[:space:]]*\$\{\{[[:space:]]*true|\|\|[[:space:]]*true[[:space:]]*$|^\+[[:space:]]*exit 0[[:space:]]*$' \
        || true)"

    # (3) A loosened timeout or retry: a numeric value that went UP.
    # Extract "key=value" pairs from removed/added lines and compare per key.
    loosened=""
    for key in timeout-minutes timeout_seconds child_timeout_seconds max_attempts retry retries attempts; do
      old_vals="$(git show "${MB}:$f" | grep -oE "${key}[=:] *[0-9]+" | grep -oE '[0-9]+' | sort -n | tail -1 || true)"
      new_vals="$(git show "${HEAD_SHA}:$f" | grep -oE "${key}[=:] *[0-9]+" | grep -oE '[0-9]+' | sort -n | tail -1 || true)"
      if [ -n "$old_vals" ] && [ -n "$new_vals" ] && [ "$new_vals" -gt "$old_vals" ] 2>/dev/null; then
        loosened="${loosened}${f}: ${key} raised ${old_vals} -> ${new_vals}"$'\n'
      fi
    done
    # `timeout=` inside a python/urllib call, and `for attempt in (1, 2)` growth.
    old_to="$(git show "${MB}:$f" | grep -oE 'timeout=[0-9]+' | grep -oE '[0-9]+' | sort -n | tail -1 || true)"
    new_to="$(git show "${HEAD_SHA}:$f" | grep -oE 'timeout=[0-9]+' | grep -oE '[0-9]+' | sort -n | tail -1 || true)"
    if [ -n "$old_to" ] && [ -n "$new_to" ] && [ "$new_to" -gt "$old_to" ] 2>/dev/null; then
      loosened="${loosened}${f}: timeout= raised ${old_to} -> ${new_to}"$'\n'
    fi

    if [ -n "$removed_names" ] || [ -n "$added_swallow" ] || [ -n "$loosened" ]; then
      msg="workflow $f:"
      [ -n "$removed_names" ] && msg="$msg
  step(s) deleted:
$(printf '%s' "$removed_names" | sed 's/^/    - /')"
      [ -n "$added_swallow" ] && msg="$msg
  a failure-swallowing line was added:
$(printf '%s' "$added_swallow" | sed 's/^/    /')"
      [ -n "$loosened" ] && msg="$msg
  timeout or retry loosened:
$(printf '%s' "$loosened" | sed 's/^/    /')"
      guard_fail "workflows" "$msg"
      sum ""
      sum "**\`$f\`**"
      if [ -n "$removed_names" ]; then
        sum ""
        sum "Steps present at base, absent at head:"
        sum '```'
        printf '%s\n' "$removed_names" >>"${SUMMARY_FILE:-/dev/stdout}"
        sum '```'
      fi
      if [ -n "$added_swallow" ]; then
        sum ""
        sum "Failure-swallowing lines added:"
        sum '```diff'
        printf '%s\n' "$added_swallow" >>"${SUMMARY_FILE:-/dev/stdout}"
        sum '```'
      fi
      if [ -n "$loosened" ]; then
        sum ""
        sum "Loosened timeouts / retries:"
        sum '```'
        printf '%s' "$loosened" >>"${SUMMARY_FILE:-/dev/stdout}"
        sum '```'
      fi
    else
      say "guard: (d) OK -- $f: no command deleted, no swallow added, no timeout/retry loosened."
      sum ""
      sum "- \`$f\`: no step deleted, no failure-swallowing line added, no timeout or retry loosened."
    fi
  done
fi

# ---------------------------------------------------------------------------
# 7. (e) The gates themselves.
# ---------------------------------------------------------------------------
sum ""
sum "## (e) The gates themselves"
say ""
say "=== (e) scripts/check.sh and scripts/guard.sh vs base ==="

GATE_FILES="$(changed_since '^scripts/(check|guard)\.sh$')"
if [ -z "$GATE_FILES" ]; then
  sum ""
  sum "_Neither \\\`scripts/check.sh\\\` nor \\\`scripts/guard.sh\\\` changed._"
  say "guard: (e) gates unchanged."
else
  for f in $GATE_FILES; do
    if ! git cat-file -e "${MB}:$f" 2>/dev/null; then
      sum ""
      sum "- \`$f\` is new in this PR (no base version) -- no step removed."
      continue
    fi
    git diff -U0 "$MB" "$HEAD_SHA" -- "$f" >/tmp/guard-gate.diff 2>&1 || true

    # A "step" is a `step "<name>"` line in check.sh, or a `# ---`-delimited
    # section header / `##` summary section in either file. Compare the SET of
    # step labels; a label that existed at base and is gone at head means a
    # step was removed.
    base_labels="$(git show "${MB}:$f" | grep -oE '^[[:space:]]*(step|die)[[:space:]]+"[^"]*"' \
                     | sed 's/^[[:space:]]*//' | sort -u)"
    head_labels="$(git show "${HEAD_SHA}:$f" | grep -oE '^[[:space:]]*(step|die)[[:space:]]+"[^"]*"' \
                     | sed 's/^[[:space:]]*//' | sort -u)"
    removed_labels="$(comm -23 <(printf '%s\n' "$base_labels" | grep . ) <(printf '%s\n' "$head_labels" | grep .) || true)"

    # Also compare the top-level check invocations, so a removed `cargo ...`
    # step is caught even if its `step` line was reworded.
    base_cmds="$(git show "${MB}:$f" | grep -oE '^[[:space:]]*cargo (fmt|build|test|clippy|deny)[^|]*' \
                   | sed 's/[[:space:]]*$//' | sed 's/^[[:space:]]*//' | sort -u)"
    head_cmds="$(git show "${HEAD_SHA}:$f" | grep -oE '^[[:space:]]*cargo (fmt|build|test|clippy|deny)[^|]*' \
                   | sed 's/[[:space:]]*$//' | sed 's/^[[:space:]]*//' | sort -u)"
    removed_cmds="$(comm -23 <(printf '%s\n' "$base_cmds" | grep .) <(printf '%s\n' "$head_cmds" | grep .) || true)"

    # A guard category that no longer appears at all: the function of a check.
    base_cats="$(git show "${MB}:$f" | grep -oE 'guard_fail "[a-z-]+"|### \([a-eA-E]\) [A-Za-z].*' | sort -u)"
    head_cats="$(git show "${HEAD_SHA}:$f" | grep -oE 'guard_fail "[a-z-]+"|### \([a-eA-E]\) [A-Za-z].*' | sort -u)"
    removed_cats="$(comm -23 <(printf '%s\n' "$base_cats" | grep .) <(printf '%s\n' "$head_cats" | grep .) || true)"

    if [ -n "$removed_labels" ] || [ -n "$removed_cmds" ] || [ -n "$removed_cats" ]; then
      msg="$f: a step was removed."
      [ -n "$removed_labels" ] && msg="$msg
  step labels gone:
$(printf '%s' "$removed_labels" | sed 's/^/    /')"
      [ -n "$removed_cmds" ] && msg="$msg
  check invocations gone:
$(printf '%s' "$removed_cmds" | sed 's/^/    /')"
      [ -n "$removed_cats" ] && msg="$msg
  guard categories gone:
$(printf '%s' "$removed_cats" | sed 's/^/    /')"
      guard_fail "gates" "$msg"
      sum ""
      sum "**\`$f\`**"
      if [ -n "$removed_labels" ]; then
        sum ""
        sum "Step labels removed:"
        sum '```'
        printf '%s\n' "$removed_labels" >>"${SUMMARY_FILE:-/dev/stdout}"
        sum '```'
      fi
      if [ -n "$removed_cmds" ]; then
        sum ""
        sum "Check invocations removed:"
        sum '```'
        printf '%s\n' "$removed_cmds" >>"${SUMMARY_FILE:-/dev/stdout}"
        sum '```'
      fi
      if [ -n "$removed_cats" ]; then
        sum ""
        sum "Guard categories removed:"
        sum '```'
        printf '%s\n' "$removed_cats" >>"${SUMMARY_FILE:-/dev/stdout}"
        sum '```'
      fi
    else
      say "guard: (e) OK -- $f: no step removed."
      sum ""
      sum "- \`$f\`: no step removed."
    fi
  done
fi

# ---------------------------------------------------------------------------
# 8. Verdict.
# ---------------------------------------------------------------------------
sum ""
sum "## Verdict"
sum ""
if [ "${#WAIVED[@]}" -gt 0 ]; then
  sum ""
  sum "**Protections waived by GUARD-OVERRIDE this run:** $(printf '%s, ' "${WAIVED[@]%%|*}" | sed 's/, $//')"
fi
if [ "$FAIL" -eq 0 ]; then
  say "guard: PASS"
  sum ""
  sum "**Passed.** No test was removed or silenced, no gate was weakened, and no"
  sum "protection was reduced without an override."
  sum ""
  sum "This does not mean the tests are *good* -- see the limits listed at the top"
  sum "of \`scripts/guard.sh\`. It means none of the things above was removed or"
  sum "loosened."
  exit 0
else
  say "guard: FAIL"
  sum ""
  sum "**Failed.** See the sections above."
  exit 1
fi
