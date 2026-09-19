#!/usr/bin/env bash
#
# Atrium's one check. Everything green = the tree is in a state worth committing.
#
# Run order is deliberate and each step depends on the one before it:
#   1. fmt    -- the cheapest possible failure, caught before anything compiles
#   2. build  -- does it compile at all (tests included: --all-targets)
#   3. test   -- does it behave
#   4. clippy -- is it written well; the only advisory step
#   5. deny   -- is it safe to DEPEND on what it depends on
#
# Stops at the first failure. Nothing here is optional except clippy, see below.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT" || exit 2

step() { printf '\n=== %s ===\n' "$1"; }
die()  { printf '\nFAILED at: %s\n' "$1" >&2; exit 1; }

step "cargo fmt --all -- --check"
cargo fmt --all -- --check || die "cargo fmt --all -- --check"

step "cargo build --workspace --all-targets"
cargo build --workspace --all-targets || die "cargo build --workspace --all-targets"

# Clippy is REPORT-ONLY for now: no `-D warnings`, so lints print but do not
# fail the run. It still stops the run if it fails outright -- a non-zero exit
# from clippy without `-D warnings` means a real compile-level error, not a
# style opinion, and that must not be swallowed.
step "cargo clippy --workspace --all-targets (report-only: warnings do not fail this run)"
cargo clippy --workspace --all-targets || die "cargo clippy --workspace --all-targets"

# Supply-chain check: RustSec advisories (vulnerabilities and unmaintained crates
# both fail), the licence allowlist, duplicate versions (warn) and sources
# (crates.io only). Config in deny.toml. This one is a hard failure, unlike
# clippy: an advisory is a fact about a dependency, not a style opinion, and
# "warn only" would mean a known-vulnerable crate could be merged on a green run.
#
# cargo-deny is not installed by this script. Locally: `cargo install --locked
# cargo-deny`. In CI the workflow installs a pinned version before calling this.
# If it is missing, this says so and exits -- it does not quietly skip the check.
step "cargo deny check"
if ! command -v cargo-deny >/dev/null 2>&1; then
  printf 'cargo-deny is not installed.\n' >&2
  printf 'Install it with: cargo install --locked cargo-deny\n' >&2
  die "cargo deny check (not installed)"
fi
cargo deny check || die "cargo deny check"

printf '\n=== all checks passed ===\n'
