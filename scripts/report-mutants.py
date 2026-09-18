#!/usr/bin/env python3
"""Collect the missed mutants from a sharded cargo-mutants run and file them as
a GitHub issue.

Called by .github/workflows/mutants.yml after every shard has uploaded its
mutants.out directory. Each shard ran `cargo mutants --shard k/4`, so no shard
saw the whole picture; this script is the single place that reads them all and
decides whether an issue is opened, commented on, or not filed at all.

Why it is a script and not inline shell: the findings are JSON inside
outcomes.json, and the decision -- open a new issue vs. comment on an existing
one -- has to be made once, consistently, rather than four times by four shards.

Exit code discipline (the lesson from PR #15, where a lost crash report passed
as a green step): this exits non-zero whenever a report that SHOULD have been
filed was not. `main()` writes a status line to issue-status.txt on every
successful path, and the workflow fails the step if that file is empty or the
exit code is non-zero.

Deliberately NOT a failure: a run that produced no results at all (every shard
died before testing). The shard steps already fail the run in that case; blaming
the reporter as well would only obscure which part broke.
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

# The aggregate tracker carries `mutation-report`; the per-file child issues the
# queue splits it into carry `mutation`. Keeping the two apart is what lets this
# script find its own tracker without ever mistaking a child for it -- a child is
# a small specific job, a tracker is the run-level summary.
ISSUE_LABEL = "mutation-report"
ISSUE_TITLE = "mutation testing: mutants not covered by tests"

# "crates/fileops/src/lib.rs:71:9: replace parent_lexical -> String with ..."
# The mutant name is "<path>:<line>:<col>: <description>".
NAME_RE = re.compile(r"^(?P<loc>.+?:\d+:\d+):\s*(?P<desc>.+)$")


def run_gh(args: list[str]) -> tuple[int, str]:
    """Run a gh command and return (exit code, stdout+stderr)."""
    p = subprocess.run(["gh", *args], capture_output=True, text=True)
    return p.returncode, (p.stdout or "") + (p.stderr or "")


def collect(results_dir: Path, expected_shards: list[int]) -> tuple[list[dict], dict, list[str], bool]:
    """Read every shard's outcomes.json.

    Returns (missed, totals, notes, found_any). `notes` records anything a reader
    would otherwise have to guess at. `found_any` is False when not one shard
    produced an outcomes.json -- the caller must treat that as a failure to
    collect rather than as a clean run, because the two are not the same claim.

    A shard with no outcomes.json is not automatically a failure: a shard
    assigned zero mutants creates a mutants.out directory but no outcomes.json
    (observed locally, cargo-mutants 27.1.0 -- it exits 0 after printing "WARN
    No mutants found under the active filters"). Because `expected_shards` is
    known from the matrix, a missing shard can be named explicitly rather than
    silently omitted, which is the difference between a partial run read as
    clean and a partial run read as partial.
    """
    missed: list[dict] = []
    totals = {
        "total_mutants": 0,
        "caught": 0,
        "missed": 0,
        "timeout": 0,
        "unviable": 0,
    }
    notes: list[str] = []

    outcomes = sorted(results_dir.rglob("outcomes.json"))
    reported_shards: list[str] = []
    for path in outcomes:
        # The shard identity comes from the path itself, because the artifact
        # layout is not something this script should have to assume. Each shard
        # uploads under the artifact name `mutants-shard-<k>`, and the downloaded
        # path contains that name; taking it from the path means a change to how
        # the artifact is wrapped cannot silently make this script read the wrong
        # shard or, worse, read nothing at all.
        shard_dir = next(
            (part for part in path.parts if part.startswith("mutants-shard-")),
            path.parent.parent.name,
        )
        reported_shards.append(shard_dir)
        try:
            data = json.loads(path.read_text())
        except (OSError, json.JSONDecodeError) as exc:
            notes.append(f"{shard_dir}: outcomes.json could not be read ({exc})")
            continue

        for key in totals:
            totals[key] += int(data.get(key) or 0)

        if int(data.get("timeout") or 0):
            notes.append(
                f"{shard_dir}: {data['timeout']} mutant(s) TIMED OUT -- those results "
                "are inconclusive, not clean"
            )

        for outcome in data.get("outcomes") or []:
            if outcome.get("summary") != "MissedMutant":
                continue
            scenario = outcome.get("scenario") or {}
            mutant = scenario.get("Mutant") or {}
            name = mutant.get("name") or "(unnamed mutant)"
            m = NAME_RE.match(name)
            missed.append(
                {
                    "name": name,
                    "location": m.group("loc") if m else name,
                    "description": m.group("desc") if m else "",
                    "file": mutant.get("file") or "",
                    "shard": shard_dir,
                }
            )

    # No results at all. This is reported to the caller as found_any=False and the
    # caller fails on it. The reason is the whole point of the negative proof: on
    # run 35400933078 the shards found the planted mutants and exited 2, but the
    # artifact layout did not match this script's older glob, so it saw nothing,
    # wrote "no results to report", and exited 0 -- a lost report on a green run,
    # which is exactly what the reporting discipline here exists to prevent.
    # "I could not find any results" and "there were no findings" are different
    # statements, and only one of them is safe to make without evidence.
    if not outcomes:
        print(
            "report: no shard produced an outcomes.json, so nothing could be "
            "collected. That is NOT the same as a clean run -- check each shard's "
            "log in this run."
        )
        return missed, totals, notes, False

    # Name the shards that reported nothing. In a small scope this is expected;
    # in a large one it means a shard died, and either way the reader should not
    # have to diff shard numbers against the matrix to notice.
    for expected in expected_shards:
        if not any(s.endswith(f"mutants-shard-{expected}") for s in reported_shards):
            notes.append(
                f"shard {expected} reported no outcomes.json -- either it was "
                "assigned no mutants, or it did not finish. Check its log before "
                "concluding the run was clean."
            )

    missed.sort(key=lambda d: d["name"])
    print(
        f"report: read {len(reported_shards)} shard(s): {', '.join(sorted(reported_shards))}"
    )
    print(
        "report: totals across shards -- "
        f"total={totals['total_mutants']} caught={totals['caught']} "
        f"missed={totals['missed']} unviable={totals['unviable']} timeout={totals['timeout']}"
    )
    if len(missed) != totals["missed"]:
        notes.append(
            f"the collected list has {len(missed)} entries but the shards' own "
            f"counts sum to {totals['missed']} -- treat the list as incomplete"
        )
    return missed, totals, notes, True


def build_body(missed: list[dict], totals: dict, notes: list[str], run_url: str) -> str:
    lines = [
        "cargo-mutants broke the code in small ways and the test suite still passed.",
        "Each line below is a change no test noticed, which means that behaviour is",
        "not covered by anything.",
        "",
        f"- run: {run_url}",
        f"- mutants tested: {totals['total_mutants']}",
        f"- caught: {totals['caught']}",
        f"- **missed: {totals['missed']}**",
        f"- unviable (would not compile, not a finding): {totals['unviable']}",
        f"- timed out (inconclusive): {totals['timeout']}",
        "",
        "## Missed mutants",
        "",
    ]
    for m in missed:
        desc = f" — {m['description']}" if m["description"] else ""
        lines.append(f"- `{m['location']}`{desc}")

    if notes:
        lines += ["", "## Notes", ""]
        lines += [f"- {n}" for n in notes]

    lines += [
        "",
        "---",
        "",
        "To reproduce locally:",
        "",
        "```",
        "cargo mutants -p atrium-resolver -p atrium-fileops -p atrium-shell",
        "```",
        "",
        "Scope and exclusions are in `.cargo/mutants.toml`; read the comment there",
        "before treating a number here as a to-do list.",
        "",
        "Filed automatically by `.github/workflows/mutants.yml`.",
    ]
    return "\n".join(lines)


def existing_issue() -> tuple[bool, int | None]:
    """Look for an open issue carrying the mutation label.

    Returns (lookup_ok, number_or_None). The two failure modes this separates
    are NOT the same thing and must not be conflated: "listing succeeded and
    there is no issue yet" means open one, while "listing failed" means we do
    not know, and opening a duplicate on a failed lookup is the worse mistake.
    Collapsing both to None makes the first-ever run exit non-zero instead of
    filing its first issue.
    """
    rc, out = run_gh(
        ["issue", "list", "--state", "open", "--label", ISSUE_LABEL, "--json", "number"]
    )
    if rc != 0:
        print(f"report: could not list issues: {out.strip()}")
        return False, None
    try:
        issues = json.loads(out or "[]")
    except json.JSONDecodeError:
        print(f"report: issue list was not JSON: {out.strip()[:200]}")
        return False, None
    if not issues:
        return True, None
    return True, issues[0]["number"]


def write(path: str, text: str) -> None:
    Path(path).write_text(text)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--results", default="mutants-results")
    ap.add_argument("--run-url", default="")
    ap.add_argument(
        "--expected-shards",
        default="0,1,2,3",
        help=(
            "comma-separated shard indices the matrix should have run. Must match "
            "the workflow's matrix, or a shard that silently produced nothing will "
            "not be reported as missing."
        ),
    )
    args = ap.parse_args()

    results_dir = Path(args.results)
    expected_shards = [int(x) for x in args.expected_shards.split(",") if x.strip()]
    missed, totals, notes, found_any = collect(results_dir, expected_shards)

    # Write the list out regardless of what happens with the issue, so the
    # workflow's step summary and the uploaded artifact carry it either way.
    summary = "\n".join(
        f"{m['location']}" + (f" — {m['description']}" if m["description"] else "")
        for m in missed
    )
    # Trailing newline: without it `wc -l` reports one fewer line than there are
    # entries, which reads as a dropped mutant to anyone checking the artifact.
    write("mutants-summary.txt", summary + "\n" if summary else "")

    # Nothing was collected at all. Fail, because the honest statement is "I
    # could not read the results", not "there were no findings". This is the
    # negative-proof defect: the shards found uncovered mutants and exited 2
    # while the reporter saw an empty directory and returned success.
    if not found_any:
        write(
            "issue-status.txt",
            "FAILED: no outcomes.json found in any shard -- check the shard logs "
            "before trusting this run",
        )
        return 1

    # A clean week: nothing to file, and nothing to comment.
    if not missed:
        write(
            "issue-status.txt",
            f"no uncovered mutants ({totals['total_mutants']} tested across all shards)",
        )
        print("report: no missed mutants; no issue needed")
        return 0

    body = build_body(missed, totals, notes, args.run_url)

    # Notes belong in the run log as well as the issue. A note is exactly the
    # thing a reader of the log would otherwise have to infer -- a shard that
    # reported nothing, a timeout, a list that does not match the shard counts --
    # and burying it in an issue body nobody opens on a clean-ish week defeats it.
    for note in notes:
        print(f"report: NOTE {note}")

    lookup_ok, number = existing_issue()
    if not lookup_ok:
        # existing_issue() already printed why. We cannot tell "no issue exists"
        # from "listing failed", and opening a duplicate issue on a failed
        # listing is the worse mistake -- report the failure instead.
        print("report: could NOT determine whether an issue already exists")
        return 1

    if number:
        rc, out = run_gh(
            ["issue", "comment", str(number), "--body", body]
        )
        if rc != 0:
            print(f"report: could NOT comment on issue #{number}: {out.strip()}")
            return 1
        write("issue-status.txt", f"commented on existing issue #{number}")
        print(f"report: commented on existing issue #{number}")
        return 0

    rc, out = run_gh(
        [
            "issue",
            "create",
            "--title",
            f"{ISSUE_TITLE} ({len(missed)})",
            "--label",
            ISSUE_LABEL,
            "--body",
            body,
        ]
    )
    if rc != 0:
        print(f"report: could NOT open an issue: {out.strip()}")
        return 1
    url = out.strip().splitlines()[-1] if out.strip() else "(no url printed)"
    write("issue-status.txt", f"opened a new issue: {url}")
    print(f"report: opened a new issue: {url}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
