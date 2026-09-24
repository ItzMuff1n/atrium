#!/usr/bin/env bash
#
# goal-gate.sh -- a /goal quality gate for an Atrium topic.
#
#   scripts/goal-gate.sh "<milestone>"
#
# Hermes runs a /goal's quality gates at EVERY turn boundary and auto-pauses
# the goal after 3 consecutive failures. That shapes the whole design: this
# gate must be quiet while a topic is simply unfinished, because "unfinished"
# is the normal state for the entire run. It exists to catch exactly two
# things, and nothing else:
#
#   1. PREMATURE FINISH -- the `Your turn: <topic>` closing issue has been
#      opened while the milestone still has open issues that are not parked.
#      Opening the closing issue is the agent declaring the topic done; doing
#      that with work still open is the failure this catches.
#      (A topic whose only remaining issues are `parked` IS finished: parking
#      is a recorded decision, and the closing issue lists it.)
#
#   2. A RED main -- the required `check` workflow on the current main commit
#      concluded in failure. The topic builds on main; if main is broken,
#      every later step inherits it.
#
# Read-only. `gh` only. Seconds, not minutes. Works from any checkout.
#
# Exit codes:
#   0  all clear (including "the topic is simply not finished yet")
#   1  premature finish
#   2  main is red
#   3  bad usage (missing/unknown milestone, unreadable repo)
#
# ON ERRORS, AND A DELIBERATE CHOICE: the plan says exit non-zero ONLY for
# those two conditions, so an inability to determine state (gh not authed, no
# network) exits 0 -- with a loud WARNING on stderr and on stdout. That is the
# literal instruction and it is the safe reading for a goal loop, since the
# alternative is pausing a run over a transient hiccup. It is, however, the
# same shape as a silent pass, so the warning is deliberately unmissable and
# the choice is recorded in the PR. Flagged rather than hidden.

set -uo pipefail

REPO_FALLBACK="ItzMuff1n/atrium"

usage() {
  echo "usage: $0 <milestone> [--repo OWNER/NAME]" >&2
  echo "  e.g. $0 'Pipeline v2'" >&2
}

MILESTONE=""
REPO=""
while [ $# -gt 0 ]; do
  case "$1" in
    --repo)
      # Guard the argument count BEFORE shifting. With `--repo` as the last
      # argument, `shift 2` fails ("shift count out of range"), $# stays at 1,
      # and this loop spins forever -- the gate HANGS instead of exiting.
      # Confirmed: `goal-gate.sh <milestone> --repo` ran past an 8s timeout
      # (exit 124). Found by the batch review.
      if [ $# -lt 2 ]; then
        echo "goal-gate: --repo needs a value (OWNER/NAME)" >&2
        usage
        exit 3
      fi
      REPO="$2"; shift 2 ;;
    -h|--help) usage; exit 3 ;;
    *)
      if [ -n "$MILESTONE" ]; then
        echo "goal-gate: unexpected extra argument '$1'" >&2
        usage
        exit 3
      fi
      MILESTONE="$1"; shift ;;
  esac
done

if [ -z "$MILESTONE" ]; then
  usage
  exit 3
fi

warn() { printf 'goal-gate: WARNING: %s\n' "$1" >&2; printf 'goal-gate: WARNING: %s\n' "$1"; }

# ---- repo resolution: work from any checkout -------------------------------
if [ -z "$REPO" ]; then
  url="$(git remote get-url origin 2>/dev/null || true)"
  case "$url" in
    git@github.com:*) REPO="${url#git@github.com:}" ;;
    *github.com/*)    REPO="${url##*github.com/}" ;;
    *)                REPO="" ;;
  esac
  REPO="${REPO%.git}"
fi
if [ -z "$REPO" ]; then
  REPO="$REPO_FALLBACK"
  warn "could not read the origin remote; falling back to $REPO"
fi

if ! command -v gh >/dev/null 2>&1; then
  warn "gh is not installed, so the gate cannot check anything. Exiting 0."
  exit 0
fi

# ---- 1. premature finish --------------------------------------------------
issues_json="$(gh issue list --repo "$REPO" --state open --limit 200 \
                 --json number,title,labels,milestone 2>&1)"
if ! printf '%s' "$issues_json" | python3 -c 'import json,sys; json.load(sys.stdin)' 2>/dev/null; then
  warn "could not list open issues for $REPO (gh said: $(printf '%s' "$issues_json" | head -1)). Exiting 0."
  exit 0
fi

verdict="$(printf '%s' "$issues_json" | MILESTONE="$MILESTONE" python3 -c '
import json, os, sys
want = os.environ["MILESTONE"]
issues = json.load(sys.stdin)

def is_closing(i):
    return (i.get("title") or "").strip().lower() == f"your turn: {want}".lower()

closing = [i for i in issues if is_closing(i)]

in_ms, in_ms_unparked, parked = [], [], []
for i in issues:
    ms = (i.get("milestone") or {}) or {}
    if (ms.get("title") or "") != want:
        continue
    # The closing issue is itself in the milestone. Counting it as open work
    # would make the "everything else is parked" case fail forever -- the
    # closing issue is open BY DEFINITION when the gate is asking the
    # question. Excluded, or path 1b is unsatisfiable.
    if is_closing(i):
        continue
    in_ms.append(i)
    labels = {l.get("name") for l in (i.get("labels") or [])}
    (parked if "parked" in labels else in_ms_unparked).append(i)

print(f"closing={len(closing)}")
print(f"open_in_ms={len(in_ms)}")
print(f"open_unparked={len(in_ms_unparked)}")
print(f"parked={len(parked)}")
for i in in_ms_unparked:
    num = i.get("number")
    title = i.get("title")
    print(f"unparked:#{num} {title}")
')"

# A crashed or empty verdict must NOT be read as "no problem". Without this,
# a Python error made every count empty, the premature-finish test compared
# "0" against 0, and the gate exited 0 -- i.e. a broken gate reporting all
# clear. That happened in the first version of this script and the detector
# caught it: several checks "passed" while stderr showed a SyntaxError.
if ! printf '%s\n' "$verdict" | grep -q '^open_unparked='; then
  warn "could not parse the issue list (no counts were produced). Exiting 0."
  exit 0
fi

get() { printf '%s\n' "$verdict" | sed -n "s/^$1=//p" | head -1; }
n_closing="$(get closing)"
n_unparked="$(get open_unparked)"

# Does the milestone exist at all? A misspelled or renamed milestone makes
# every count zero, which reads exactly like "nothing to worry about" and
# exits 0 -- the gate silently switched off, which is the dangerous direction
# for a thing that is supposed to catch a premature finish. The header already
# documents exit 3 for "missing/unknown milestone"; this makes the code match
# that contract. Found by the batch review.
if ! gh api "repos/$REPO/milestones?state=all&per_page=100" \
        --jq '.[].title' 2>/dev/null | grep -Fxq "$MILESTONE"; then
  # Distinguish "the API call failed" from "the milestone really is absent":
  # the first is an inability to determine state (warn + 0, per the header),
  # the second is a caller error (3).
  if ! gh api "repos/$REPO/milestones?state=all&per_page=100" \
        --jq '.[].title' >/dev/null 2>&1; then
    warn "could not list milestones for $REPO. Exiting 0."
    exit 0
  fi
  echo "goal-gate: no milestone titled \"$MILESTONE\" in $REPO." >&2
  echo "goal-gate: a misspelled milestone would make every count zero and read as all-clear, so this is exit 3." >&2
  exit 3
fi

printf 'goal-gate: milestone "%s" -- open issues: %s (%s unparked, %s parked); closing issue present: %s\n' \
  "$MILESTONE" "$(get open_in_ms)" "$n_unparked" "$(get parked)" \
  "$([ "${n_closing:-0}" -gt 0 ] && echo yes || echo no)"

if [ "${n_closing:-0}" -gt 0 ] && [ "${n_unparked:-0}" -gt 0 ]; then
  echo "goal-gate: FAIL -- the closing issue \"Your turn: $MILESTONE\" is open while $n_unparked issue(s) in the milestone are still open and not parked:"
  printf '%s\n' "$verdict" | sed -n 's/^unparked://p' | sed 's/^/  /'
  echo "goal-gate: either finish or park them, or the topic is not done."
  exit 1
fi

# ---- 2. is main red? -----------------------------------------------------
# The required check is the `check` workflow (fmt, build, test, clippy) -- the
# one that gates every merge. fuzz and mutants are scheduled jobs that file
# issues rather than block a merge, so a red nightly is not "main is broken"
# in the sense this gate means. Named explicitly rather than "any workflow".
# A run still in progress is NOT red: this gate reports failures, not
# weather.
main_sha="$(gh api "repos/$REPO/commits/main" --jq .sha 2>/dev/null || true)"
if [ -z "$main_sha" ]; then
  warn "could not read main's commit for $REPO. Exiting 0."
  exit 0
fi

runs="$(gh run list --repo "$REPO" --branch main --workflow check --limit 20 \
          --json headSha,status,conclusion,createdAt 2>&1)"
if ! printf '%s' "$runs" | python3 -c 'import json,sys; json.load(sys.stdin)' 2>/dev/null; then
  warn "could not list CI runs for $REPO. Exiting 0."
  exit 0
fi

red="$(printf '%s' "$runs" | MAIN_SHA="$main_sha" python3 -c '
import json, os, sys
sha = os.environ["MAIN_SHA"]
runs = [r for r in json.load(sys.stdin) if (r.get("headSha") or "") == sha]
if not runs:
    print("none"); raise SystemExit
runs.sort(key=lambda r: r.get("createdAt") or "")
latest = runs[-1]
if latest.get("status") != "completed":
    print("pending"); raise SystemExit
print(latest.get("conclusion") or "unknown")
')"

case "$red" in
  none)
    warn "no check run found for main's current commit ${main_sha:0:8}; treating as not-red. Exiting 0."
    exit 0 ;;
  pending)
    printf 'goal-gate: main %s -- check is still running; not red. OK\n' "${main_sha:0:8}"
    exit 0 ;;
  success|skipped|neutral)
    printf 'goal-gate: main %s -- check %s. OK\n' "${main_sha:0:8}" "$red"
    exit 0 ;;
  *)
    printf 'goal-gate: FAIL -- main is red: the `check` workflow on %s concluded %s.\n' \
      "${main_sha:0:8}" "$red"
    printf 'goal-gate: https://github.com/%s/commit/%s/checks\n' "$REPO" "$main_sha"
    exit 2 ;;
esac
