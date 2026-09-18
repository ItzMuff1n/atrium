#!/usr/bin/env bash
# Parent's own adversarial probes — lines NOT in attack-list.md.
# Purpose: try to make the resolver accept something outside the root.
set -u
ROOT=/tmp/atrium-probe
BIN="./target/debug/atrium-resolver"
rm -rf "$ROOT"; mkdir -p "$ROOT"
"$BIN" fixtures --root "$ROOT" >/dev/null || exit 2

# Prefix-sibling: a real directory whose name starts with the root's name.
EVIL="${ROOT}-evil"
rm -rf "$EVIL"; mkdir -p "$EVIL"
echo secret > "$EVIL/secret.txt"
ln -sfn "$EVIL" "$ROOT/link-to-sibling"

echo "root=$ROOT  evil=$EVIL"
echo
printf '%-52s %s\n' "PATH" "VERDICT"
for p in \
  "/../$(basename "$ROOT")/home/documents" \
  "/../$(basename "$ROOT")-evil" \
  "/../../tmp/$(basename "$ROOT")/home" \
  "/link-to-sibling" \
  "/link-to-sibling/secret.txt" \
  "/link-to-inside/../.." \
  "/link-to-inside/../../home/documents" \
  "/home/documents/../../$(basename "$ROOT")/home" \
  "/home/documents/../../$(basename "$ROOT")-evil/secret.txt" \
  "/good-dir/../good-dir/../../.." \
  "/home/documents/./../../." \
  "/link-to-etc/passwd" \
  "/link-to-root$(basename "$ROOT")" \
  ; do
  out="$("$BIN" resolve --root "$ROOT" "$p" 2>&1)"; rc=$?
  echo "$(printf '%-52s' "$p") [$rc] $out"
done

echo
echo "--- length cases, printed with real lengths ---"
LONG="/$(printf 'a%.0s' $(seq 1 300))"
DEEP="/a"; for _ in $(seq 1 199); do DEEP="$DEEP/a"; done
C255="/$(printf 'b%.0s' $(seq 1 255))"
C256="/$(printf 'c%.0s' $(seq 1 256))"
echo "LONG len=${#LONG}  first 20: ${LONG:0:20}"
echo "DEEP len=${#DEEP}  first 20: ${DEEP:0:20}  slashes=$(tr -cd '/' <<<"$DEEP" | wc -c)"
echo "LONG -> $("$BIN" resolve --root "$ROOT" "$LONG" 2>&1)"
echo "DEEP -> $("$BIN" resolve --root "$ROOT" "$DEEP" 2>&1)"

echo
echo "--- 255 vs 256 char single component (NAME_MAX boundary) ---"
C255="$(printf '/%0.sb' $(seq 1 255))"
C256="$(printf '/%0.sc' $(seq 1 256))"
echo "255 -> $("$BIN" resolve --root "$ROOT" "$C255" 2>&1)"
echo "256 -> $("$BIN" resolve --root "$ROOT" "$C256" 2>&1)"

rm -rf "$ROOT" "$EVIL"
