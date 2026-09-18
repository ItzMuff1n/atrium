#!/usr/bin/env bash
# Build the committed seed corpus for the `resolve` fuzz target.
#
# Thin wrapper: the work is in `seed_corpus.py` beside this file, because the
# obvious shell version of this job silently loses every seed containing a NUL
# byte (bash's `read` cannot carry one) and those seeds cover the resolver's named
# NUL rejection. See that file's docstring.
#
# Usage:
#   bash fuzz/scripts/seed-corpus.sh           # rewrite fuzz/seeds/resolve/
#   bash fuzz/scripts/seed-corpus.sh --check   # verify it is still there and big enough
#
# Nothing is written outside `fuzz/seeds/resolve/`.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

exec python3 "$REPO_ROOT/fuzz/scripts/seed_corpus.py" "$@"
