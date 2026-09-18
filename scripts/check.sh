#!/usr/bin/env bash
#
# Atrium's one check. Everything green = the tree is in a state worth committing.
#
# Run order is deliberate and each step depends on the one before it:
#   1. fmt    -- the cheapest possible failure, caught before anything compiles
#   2. build  -- does it compile at all (tests included: --all-targets)
#   3. test   -- does it behave
#   4. clippy -- is it written well; the only advisory step
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

step "cargo test --workspace"
cargo test --workspace || die "cargo test --workspace"

# Clippy is REPORT-ONLY for now: no `-D warnings`, so lints print but do not
# fail the run. It still stops the run if it fails outright -- a non-zero exit
# from clippy without `-D warnings` means a real compile-level error, not a
# style opinion, and that must not be swallowed.
step "cargo clippy --workspace --all-targets (report-only: warnings do not fail this run)"
cargo clippy --workspace --all-targets || die "cargo clippy --workspace --all-targets"

printf '\n=== all checks passed ===\n'
