#!/usr/bin/env bash
# Negative proofs for the Phase 2d harness. A harness that has never failed is not
# evidence (2b lost a pass to exactly that reasoning). Each proof asserts on the
# SPECIFIC marker firing, not merely on a non-zero exit code.
#
#   P1  a broken expectation produces [FAIL] and exit 1
#   P2  a real change to the outside directory produces !! OUTSIDE TOUCHED
#   P3  a real path printed in the change stream produces !! REAL PATH LEAKED
#
# The harness is copied and mutated in each proof; the original is never touched.
set -u
cd "$(dirname "$0")"
ORIG=hand-test-2d.sh
FAILS=0

prove() {
  local name="$1" expect="$2" file="$3"
  local out rc
  out="$(timeout 250 bash "$file" /tmp/atrium-2d-neg-root 2>&1)"; rc=$?
  if printf '%s' "$out" | grep -qF "$expect"; then
    echo "  [ok]   $name — '$expect' fired"
  else
    echo "  [FAIL] $name — '$expect' did NOT fire (rc=$rc)"
    printf '%s\n' "$out" | tail -12 | sed 's/^/         /'
    FAILS=$((FAILS+1))
  fi
  if [ "$rc" -eq 0 ]; then
    echo "  [FAIL] $name — the run exited 0, so a fault did not fail the run"
    FAILS=$((FAILS+1))
  else
    echo "  [ok]   $name — exit $rc (non-zero)"
  fi
  rm -rf /tmp/atrium-2d-neg-root /tmp/atrium-2d-neg-outside /tmp/atrium-2d-neg-stage
}

echo "=== negative proofs for hand-test-2d.sh ==="
echo

echo "P1. a broken expectation produces [FAIL]"
sed 's|if has CREATE "/home/documents/by-command.txt"; then|if has CREATE "/home/documents/THIS-NEVER-EXISTS.txt"; then|' \
  "$ORIG" > ./neg-p1.tmp.sh
prove "P1" "[FAIL] A.1" ./neg-p1.tmp.sh
echo

echo "P2. a real change to the outside directory is caught"
# Inject one line that writes into the outside directory mid-run, inside the A block.
sed 's|^run_watch "A" 1600$|touch "$OUTSIDE/INJECTED-ESCAPE" ; run_watch "A" 1600|' \
  "$ORIG" > ./neg-p2.tmp.sh
grep -q 'INJECTED-ESCAPE' ./neg-p2.tmp.sh || { echo "  [FAIL] P2 injection did not land in the script"; FAILS=$((FAILS+1)); }
prove "P2" "!! OUTSIDE TOUCHED" ./neg-p2.tmp.sh
echo

echo "P3. a real path in the change stream is caught"
# Append the real root to RUN_OUT after the run, so the disclosure check has
# something to find. Direct and unambiguous.
sed 's|^  check_outside "\$label"$|  RUN_OUT="$RUN_OUT\nCREATE   $ROOT/leaked.txt"\n  check_outside "$label"|' \
  "$ORIG" > ./neg-p3.tmp.sh
grep -q 'leaked.txt' ./neg-p3.tmp.sh || { echo "  [FAIL] P3 injection did not land"; FAILS=$((FAILS+1)); }
prove "P3" "!! REAL PATH LEAKED" ./neg-p3.tmp.sh
echo

rm -f ./neg-p1.tmp.sh ./neg-p2.tmp.sh ./neg-p3.tmp.sh
echo "=== negative proofs done: $FAILS failure(s) ==="
exit $([ "$FAILS" -eq 0 ] && echo 0 || echo 1)
