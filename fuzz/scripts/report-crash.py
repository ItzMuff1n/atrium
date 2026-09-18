#!/usr/bin/env python3
"""Report a fuzzer crash as a GitHub issue -- one issue per bug class.

Called by `.github/workflows/fuzz.yml` after the run has finished. The workflow
has already done the mechanical part (saved the log, uploaded the artifacts); this
decides what the crash *means* and files it.

    python3 fuzz/scripts/report-crash.py \
        --artifacts fuzz/artifacts/resolve \
        --duration 1200 \
        --run-url https://github.com/.../actions/runs/123

**Deduplication is by panic message, not by input.** A fuzzer that finds one bug
finds it ten thousand times from slightly different inputs; the panic message is
the stable thing, so it is the identity of the bug. The panic messages in
`fuzz_targets/resolve.rs` are deliberately written not to mention the input for
exactly this reason. When an open issue with the same panic message exists, this
comments on it with the new input instead of opening a second issue.

**The minimized input is produced here, by `cargo fuzz tmin`,** because the raw
libFuzzer artifact is usually far larger than it needs to be -- the first crash
found is whatever random soup got there first. Replaying a minimized input is the
difference between a report someone can act on and one they have to triage first.
If `tmin` fails the raw artifact is still reported; a report with a larger input
beats no report.

The verdict is written to `issue-status.txt` in the working directory so the
workflow can put it in the run summary, and the issue URL (when an issue was
touched) to `created-issue-url.txt`.
"""

import argparse
import base64
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))

# A crash artifact from libFuzzer is named `crash-<hash>`; other prefixes
# (`oom-`, `timeout-`, `slow-unit-`) are not escapes in this target, but they are
# still findings and are reported the same way.
CRASH_PREFIXES = ("crash-", "oom-", "timeout-", "leak-", "slow-unit-")

# GitHub's issue body limit is 65,535 characters. Leave room for the rest of the
# body rather than discovering the limit at the API.
MAX_INPUT_CHARS = 20_000

# How long `tmin` may run. Bounded because an artifact that does not actually
# reproduce would otherwise leave tmin searching indefinitely, and the workflow
# would sit there until the job timed out.
TMIN_SECONDS = 120

# Rust's panic output puts the message on the line AFTER the location when the
# panic hook formats it:
#
#     thread '<unnamed>' panicked at fuzz/fuzz_targets/resolve.rs:299:9:
#     resolver oracle: an Ok result canonicalises outside the canonical root
#
# but on the same line when the location is already followed by text. Both forms
# occur in libFuzzer logs, so both are handled.
PANIC_WITH_MESSAGE = re.compile(r"panicked at \S+:\d+:\d+:\s*(.+)$")


def sh(cmd, cwd=None):
    """Run a command, never raising: a failed report must not hide the crash."""
    return subprocess.run(cmd, capture_output=True, text=True, cwd=cwd, check=False)


def find_artifact(artifacts_dir):
    """The crash file libFuzzer left behind, newest first."""
    if not os.path.isdir(artifacts_dir):
        return None
    found = []
    for name in os.listdir(artifacts_dir):
        if name.startswith(CRASH_PREFIXES):
            path = os.path.join(artifacts_dir, name)
            if os.path.isfile(path):
                found.append((os.path.getmtime(path), path))
    if not found:
        return None
    return sorted(found)[-1][1]


def panic_message(log_path):
    """The first panic message in the run log -- the bug's identity."""
    try:
        with open(log_path, "r", errors="replace") as fh:
            text = fh.read()
    except OSError:
        return None
    lines = text.splitlines()
    for i, line in enumerate(lines):
        if "panicked at" not in line:
            continue
        m = PANIC_WITH_MESSAGE.search(line)
        if m and m.group(1).strip():
            return m.group(1).strip()
        # Message on the following line, skipping the hook's own `note:` line.
        for follow in lines[i + 1 : i + 4]:
            if follow.strip() and not follow.strip().startswith("note:"):
                return follow.strip()
    return None


def minimize(artifact):
    """Minimize `artifact` with cargo fuzz tmin. Returns the path to use.

    The artifact is copied first: `tmin` rewrites the file it is given, and the
    original is worth keeping as the artifact that actually crashed.
    """
    work = os.path.join(os.path.dirname(artifact) or ".", "repro")
    try:
        with open(artifact, "rb") as src, open(work, "wb") as dst:
            dst.write(src.read())
    except OSError as exc:
        print("report: could not copy the artifact (%s); using it as-is" % exc)
        return artifact

    # Run from the repo root: cargo-fuzz resolves `fuzz/` relative to the cwd.
    repo = os.path.dirname(os.path.dirname(HERE))
    res = sh(
        [
            "cargo", "+nightly", "fuzz", "tmin", "resolve", work,
            "--", "-max_total_time=%d" % TMIN_SECONDS,
        ],
        cwd=repo,
    )
    if res.returncode != 0 or not os.path.exists(work):
        print("report: tmin failed (exit %d); reporting the raw artifact" % res.returncode)
        return artifact
    print("report: minimized %d bytes -> %d bytes" % (os.path.getsize(artifact), os.path.getsize(work)))
    return work


def printable(data):
    """A readable rendering of the input bytes: ASCII stays, all else is \\xNN."""
    out = []
    for b in data:
        if b == 0x5C:
            out.append("\\\\")
        elif 0x20 <= b <= 0x7E:
            out.append(chr(b))
        else:
            out.append("\\x%02x" % b)
    return "".join(out)


def reproduce_command(run_url, duration):
    """The exact command that reproduces this crash.

    Both forms are given because they answer different questions: the dispatch
    form re-runs the workflow, the local form re-runs the fuzzer. The `-runs=1`
    form is the one that replays the single saved input.
    """
    owner_repo = "/".join(run_url.split("/")[3:5]) if run_url.count("/") >= 4 else "ItzMuff1n/atrium"
    return (
        "Workflow, same as the run that found this (120s is enough in practice; "
        "this crash was found in %ds):\n"
        "```\n"
        "gh workflow run fuzz.yml --repo %s --ref main -f duration=120\n"
        "```\n"
        "\n"
        "Locally, replaying the saved input once:\n"
        "```\n"
        "git clone https://github.com/%s && cd atrium\n"
        "rustup toolchain install nightly\n"
        "cargo install --locked cargo-fuzz --version 0.13.2\n"
        "mkdir -p fuzz/artifacts/resolve\n"
        "# download the artifact from the run below, then:\n"
        "cargo +nightly fuzz run resolve fuzz/artifacts/resolve/<file> -- -runs=1\n"
        "```\n"
        % (duration, owner_repo, owner_repo)
    )


def body_for(panic, data, run_url, duration, artifact_name):
    encoded = base64.b64encode(data).decode()
    shown = printable(data)
    truncated = False
    if len(encoded) > MAX_INPUT_CHARS:
        encoded = encoded[:MAX_INPUT_CHARS]
        truncated = True
    if len(shown) > MAX_INPUT_CHARS:
        shown = shown[:MAX_INPUT_CHARS]
        truncated = True

    return (
        "The nightly fuzzing workflow found a crash in the resolver.\n"
        "\n"
        "**Panic message** (this is the bug's identity; issues are deduplicated by it):\n"
        "\n"
        "```\n"
        "%s\n"
        "```\n"
        "\n"
        "**Run:** [%s](%s)\n"
        "\n"
        "**Minimized input** -- the bytes are the fuzzer's virtual path, fed to "
        "`atrium_resolver::resolve()`. The minimized form is what reproduces; the "
        "original artifact that crashed is in the run's artifacts, and is kept "
        "because it is the evidence.\n"
        "\n"
        "As printable text (non-ASCII bytes as `\\xNN`):\n"
        "\n"
        "```\n"
        "%s\n"
        "```\n"
        "\n"
        "As base64, which is the exact bytes:\n"
        "\n"
        "```\n"
        "%s\n"
        "```\n"
        "\n"
        "%s"
        "**Reproduce**\n"
        "\n"
        "%s"
        % (
            panic,
            run_url,
            run_url,
            shown or "(empty input)",
            encoded or "(empty input)",
            "The input shown above was too long for an issue body and is "
            "truncated; the artifact has the whole thing.\n\n" if truncated else "",
            reproduce_command(run_url, duration),
        )
    )


def existing_issue(title):
    """Number of an open `fuzz` issue with exactly this title, or None."""
    res = sh(
        [
            "gh", "issue", "list",
            "--state", "open",
            "--label", "fuzz",
            "--limit", "100",
            "--json", "number,title",
        ]
    )
    if res.returncode != 0:
        print("report: could not list issues: %s" % res.stderr.strip())
        return None
    import json

    try:
        issues = json.loads(res.stdout or "[]")
    except ValueError:
        return None
    for issue in issues:
        if issue.get("title") == title:
            return issue.get("number")
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--artifacts", required=True)
    ap.add_argument("--duration", type=int, default=0)
    ap.add_argument("--run-url", default="")
    ap.add_argument("--log", default="fuzz-run.log")
    args = ap.parse_args()

    def finish(status, issue_url=""):
        with open("issue-status.txt", "w") as fh:
            fh.write(status + "\n")
        if issue_url:
            with open("created-issue-url.txt", "w") as fh:
                fh.write(issue_url + "\n")
        print("report: %s" % status)

    artifact = find_artifact(args.artifacts)
    if artifact is None:
        # The workflow only calls this when the run failed, but a failure can
        # also be a build error or a timeout, which leaves no artifact. Saying so
        # is more useful than a bare failure.
        finish(
            "No crash artifact was found. The run failed without libFuzzer "
            "recording an input -- a build error, a timeout, or a cancelled run "
            "looks like this. See the run log."
        )
        return 0

    panic = panic_message(args.log)
    if not panic:
        panic = "unknown panic (the run log did not contain a panic line)"

    minimized = minimize(artifact)
    with open(minimized, "rb") as fh:
        data = fh.read()

    title = "fuzz: %s" % panic
    body = body_for(panic, data, args.run_url, args.duration, os.path.basename(minimized))

    number = existing_issue(title)
    if number is not None:
        res = sh(["gh", "issue", "comment", str(number), "--body",
                  "Same panic message again, with a different input.\n\n" + body])
        if res.returncode != 0:
            finish("Found the crash but could NOT comment on issue #%d: %s"
                   % (number, res.stderr.strip()))
            return 0
        url = sh(["gh", "issue", "view", str(number), "--json", "url"]).stdout
        finish("Commented on the existing open issue #%d (same panic message)." % number,
               extract_url(url))
        return 0

    res = sh(["gh", "issue", "create", "--title", title, "--body", body, "--label", "fuzz"])
    if res.returncode != 0:
        finish("Found the crash but could NOT open an issue: %s" % res.stderr.strip())
        return 0
    finish("Opened a new issue.", res.stdout.strip().splitlines()[-1] if res.stdout.strip() else "")
    return 0


def extract_url(gh_json):
    import json

    try:
        return json.loads(gh_json or "{}").get("url", "")
    except ValueError:
        return ""


if __name__ == "__main__":
    sys.exit(main())
