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
#   3  bad usage (missing/unknown milestone, unreadable repo, bad --spend-limit)
#   4  OVER BUDGET -- only reachable when --spend-limit was given (see below)
#
# THE SPEND LIMIT (--spend-limit, optional; added 26 Sep 2026, issue #94)
#
#   scripts/goal-gate.sh "<milestone>" --spend-limit 75M
#
# Two stop conditions written only in prose have now been skipped -- 25M in
# Pipeline v2, 75M in Phase 2e -- so the stop is mechanical here instead. This
# gate is already run by Hermes at every turn boundary and auto-pauses the goal
# after 3 consecutive failures, so exiting non-zero once the budget is passed
# pauses the run BY ITSELF, with no agent judgement involved. That is the whole
# point: the rule it replaces was one an agent had to remember to enforce against
# itself, at the moment it is least inclined to stop working.
#
# The figure is measured exactly as docs/TOPICS.md section 7 defines a topic's
# spend: the sum of every token column in `session_model_usage`, for the Lead
# session plus every child spawned during the topic, found by walking
# `parent_session_id` recursively. The Lead session is $HERMES_SESSION_ID, which
# Hermes itself puts into the agent's environment, so no session argument is
# needed and there is nothing for a caller to keep in sync.
#
# IT FAILS CLOSED, and this is the one place in this file that does. A limit was
# requested, so if the spend cannot be measured then the stop condition is not
# being enforced -- and that must not read as "under budget". A missing session
# id, a missing or unreadable state.db, or a session with no usage rows all exit
# 4 with the reason. With no --spend-limit, nothing about spend is measured or
# reported and this whole section is inert.
#
# ON ERRORS, AND A DELIBERATE CHOICE: for every check EXCEPT the spend limit the
# plan says exit non-zero ONLY for the conditions below, so an inability to
# determine state (gh not authed, no network) exits 0 -- with a loud WARNING on
# stderr and on stdout. That is the literal instruction and it is the safe
# reading for a goal loop, since the alternative is pausing a run over a
# transient hiccup. It is, however, the same shape as a silent pass, so the
# warning is deliberately unmissable and the choice is recorded in the PR.
# Flagged rather than hidden. The spend limit is the deliberate exception, for
# the reason given above: it is opt-in, and an opt-in stop that does not work
# must not look like one that does.

set -uo pipefail

REPO_FALLBACK="ItzMuff1n/atrium"

usage() {
  echo "usage: $0 <milestone> [--repo OWNER/NAME] [--spend-limit N]" >&2
  echo "  e.g. $0 'Pipeline v2' --spend-limit 75M" >&2
  echo "  N is tokens: plain digits or a K/M/G suffix (75000000, 75M, 116.5M)." >&2
}

MILESTONE=""
REPO=""
SPEND_LIMIT=""

# Parse a token count with an optional K/M/G suffix into a plain integer.
# Prints the integer and returns 0, or prints nothing and returns 1.
#
# NO EXTERNAL TOOL AT ALL -- no python3, no sed, no tr, nothing. Everything else
# in this file that parses anything shells out to python3, but this does not, and
# the reason is the ordering property stated in section 0: the spend stop must
# still be enforced on a host where the rest of the script cannot run. Measured:
# an earlier version of this function called python3, and with PATH emptied a
# `--spend-limit 75M` was rejected as "not a token count" -- a stop condition
# reporting itself as a typo. A second version removed python3 but still called
# `tr`, which failed the same test the same way; that is why the claim here is the
# literal one and not "no python3". A stop that can be defeated by a missing
# utility is not a stop.
#
# A fractional part finer than the multiplier can express is TRUNCATED, not
# rounded: measured, `1.2345678M` is 1234567 and `1.9999999M` is 1999999, not
# 2000000. Stated because it is silent otherwise. At the scale a budget is set on
# it cannot matter, but it is real behaviour and not an accident.
parse_tokens() {
  local s mult=1 num frac digits scaled=0
  s="$1"
  # Trim surrounding SPACES (not tabs) with parameter expansion alone.
  #
  # Written as an explicit space rather than a `[[:space:]]` bracket class in the
  # pattern, because that class inside `${s#...}` did not behave as the class did
  # inside a `case` pattern in testing here -- the same construct stripped a
  # trailing space but not a leading one. Rather than leave a subtlety in a stop
  # condition, this uses the form that was verified directly.
  while [ -n "$s" ] && [ "${s# }" != "$s" ]; do s="${s# }"; done
  while [ -n "$s" ] && [ "${s% }" != "$s" ]; do s="${s% }"; done
  [ -n "$s" ] || return 1
  case "$s" in
    *[Kk]) mult=1000;       num="${s%?}" ;;
    *[Mm]) mult=1000000;    num="${s%?}" ;;
    *[Gg]) mult=1000000000; num="${s%?}" ;;
    *)     num="$s" ;;
  esac
  # Reject anything that is not digits, with at most one dot and at least one
  # digit before it. `.5M` is rejected along with `5.M`: neither is a token count
  # anyone writes, and accepting one while rejecting the other would be arbitrary.
  case "$num" in
    ''|*[!0-9.]*) return 1 ;;
    .*)           return 1 ;;
    *.*.*)        return 1 ;;
    *.)           return 1 ;;
  esac
  case "$num" in
    *.*)
      frac="${num#*.}"; num="${num%%.*}"
      case "$frac" in
        ''|*[!0-9]*) return 1 ;;
      esac
      ;;
    *) frac="" ;;
  esac
  case "$num" in *[!0-9]*) return 1 ;; esac
  [ -n "$num" ] || return 1
  case "$mult" in
    1)          digits=0 ;;
    1000)       digits=3 ;;
    1000000)    digits=6 ;;
    1000000000) digits=9 ;;
  esac
  if [ "$digits" -gt 0 ] && [ -n "$frac" ]; then
    frac="${frac:0:$digits}"
    while [ "${#frac}" -lt "$digits" ]; do frac="${frac}0"; done
    scaled="$((10#$frac))"
  fi
  # 10# forces base 10: a bare leading zero would otherwise be read as octal.
  printf '%s\n' "$(( 10#$num * mult + scaled ))"
}

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
    --spend-limit)
      # Same argument-count guard as --repo, for the same reason.
      if [ $# -lt 2 ]; then
        echo "goal-gate: --spend-limit needs a value (e.g. 75M)" >&2
        usage
        exit 3
      fi
      if ! SPEND_LIMIT="$(parse_tokens "$2")" || [ -z "$SPEND_LIMIT" ]; then
        echo "goal-gate: --spend-limit '$2' is not a token count." >&2
        echo "goal-gate: expected plain digits or a K/M/G suffix, e.g. 75000000 or 75M." >&2
        usage
        exit 3
      fi
      shift 2 ;;
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

# ---------------------------------------------------------------------------
# 0. The spend limit, when one was asked for. Runs FIRST, and before any gh use.
#
#    Ordering is deliberate, twice over:
#
#      * it needs no network and no `gh`, so it must not sit behind the
#        `command -v gh` bail-out below -- on a machine without gh the spend
#        limit would silently stop existing, which is the exact failure this
#        feature is for;
#      * it must not sit behind the premature-finish (1) or red-main (2) exits
#        either, or an over-budget run whose closing issue happened to be open
#        would report exit 1 and the budget stop would never be the reason.
#
#    Every failure below exits 4 rather than warning and continuing. See the
#    header: an opt-in stop that cannot be evaluated has not been enforced, and
#    "could not check" must not read as "under budget".
#
#    ONE THING THIS CANNOT DO, stated rather than implied: it measures the
#    SESSION, so it sees spend only once Hermes has written it to state.db. On
#    the first turn boundary of a brand-new session there may be nothing written
#    yet, and the gate will exit 4 rather than measure zero. That is the
#    fail-closed direction chosen on purpose -- a spurious pause is visible,
#    resumable (`/goal resume`) and costs one interruption, while a stop
#    condition that quietly does not fire costs the overrun this exists to
#    prevent. Named here so it is not discovered as a surprise.
# ---------------------------------------------------------------------------
if [ -n "$SPEND_LIMIT" ]; then
  HERMES_DIR="${HERMES_HOME:-$HOME/.hermes}"
  STATE_DB="$HERMES_DIR/state.db"

  if [ -z "${HERMES_SESSION_ID:-}" ]; then
    printf 'goal-gate: OVER BUDGET CHECK FAILED -- a --spend-limit was requested but HERMES_SESSION_ID is not set,\n' >&2
    printf 'goal-gate:   so the topic session cannot be identified and the spend WAS NOT MEASURED.\n' >&2
    printf 'goal-gate:   Exiting 4 (fail closed): an unenforced stop must not read as "under budget".\n' >&2
    printf 'goal-gate: FAIL -- spend limit %s was requested and could not be checked (no HERMES_SESSION_ID).\n' "$SPEND_LIMIT"
    exit 4
  fi

  if [ ! -r "$STATE_DB" ]; then
    printf 'goal-gate: OVER BUDGET CHECK FAILED -- no readable session database at %s.\n' "$STATE_DB" >&2
    printf 'goal-gate:   Exiting 4 (fail closed): the stop condition could not be enforced.\n' >&2
    printf 'goal-gate: FAIL -- spend limit %s was requested and could not be checked (unreadable %s).\n' \
      "$SPEND_LIMIT" "$STATE_DB"
    exit 4
  fi

  # The measurement, in one place: docs/TOPICS.md section 7 -- the sum of every
  # token column in session_model_usage, over the Lead session plus every child
  # spawned during the topic, found by walking parent_session_id recursively.
  #
  # reasoning_tokens is included because section 7 says "every token column". It
  # is 0 in every row in this database as at 26 Sep 2026, so it changes no figure
  # today; it is summed anyway rather than quietly omitted, because a column that
  # starts being populated later must not silently go uncounted.
  spend_out=""
  if ! spend_out="$(LEAD_ID="$HERMES_SESSION_ID" DB="$STATE_DB" python3 - <<'PY'
import os, sqlite3, sys

lead = os.environ["LEAD_ID"]
db = os.environ["DB"]

try:
    con = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
except Exception as exc:
    print(f"ERR open {type(exc).__name__}: {exc}")
    raise SystemExit(0)

try:
    # sessions that exist, and the recursive set of the Lead plus its descendants
    cur = con.execute("SELECT 1 FROM sessions WHERE id = ?", (lead,))
    if cur.fetchone() is None:
        print(f"ERR unknown-session {lead} is not in this database")
        raise SystemExit(0)

    tree = [lead]
    frontier = [lead]
    while frontier:
        marks = ",".join("?" * len(frontier))
        rows = con.execute(
            f"SELECT id FROM sessions WHERE parent_session_id IN ({marks})",
            frontier,
        ).fetchall()
        frontier = [r[0] for r in rows]
        tree.extend(frontier)

    marks = ",".join("?" * len(tree))
    row = con.execute(
        f"SELECT COUNT(*), "
        f"COALESCE(SUM(input_tokens),0), "
        f"COALESCE(SUM(output_tokens),0), "
        f"COALESCE(SUM(cache_read_tokens),0), "
        f"COALESCE(SUM(cache_write_tokens),0), "
        f"COALESCE(SUM(reasoning_tokens),0) "
        f"FROM session_model_usage WHERE session_id IN ({marks})",
        tree,
    ).fetchone()
except Exception as exc:
    print(f"ERR query {type(exc).__name__}: {exc}")
    raise SystemExit(0)

rows, i, o, cr, cw, rz = row
total = i + o + cr + cw + rz
print(f"OK {rows} {len(tree)} {total} {i} {o} {cr} {cw} {rz}")
PY
  )"; then
    printf 'goal-gate: OVER BUDGET CHECK FAILED -- reading the spend raised an error.\n' >&2
    printf 'goal-gate:   Exiting 4 (fail closed): the stop condition could not be enforced.\n' >&2
    printf 'goal-gate: FAIL -- spend limit %s was requested and could not be checked.\n' "$SPEND_LIMIT"
    exit 4
  fi

  case "$spend_out" in
    OK\ *)
      set -- $spend_out
      s_rows="$2" s_sessions="$3" s_total="$4"
      s_in="$5" s_out="$6" s_cr="$7" s_cw="$8" s_rz="$9"
      if [ "${s_rows:-0}" -eq 0 ]; then
        # A limit was requested and the session tree holds no usage rows at all.
        # Reported as a measurement gap, NOT as zero -- see the fail-closed note
        # in the header, and the known first-turn case described above.
        printf 'goal-gate: OVER BUDGET CHECK FAILED -- no usage rows exist yet for session %s\n' "$HERMES_SESSION_ID" >&2
        printf 'goal-gate:   (or its children), so the spend could not be measured. This is a measurement gap,\n' >&2
        printf 'goal-gate:   not an overrun. Exiting 4 (fail closed) so it cannot read as "under budget".\n' >&2
        printf 'goal-gate:   If this is the first turn of a fresh session, /goal resume once usage has landed.\n' >&2
        printf 'goal-gate: FAIL -- spend limit %s was requested and nothing could be measured.\n' "$SPEND_LIMIT"
        exit 4
      fi
      if [ "$s_total" -gt "$SPEND_LIMIT" ]; then
        printf 'goal-gate: FAIL -- OVER BUDGET: the topic has spent %s tokens, past the limit of %s.\n' \
          "$s_total" "$SPEND_LIMIT"
        printf 'goal-gate:   session %s plus %s child session(s); %s usage row(s).\n' \
          "$HERMES_SESSION_ID" "$((s_sessions - 1))" "$s_rows"
        printf 'goal-gate:   in %s  out %s  cache-read %s  cache-write %s  reasoning %s\n' \
          "$s_in" "$s_out" "$s_cr" "$s_cw" "$s_rz"
        printf 'goal-gate: the stop condition has fired; this run is stopping rather than continuing to the cap.\n'
        exit 4
      fi
      printf 'goal-gate: spend %s of %s tokens (session %s + %s child session(s), %s usage row(s)) -- under budget.\n' \
        "$s_total" "$SPEND_LIMIT" "$HERMES_SESSION_ID" "$((s_sessions - 1))" "$s_rows"
      ;;
    ERR\ *)
      printf 'goal-gate: OVER BUDGET CHECK FAILED -- %s\n' "${spend_out#ERR }" >&2
      printf 'goal-gate:   Exiting 4 (fail closed): the stop condition could not be enforced.\n' >&2
      printf 'goal-gate: FAIL -- spend limit %s was requested and could not be checked.\n' "$SPEND_LIMIT"
      exit 4
      ;;
    *)
      printf 'goal-gate: OVER BUDGET CHECK FAILED -- the measurement returned nothing usable.\n' >&2
      printf 'goal-gate:   Exiting 4 (fail closed): the stop condition could not be enforced.\n' >&2
      printf 'goal-gate: FAIL -- spend limit %s was requested and could not be checked.\n' "$SPEND_LIMIT"
      exit 4
      ;;
  esac
fi

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
