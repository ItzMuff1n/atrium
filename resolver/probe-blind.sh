#!/usr/bin/env bash
# Cases from blind-attack-list.md that attack-list.md does NOT already cover.
# Each needs its own fixtures (planted symlinks), which a real attacker would
# have created through a write-capable tool earlier in a session.
set -u
ROOT=/tmp/atrium-blind
BIN="./target/debug/atrium-resolver"
rm -rf "$ROOT"; mkdir -p "$ROOT"
"$BIN" fixtures --root "$ROOT" >/dev/null || exit 2

# Fixtures the blind list assumes exist.
ln -sfn /etc "$ROOT/link-out"
ln -sfn / "$ROOT/link-root"
ln -sfn ../outside "$ROOT/link-relout"
ln -sfn loop "$ROOT/loop"            # self-referencing
ln -sfn /proc/self/root "$ROOT/X-procroot"
ln -sfn /proc/self/cwd  "$ROOT/X-cwd"
ln -sfn /dev/stdin      "$ROOT/X-stdin"
mkdir -p "$ROOT/d"
ln -sfn /etc "$ROOT/d/mid"           # middle-component symlink
ln -sfn "$ROOT/link-out" "$ROOT/l1"
ln -sfn /var/lib "$ROOT/l2"
mkdir -p "$ROOT/tmpdir"
mkdir -p "$ROOT/foo"
mkdir -p "$ROOT/a/b"
mkdir -p "$ROOT/deep/a/b/c"

ESC=0
probe() {
    local label="$1" path="$2"
    local out rc
    out="$(timeout 5 "$BIN" resolve --root "$ROOT" "$path" 2>&1)"; rc=$?
    local verdict="${out%% *}"
    local mark="ok  "
    if [ "$rc" = "124" ]; then mark="FAIL(HANG)"; ESC=$((ESC+1))
    elif [ "$verdict" = "ACCEPT" ]; then
        case "${out#* }" in
            "$ROOT"*) : ;;   # contained: acceptable
            *) mark="FAIL(ESCAPE)"; ESC=$((ESC+1)) ;;
        esac
    fi
    printf '[%s] %-42s %s\n' "$mark" "$label" "$out"
}

echo "=== 1. lexical / absolute / separators ==="
probe '.../...//etc/passwd'        '/.../...//etc/passwd'
probe 'root given as absolute host path' "$ROOT"
probe '//etc/passwd'               '//etc/passwd'
probe '///etc/passwd'              '///etc/passwd'
probe 'prefix-sibling /workspace-malicious/pwn' '/home/muffin/workspace-malicious/pwn'
probe 'foo//bar'                   '/foo//bar'
probe './././secret'               '/././secret'
probe 'a/./.././../etc'            '/a/./.././../etc'

echo
echo "=== 2. trailing slash on a FILE (POSIX: should fail ENOTDIR) ==="
probe 'file.txt/ (file, trailing slash)' '/home/documents/notes.txt/'
probe 'file.txt  (same, no slash)'       '/home/documents/notes.txt'

echo
echo "=== 3. deep path: 500 components then ../../.. ==="
DEEP="/$(printf 'a/%.0s' $(seq 1 500))"
probe '500 components'             "$DEEP"
probe '500 components then /../../..' "${DEEP}../../.."

echo
echo "=== 4. planted symlinks (blind list section 4) ==="
probe 'link-out/passwd (middle component)'  '/link-out/passwd'
probe 'link-root/etc/passwd'                '/link-root/etc/passwd'
probe 'd/mid/passwd (mid symlink in real dir)' '/d/mid/passwd'
probe 'link-relout (target ../outside)'     '/link-relout'
probe 'l1 (->link-out), /../.. after it'    '/l1/../..'
probe 'l1/l2/../../no'                      '/l1/l2/../../no'
echo "  -- self-referencing symlink (ELOOP probe):"
probe 'loop'                                '/loop'
probe 'loop/loop/loop/x'                    '/loop/loop/loop/x'

echo
echo "=== 5. procfs / dev magic links ==="
probe 'X-procroot/etc/passwd'               '/X-procroot/etc/passwd'
probe 'X-cwd/../../etc'                     '/X-cwd/../../etc'
probe 'X-stdin'                             '/X-stdin'

echo
echo "=== 6. no shell expansion / encoding / injection bait ==="
probe '$HOME/../../etc/passwd'              '$HOME/../../etc/passwd'
probe '~root/.ssh/id_rsa'                   '~root/.ssh/id_rsa'
probe '$PWD/../x'                           '$PWD/../x'
probe '%2e%2e%2f%2e%2e%2fetc%2fpasswd'      '/%2e%2e%2f%2e%2e%2fetc%2fpasswd'
probe '..;/..;/etc/passwd'                  '/..;/..;/etc/passwd'
probe 'U+2025 two-dot leader'               '/‥/‥/etc/passwd'
probe '255-byte component + traversal'      "/$(printf 'a%.0s' $(seq 1 255))/../../../../etc/passwd"

echo
echo "=== 7. degenerate inputs ==="
probe 'empty string'                        ''
probe 'single dot /'                        '/.'
probe 'dotdot /..'                          '/..'

echo
echo "escapes or hangs: $ESC"
rm -rf "$ROOT"
[ "$ESC" = "0" ] && echo "probe-blind: no escapes, no hangs" || echo "probe-blind: PROBLEMS ABOVE"
