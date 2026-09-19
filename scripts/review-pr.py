#!/usr/bin/env python3
"""Review a pull request with an LLM and post the findings as a PR comment.

Called by .github/workflows/review.yml on every pull_request. The review is
one call to an OpenAI-compatible chat endpoint (Ollama Cloud, model glm-5.3)
with temperature 0, and the model is required to answer with JSON only.

Why the decision lives in a script rather than in YAML: the same reason
report-mutants.py and report-crash.py exist. The logic has several branches --
dependabot, a missing secret, an oversized diff, an unparseable answer -- and
each of them has to end in a defensible exit code exactly once, not once per
copy in a shell step.

Exit code discipline (the lesson from PR #15 and #19, where a lost report
passed as a green step): this exits non-zero whenever the review could not be
completed, as well as when the review says fail. "I could not review this" and
"this passed review" are different statements, and only one of them is safe to
make without evidence.

Deliberately NOT a failure: a dependabot PR. It is skipped, which is recorded
in a comment so the absence of a review is visible rather than silent.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import urllib.error
import urllib.request
from typing import Any

DEFAULT_API_URL = "https://ollama.com/v1/chat/completions"
DEFAULT_MODEL = "glm-5.3"

# The size limit, stated so it can be argued with: 100,000 bytes of diff.
# Every PR merged into this repository so far is between 5,600 and 28,100 bytes
# of diff (measured 19 Sep 2026: #26 5632, #25 5782, #28 7174, #19 7791,
# #27 8116, #18 28125), so the limit is about 3.5x the largest real PR here.
# It is not there to be generous; it is there so a runaway PR fails loudly
# instead of being reviewed badly. A review that silently truncates its input
# is worse than no review, because the findings look complete.
MAX_DIFF_BYTES = 100_000

INSTRUCTIONS = """You are reviewing a pull request in a Rust workspace. You are the \
only reviewer this project gets: the human owner does not read code, so a \
weak change that passes here will ship.

Reject the pull request if it does any of these:

1. Weakens or deletes an assertion, test, or check instead of fixing what it \
tests. A test edited so it can no longer fail is a failure, not a fix.
2. Changes files outside the scope of the issue it says it closes. The linked \
issue's text is the scope; anything not needed for it is out of scope.
3. Changes source code for the sole purpose of killing a mutation-testing \
mutant. A missed mutant means a test is missing; the fix belongs in the tests.
4. Contains a workaround, bypass, or exception in place of a real fix -- \
disabling a check, special-casing an input to dodge a rule, or catching an \
error to hide it.
5. Introduces or exposes a secret, credential, token, or API key.

Answer with JSON ONLY. No prose, no markdown fence, no explanation outside the \
JSON. The exact shape:

{"verdict": "pass" or "fail", "findings": ["one sentence per finding", ...]}

Verdict "pass" requires findings to be an empty list. Verdict "fail" requires \
at least one finding, and each finding must name the file and say what is \
wrong. If you are unsure, that is a fail with the uncertainty as the finding.
"""


class ReviewError(Exception):
    """A review that could not be completed. Always a non-zero exit."""


def run_gh(args: list[str]) -> tuple[int, str]:
    p = subprocess.run(["gh", *args], capture_output=True, text=True)
    return p.returncode, (p.stdout or "") + (p.stderr or "")


def gh_json(args: list[str]) -> Any:
    rc, out = run_gh(args)
    if rc != 0:
        raise ReviewError(f"gh {' '.join(args)} failed: {out.strip()}")
    try:
        return json.loads(out or "null")
    except json.JSONDecodeError as exc:
        raise ReviewError(f"gh {' '.join(args)} did not return JSON: {exc}") from exc


def pr_author(pr: str) -> str:
    data = gh_json(["pr", "view", pr, "--json", "author"])
    author = (data or {}).get("author") or {}
    return author.get("login") or ""


def pr_diff(pr: str) -> str:
    rc, out = run_gh(["pr", "diff", pr])
    if rc != 0:
        raise ReviewError(f"could not read the diff for PR {pr}: {out.strip()}")
    if not out.strip():
        raise ReviewError(f"PR {pr} has an EMPTY diff -- refusing to review nothing")
    return out


def closing_issues(pr: str) -> list[dict]:
    """The issues this PR closes, with their bodies.

    Taken from GitHub's own `closingIssuesReferences` rather than by parsing
    "Fixes #N" out of the body. The native field is what GitHub itself uses to
    close issues, so the review's notion of scope cannot drift from the one
    that actually applies. If a PR body says "Fixes #5" in a way GitHub does
    not honour, that is not scope, and treating it as scope would let a PR
    claim coverage it does not have.
    """
    data = gh_json(["pr", "view", pr, "--json", "closingIssuesReferences"])
    refs = (data or {}).get("closingIssuesReferences") or []
    issues = []
    for ref in refs:
        number = ref.get("number")
        if number is None:
            continue
        detail = gh_json(["issue", "view", str(number), "--json", "number,title,body"])
        issues.append(detail or {"number": number})
    return issues


def build_prompt(pr: str, diff: str, issues: list[dict]) -> str:
    parts = [INSTRUCTIONS, "", f"# Pull request {pr}", "", "## Diff", "", "```diff", diff, "```", ""]
    if issues:
        parts += ["## The issue(s) this PR closes (the scope)", ""]
        for iss in issues:
            parts += [
                f"### #{iss.get('number')}: {iss.get('title') or '(no title)'}",
                "",
                (iss.get("body") or "(no body)").strip(),
                "",
            ]
    else:
        parts += [
            "## The issue(s) this PR closes (the scope)",
            "",
            "NONE. This pull request does not close any issue, so there is no "
            "stated scope to check the diff against. Treat any substantive "
            "behavioural change as a finding for that reason.",
            "",
        ]
    return "\n".join(parts)


def ask_model(prompt: str, model: str, api_url: str, key: str) -> str:
    """One call to the model, with one retry on a transport failure.

    The timeout and the retry are both here for a measured reason, not caution
    in the abstract. glm-5.3 is a reasoning model: it emits a `reasoning` field
    before its answer, so latency is dominated by thinking rather than by prompt
    size. Measured direct from a workstation on 19 Sep 2026, a 2,007-byte prompt
    took 19.7s and a 16,596-byte one took 32.5s.

    In CI the same call exceeded 180s and the step failed with "could not reach
    the model endpoint: The read operation timed out" (run 35409248445, on a
    1,015-byte diff -- the SMALLEST input it was given). Failing closed was
    correct, but a check that times out on small inputs and blocks every PR
    behind `current_user_can_bypass: never` is a check that has to be disabled,
    so the timeout is 600s and a single transport retry absorbs a transient
    stall. A timeout is still a failure -- there is no pass-on-error path.
    """
    body = json.dumps(
        {
            "model": model,
            "temperature": 0,
            "messages": [{"role": "user", "content": prompt}],
        }
    ).encode()
    req = urllib.request.Request(
        api_url,
        data=body,
        headers={
            "Authorization": f"Bearer {key}",
            "Content-Type": "application/json",
        },
        method="POST",
    )
    last: Exception | None = None
    for attempt in (1, 2):
        try:
            with urllib.request.urlopen(req, timeout=600) as resp:
                payload = json.loads(resp.read().decode())
            break
        except urllib.error.HTTPError as exc:
            detail = exc.read().decode(errors="replace")[:500]
            # A 5xx is worth one retry; a 4xx is a request problem that will
            # fail identically the second time, so it is raised immediately.
            if exc.code < 500 or attempt == 2:
                raise ReviewError(
                    f"the model endpoint returned HTTP {exc.code}: {detail}"
                ) from exc
            last = exc
        except (urllib.error.URLError, TimeoutError, OSError) as exc:
            last = exc
            if attempt == 2:
                raise ReviewError(f"could not reach the model endpoint: {exc}") from exc
        except json.JSONDecodeError as exc:
            raise ReviewError(f"the model endpoint did not return JSON: {exc}") from exc
    else:
        # Both attempts failed on a transport error. `last` is set on every path
        # that reaches here, but the explicit fallback avoids a None leaking into
        # the message if that ever stops being true.
        raise ReviewError(f"could not reach the model endpoint: {last}")
    try:
        return payload["choices"][0]["message"]["content"]
    except (KeyError, IndexError, TypeError) as exc:
        raise ReviewError(f"the model response had no message content: {exc}") from exc


def parse_verdict(content: str) -> tuple[str, list[str]]:
    """Pull the required JSON object out of the model's answer.

    An unparseable answer is a FAIL, never a pass and never a skip. A reviewer
    that cannot state its verdict has not cleared the change, and treating
    silence as approval is the one failure mode that makes the whole gate
    worthless.

    A fenced block is tolerated because it carries no ambiguity about intent
    (the JSON is still there and still says one thing); anything else that does
    not parse is a failure.
    """
    text = (content or "").strip()
    fenced = re.search(r"```(?:json)?\s*(.*?)```", text, re.DOTALL)
    if fenced:
        text = fenced.group(1).strip()

    start, end = text.find("{"), text.rfind("}")
    if start == -1 or end == -1 or end < start:
        raise ReviewError(f"the model did not answer with JSON: {content[:400]!r}")
    try:
        parsed = json.loads(text[start : end + 1])
    except json.JSONDecodeError as exc:
        raise ReviewError(f"the model's answer was not valid JSON ({exc}): {content[:400]!r}")

    if not isinstance(parsed, dict):
        raise ReviewError(f"the model's JSON was not an object: {content[:400]!r}")

    verdict = parsed.get("verdict")
    findings = parsed.get("findings")

    if verdict not in ("pass", "fail"):
        raise ReviewError(
            f"the model's verdict was {verdict!r}, not 'pass' or 'fail': {content[:400]!r}"
        )
    if not isinstance(findings, list) or not all(isinstance(f, str) for f in findings):
        raise ReviewError(f"the model's findings were not a list of strings: {content[:400]!r}")
    if verdict == "pass" and findings:
        raise ReviewError(
            f"the model said 'pass' but listed findings, which contradicts itself: {findings!r}"
        )
    if verdict == "fail" and not findings:
        raise ReviewError("the model said 'fail' but listed no findings, so it said nothing useful")
    return verdict, findings


def comment(pr: str, body: str) -> None:
    rc, out = run_gh(["pr", "comment", pr, "--body", body])
    if rc != 0:
        raise ReviewError(f"could not comment on PR {pr}: {out.strip()}")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--pr", required=True)
    ap.add_argument("--model", default=os.environ.get("REVIEW_MODEL") or DEFAULT_MODEL)
    ap.add_argument("--api-url", default=os.environ.get("REVIEW_API_URL") or DEFAULT_API_URL)
    ap.add_argument("--max-diff-bytes", type=int, default=MAX_DIFF_BYTES)
    args = ap.parse_args()

    key = os.environ.get("OLLAMA_API_KEY") or ""

    try:
        author = pr_author(args.pr)
    except ReviewError as exc:
        print(f"review: FAILED to read the PR author: {exc}")
        return 1

    # dependabot PRs are skipped on purpose: they are machine-generated version
    # bumps, and a model asked to judge them would be reviewing a changelog it
    # cannot verify. The skip is commented so that "no review happened" is
    # visible on the PR rather than indistinguishable from "the job never ran".
    if author == "dependabot[bot]":
        print("review: dependabot PR -- skipping, as configured")
        try:
            comment(
                args.pr,
                "**AI review skipped.** This is a `dependabot[bot]` pull request; "
                "the automated review does not run on dependency bumps.",
            )
        except ReviewError as exc:
            print(f"review: FAILED to comment the skip: {exc}")
            return 1
        return 0

    # Any other author with no key is a failure, not a skip. The review is a
    # required gate; a PR that cannot be reviewed must not merge by default.
    if not key:
        print(f"review: FAILED -- OLLAMA_API_KEY is not set, so PR {args.pr} cannot be reviewed")
        try:
            comment(
                args.pr,
                "**AI review FAILED: `OLLAMA_API_KEY` is not available to this "
                "workflow run.** The review is a required check, so it fails "
                "rather than passing unreviewed. If this is a forked pull "
                "request, secrets are not exposed to it by design.",
            )
        except ReviewError:
            pass
        return 1

    try:
        diff = pr_diff(args.pr)
        issues = closing_issues(args.pr)
    except ReviewError as exc:
        print(f"review: FAILED to gather the input: {exc}")
        return 1

    size = len(diff.encode())
    print(f"review: diff is {size} bytes; limit is {args.max_diff_bytes}")
    print(f"review: the PR closes {len(issues)} issue(s)")

    if size > args.max_diff_bytes:
        body = (
            f"**AI review failed: split this PR.**\n\n"
            f"The diff is **{size} bytes**, over the limit of "
            f"{args.max_diff_bytes} bytes. A review of a diff this size would be "
            f"unreliable, and an unreliable review that reports findings reads as "
            f"complete. Split it into PRs that can each be reviewed on their own."
        )
        try:
            comment(args.pr, body)
        except ReviewError as exc:
            print(f"review: FAILED to comment the size refusal: {exc}")
            return 1
        print("review: FAIL -- over the size limit")
        return 1

    prompt = build_prompt(args.pr, diff, issues)
    try:
        content = ask_model(prompt, args.model, args.api_url, key)
    except ReviewError as exc:
        print(f"review: FAILED to get a verdict: {exc}")
        return 1

    try:
        verdict, findings = parse_verdict(content)
    except ReviewError as exc:
        # An unparseable answer fails the job AND says so on the PR. A silent
        # non-zero exit would look like an infrastructure blip and be re-run
        # until it happened to pass, which is the same as having no gate.
        print(f"review: FAILED to parse a verdict: {exc}")
        try:
            comment(
                args.pr,
                "**AI review FAILED: the model's answer could not be parsed.**\n\n"
                "A verdict that cannot be read is not a pass. The raw answer is "
                "in the workflow log. Re-run once to rule out a transient fault; "
                "if it recurs, the prompt or the model needs fixing before this "
                "gate can be trusted.",
            )
        except ReviewError:
            pass
        return 1

    if verdict == "pass":
        body = (
            f"**AI review: pass.**\n\n"
            f"Reviewed by `{args.model}` at temperature 0, over a "
            f"{len(issues)}-issue scope and a {size}-byte diff. No findings."
        )
        try:
            comment(args.pr, body)
        except ReviewError as exc:
            print(f"review: FAILED to comment the pass: {exc}")
            return 1
        print("review: PASS")
        return 0

    lines = "\n".join(f"- {f}" for f in findings)
    body = (
        f"**AI review: fail.**\n\n"
        f"Reviewed by `{args.model}` at temperature 0, over a "
        f"{len(issues)}-issue scope and a {size}-byte diff.\n\n"
        f"## Findings\n\n{lines}\n\n"
        f"<!-- This check fails until the findings are addressed or rebutted. "
        f"A rebuttal belongs in a reply to this comment, not in a re-run. -->"
    )
    try:
        comment(args.pr, body)
    except ReviewError as exc:
        print(f"review: FAILED to comment the findings: {exc}")
        return 1
    print(f"review: FAIL -- {len(findings)} finding(s)")
    return 1


if __name__ == "__main__":
    sys.exit(main())
