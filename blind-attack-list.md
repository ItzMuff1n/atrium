# Atrium — blind second attack list (§12)

**Produced:** 11 Sep 2026, delegation `deleg_c6716023`, model kimi-k3.
**Purpose:** satisfies the independence requirement in BUILD-PLAN and
`attack-list.md` §Independence — an attack list from a head that never saw the
resolver source, the delegation brief, or `attack-list.md`.

The subagent was given only a plain description of what the sandbox must
guarantee, and asked what path strings it would try. It used no tools (1 API
call, pure reasoning, 34s). Its own words, verbatim, below.

**Recovered and stored 12 Sep 2026.** This list previously existed only as
`/tmp/blind.json` and a delegation-summary cache file — both outside the
project folder, and `/tmp` does not survive a reboot. It is copied here so the
record is durable. Nothing in it has been edited.

---

This is a pure reasoning task — no tools needed. Here is my independent attack
list. I'll assume the workspace root is `/home/muffin/workspace` for
concreteness, though all relative paths are root-agnostic.

## 1. Basic traversal — lexical
- `../../etc/passwd` — classic dot-dot escape; catches resolvers that join strings without lexical normalization.
- `../../../../../../etc/passwd` — over-traversal; if the resolver only clamps one or two `..` or stops at root, this probes whether clamping makes it "/" (still outside).
- `..` — the root's parent, directly. Minimal case often forgotten.
- `../` — same, with trailing separator; some normalizers treat `..` and `../` differently.
- `foo/../../..` — traversal after a legit prefix; catches resolvers that only check whether the *first* component is `..`.
- `a/b/c/../../../../..` — deep-relative then escape; probes iterative vs single-pass collapsing.
- `./../../etc/passwd` — leading `./` then traversal; catches resolvers that only strip leading `./` and skip the traversal check.
- `.../...//etc/passwd` — `...` is just a filename on Linux; this is benign in reality but tests whether a resolver naively rejects any component *starting with* dots (should be safe — included so the allowlist logic isn't accidentally weakened around it).

## 2. Absolute paths and root confusion
- `/etc/passwd` — obvious absolute path; tests whether the resolver rejects absolute input outright (it should, or reinterpret it under root).
- `/home/muffin/workspace/etc/passwd` — an absolute path that *is* inside the root; tests whether absolute-in-root is handled or blindly rejected.
- `/home/muffin/workspace/../.ssh/id_rsa` — absolute path that starts inside root then escapes; catches "starts with root prefix" string checks done *before* normalization.
- `/home/muffin/workspace` — the root itself, absolute; boundary case.
- `//etc/passwd` — POSIX says a leading exactly-two-slashes is implementation-defined; some tools special-case it.
- `///etc/passwd` — collapsing behavior may differ from `//`.
- `/home/muffin/workspace-malicious/pwn` — a sibling whose name shares the root as a *string prefix* but not a path prefix; classic failure of `realpath().startswith(root)` done on a root without a trailing separator.

## 3. Separator and normalization abuse
- `foo//bar` — duplicated separators; tests collapsing idempotence of my own check vs the resolver's.
- `./././secret` — repeated current-dir noise burying a sensitive name.
- `a/./.././../etc` — interleaved `.` and `..` to stress the collapse order.
- Trailing-slash probes: `secret/`, `file.txt/`, `../` — `O_NOFOLLOW`/`ENOTDIR` behavior differs; a trailing slash on a symlink-to-dir must resolve through, on a file must fail. Also `.` vs `./`.
- Very deep `a/a/a/.../a` (500+ components) then `../../..` — probes stack/recursion limits in component-wise resolvers and ELOOP-adjacent behavior.

## 4. Symlink attacks (the hardest class for any resolver)
Note: these require planting symlinks first (via a write-capable call earlier in the session) — a real attacker's standard setup. Each assumes the simulated env contains (or the agent has created) the link named:
- `link-out` → `/etc`, then supplied path `link-out/passwd` — a symlink component in the *middle* of the path escaping the root.
- `link-out` → `/`, supplied `link-out/etc/passwd` — full-root dereference; checks whether the resolver resolves symlinks at all or only normalizes lexically.
- Chain: `a` → `b`, `b` → `..`, `b2` inside root → `a/x` — multi-hop chains where no single link points outside but the composition does.
- `link` → `../outside`, supplied `link/secret.txt` — relative symlink target escaping one level.
- Self-reference depth: `loop` → `loop`, supplied `loop/loop/loop/x` — ELOOP probe; a resolver must give up cleanly, not crash or fall back to accepting the lexically-normalized (inside-root-looking) path.
- Intermediate-component symlink: real dir `d/`, inside it `d/mid` → `/etc`, supplied `d/mid/passwd` — catches resolvers that only check the *final* component (`realpath` of full path is the fix; `O_NOFOLLOW` only on the last component is the bug).
- Symlink to a symlink to a *directory* outside: `l1` → `l2`, `l2` → `/var/lib`, supplied `l1/l2/../../no` — mixing symlink hops with traversal between them.

## 5. TOCTOU-shaped paths (race-class)
Not a string trick per se, but the strings I'd use to probe race windows:
- `tmp/link/secret` — supply it while `tmp/link` is a real in-root dir, then swap it to a symlink to `/etc` concurrently with the resolve call; tests openat2/dirfd-style resolution vs resolve-then-open.
- `deep/a/b/c/target` — many intermediate components, each of which can be swapped mid-resolution; maximizes the race window.

## 6. Unusual but legal characters
- `secret.txt\0../../etc/passwd` — embedded NUL: truncates at the NUL in C-string handling, or is rejected; dangerous if the check and the open disagree about truncation point (check sees tail, open sees head, or vice versa).
- `..\../..\etc\passwd` — backslashes are *ordinary characters* on Linux, so this is one long innocent filename — but it catches resolvers ported from Windows logic that treat `\` as a separator and would compute a traversal the OS won't follow *or* vice versa (resolver thinks inside, OS thinks... still inside, but the mismatch reveals the class).
- `file with spaces/../../etc/passwd` — quoting/escaping bugs in a resolver that shells out to `realpath`/`readlink`.
- `$HOME/../../etc/passwd`, `~root/.ssh/id_rsa`, `$PWD/../x` — no expansion should happen; if the resolver passes input through a shell or `wordexp`, these escape trivially.
- `%2e%2e%2f%2e%2e%2fetc%2fpasswd` — percent-encoded traversal; only bites if anything URL-decodes.
- `..;/..;/etc/passwd` — semicolons legal in Linux filenames; command-injection bait if a shell is involved.
- A component of 255 bytes plus more, and a total path approaching/exceeding PATH_MAX (4096): `aaaa...a/../../../../etc/passwd` with oversized segments — probes whether a too-long path causes an error fallback that *accepts*, or truncation-based prefix confusion.
- Non-ASCII: `‥/‥/etc/passwd` (Unicode two-dot leader U+2025) — not traversal on Linux, but reveals whether rejection logic is over-broad pattern-matching rather than real semantics.
- Unicode confusables in a *symlink target planted earlier*: file named `..` can't exist, but a name that *renders* as `..` can — tests logging/display confusion rather than resolution itself.

## 7. Root-adjacent and special files (assuming they resolve, reach is the prize)
- `proc/self/root/etc/passwd` via a planted symlink `X` → `/proc/self/root`, then `X/etc/passwd` — the kernel's magic symlink makes any lexical check meaningless; catches resolvers that readlink manually but don't treat `/proc/*/root` as opaque.
- Symlink planted to `/proc/self/cwd`, supplied `X/../../etc` — cwd-relative escape through procfs magic links.
- Symlink to `/dev/stdin` or `/dev/fd/N`, supplied `X` — fd smuggling if the resolver opens and passes descriptors.

## 8. Empty and degenerate inputs

*(The subagent's summary was truncated at this point by the delegation
harness — 2427 of its characters were captured. Section 8 is the only
incomplete part; sections 1–7 above are complete as delivered.)*

---

**Its own verdict on which battery matters most** (last captured lines):

> The single most decisive battery in practice: **section 4 (symlinks,
> especially relative-target and middle-component links) plus 2's
> prefix-sibling case** — those are where homegrown resolvers ("normalise
> lexically, then check `startswith(root)`") reliably [fail].

## Status of this list against the resolver

Per STATUS.md (11 Sep 2026): every novel line from this list was run against
the resolver with the fixtures rebuilt, **zero escapes**. Its "most decisive
battery" — symlinks and prefix-siblings — all held. Prefix-sibling and
middle-component symlink cases were re-run independently on 12 Sep 2026 with
the same result; the full output is in `phase-1-evidence.txt` §5, produced by
`resolver/probe-blind.sh`.
