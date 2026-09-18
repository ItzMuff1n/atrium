# Atrium Resolver — Independent Attack List (blind, emphasis on non-existent targets)

Assumptions: virtual paths start with `/`, that slash is the environment root, and the
resolver must return a host path guaranteed inside that root. Only two escape mechanisms
exist: a `..` that climbs above root, and a symlink whose final target lies outside.
Resolution is read-only, and non-existent targets must be permitted when they land inside
the root — so the interesting attacks are paths whose **tail does not exist** but whose
resolution walks through things that either do exist (and may be attacker-controlled
symlinks) or that tempt the resolver to normalize wrongly.

---

## 1. Plain `..` traversal at/above the root (non-existent tails)

"/.."

- **Reasoning:** Clamping test. Canonicalizing `<root>/..` gives the host parent of the
  root. A correct resolver must refuse to step above the root — clamp to root or error.
  If it normalizes purely lexically and then blindly concatenates, this escapes trivially.

"/../.."

- **Reasoning:** Same, but two steps: catches resolvers that special-case or clamp only a
  single leading `..`, or that clamp once instead of "at every step".

"/../../.."

- **Reasoning:** Three steps deep. Host layout of the root's parents is unknown; going
  further up maximizes the chance that a partially-clamping implementation lets at least
  one step through and lands somewhere interesting (e.g. the parent of the root's parent).

"/foo/../../bar"

- **Reasoning:** `/foo` does not exist under the root. If the resolver resolves component
  by component and errors on non-existent intermediate components, it may bail here —
  but if it *relaxes* the existence check (as it must, to allow new files), does it relax
  the `..` accounting too? The path lexically lands at `<root>/bar`, but a resolver using
  `realpath()`-style logic on a non-existent `/foo` can behave differently than a purely
  lexical one. Probes the lexical-vs-kernel resolution boundary.

"/nonexistent/../..//..//escape"

- **Reasoning:** Like the above, but the traversal happens *after* a non-existent
  component, so no symlink check can possibly have been done on any component. Whatever
  clamping exists must be purely lexical. If the clamp is "resolve `..` against real
  filesystem state," the non-existent component may confuse it into dropping a step.

"/a/b/c/../../../../d"

- **Reasoning:** Starts two levels down, climbs four: net one above root. Tests whether
  clamping is applied per-step ("can never go above root at any point") or only checked
  at the end, and whether the depth counter is rooted correctly. None of `a/b/c` need exist.

## 2. `.` and empty-component noise combined with `..`

"/./../../x"

- **Reasoning:** Interleaving `.` between the `..` steps. A normalizer that strips `.`
  only once, or that applies the "can't pop past root" rule to a component stack that
  still contains unprocessed `.` entries, can miscount depth and allow one step over.

"//../../x"

- **Reasoning:** Double leading slash. POSIX says exactly two leading slashes are
  implementation-defined; many parsers collapse slashes naively, but some take the
  "root anchor" branch on `//` and skip later clamping. Also probes whether the resolver
  distinguishes "the leading `/`" (virtual root) from subsequent slashes.

"/a//..//..//b"

- **Reasoning:** Empty components between traversal steps. Rust's `Path::components()`
  and Python's `os.path.normpath()` treat these differently (empty components are
  dropped, but `..` handling differs when the stack has gaps if the implementer built a
  manual stack). Miscount risk.

"/.../x"

- **Reasoning:** `...` is *not* a parent reference — a correct resolver treats it as a
  literal filename inside the root. Included as a control: verifies the resolver isn't
  over-eagerly treating any run of dots as traversal. Should NOT escape; if it does,
  the classifier is substring-based, which hints at deeper flaws.

"/..../x", "/.. /x"

- **Reasoning:** Similar controls: `....` and `.. ` (dot-dot-space) are literals on any
  sane filesystem. `..` followed by a space additionally probes for trimming bugs.

## 3. Symlink-assisted traversal with non-existent tails

These need something the attacker placed (or found) inside the root: a symlink. The
**target of the requested path still does not exist** — only the symlink does.

"/link/../escape"

- **Reasoning:** *The* classic. If `<root>/link` is a symlink to `/host/somewhere`, then
  lexical resolution gives `<root>/escape` (inside, safe), but kernel/`realpath`
  resolution gives `/host/escape` (outside). A resolver must either resolve `..`
  lexically against the *unresolved* prefix, or resolve symlinks first and then re-clamp.
  Whichever it does, this path probes the mismatch. The file `escape` need not exist
  anywhere — which is exactly why `realpath()` fails on some systems and forces the
  resolver down a "clean the path lexically" fallback that is exploitable.

"/link/../../escape"

- **Reasoning:** Same idea with two steps. If the resolver resolves the symlink then
  applies traversal against the *virtual* depth rather than the *realized* depth, one
  of these is miscounted. Also catches implementations whose clamp is "count `..` in
  the original string" — after symlink expansion the depth budget is wrong.

"/link/sub/../../escape"

- **Reasoning:** Adds a non-existent component *inside* the symlink target before the
  traversal. Realizing `/link/sub` fails (doesn't exist), so the resolver must decide
  `..` semantics against a partially-real path. This is where TOCTOU-ish "resolve as
  far as possible, then append the rest" logic gets the clamping wrong.

"/deep/link/../.."

- **Reasoning:** Symlink not at depth 1. If the resolver tracks "virtual depth" as it
  walks components, crossing a symlink should reset that accounting to the realized
  depth; implementations that keep counting virtual depth from the original string
  can clamp at the wrong point.

"/link"  (where `link -> /outside`, tail doesn't exist issue aside)

- **Reasoning:** Control case: the *final* component is the symlink. A resolver that
  only checks intermediate components, or that lstat-dereferences the last component
  without re-clamping the result, hands back the raw outside target. Even though this
  target may exist, it anchors the group's logic: if this escapes, everything above is
  moot; if it's blocked but the `/link/../` variants pass, the clamp is purely lexical
  and probably fine — or vice versa.

"/link/."  and "/link/.."

- **Reasoning:** Degenerate tails after a symlink. `.` should resolve to the link's
  target itself (re-clamped); `..` should resolve to the *virtual* parent (root), never
  to the realized parent of the outside target. The `..` variant is the sharper test.

"/a/b/../../link/sub/../../etc/passwd"

- **Reasoning:** Composite: up-down shuffle first, then a symlink hop, then traversal
  inside the realized location, landing on a non-existent subpath of a real outside
  directory. Probes whether clamping survives multiple phase changes (lexical traversal
  → symlink realization → traversal again) in a single resolution.

## 4. Absolute-target and relative-target symlink variants

The virtual path is the same; the *symlink content* is what varies. The path list I
would run is:

"/link2/../escape"   where link2 -> "../../outside" (relative target)

- **Reasoning:** Relative symlink targets are interpreted against the link's containing
  directory — i.e., against `<root>` here, so `link2` may point outside the root via its
  own `..`. Resolvers that only clamp `..` *in the requested path* forget that the
  symlink target introduces brand-new traversal steps. The tail `escape` doesn't exist.

"/nested/link3/sub/../escape"  where /nested/link3 -> "/etc" (absolute target)

- **Reasoning:** Absolute symlink targets are the straightforward escape; combining an
  absolute target with a non-existent subpath plus a trailing `..` tests the order of
  "resolve symlink" vs "apply traversal" vs "clamp" once more, from a deeper position.

## 5. Chained symlinks with non-existent tails

"/c1/../escape"

- **Reasoning:** Where `<root>/c1` -> `c2`, `c2` -> `/outside/dir`. Multi-hop chains
  stress any bounded re-clamping: each hop is a new chance that the resolver treats the
  intermediate (still-inside) path as already-validated and skips re-clamping after the
  final hop. Tail nonexistent so pure `realpath` can't bail the implementation out.

"/c1/sub/../../escape"

- **Reasoning:** Chain plus traversal after entering the realized tree: tests that the
  depth accounting restarts correctly at each hop, not just once.

## 6. Deep nesting, long names, boundary conditions

"/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/../../../../../../../../../../../../../../../../escape"

- **Reasoning:** Twenty `a` components (which don't exist), twenty `..`. Lexically it
  lands exactly at `<root>/escape` — a *control* that must not escape — but if any
  counter in the implementation is off by one at deep nesting, the clamping trips
  differently than at depth 2. Follow with twenty-one `..` for the actual escape
  attempt: "/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/../../../../../../../../../../../../../../../../../escape"

"/<255 a's>/../<255 b's>"

- **Reasoning:** Component at NAME_MAX. Buffer-oriented resolvers historically
  truncate long components; truncation of the first component changes what `..`
  cancels, which changes where the path lands. The tail is arbitrary and non-existent.

"/$(python -c 'print("a/"*2000)')../../escape"

- **Reasoning:** Pathological total length (~PATH_MAX and beyond). Probing for length
  caps that cause the resolver to fall back to a different (less careful) code path,
  or truncations that cut the string mid-`..`-sequence.

## 7. Weird-but-legal characters (mostly controls)

"/%2e%2e/x" , "/..%00/x" , "/%2E%2E%2Fhost/x"

- **Reasoning:** Percent-encoding and NUL are *not* filesystem syntax; a correct
  resolver treats them literally and they land harmlessly (and non-existent) inside the
  root. But if any layer above the resolver (or the resolver itself) URL-decodes or C-
  string-truncates, these become traversal. Cheap to try, high information value.

"/dir/../..\\..\\escape" and "/..\\"

- **Reasoning:** Backslash is a literal character on Linux. Windows-authored resolver
  code, or code shared with a Windows port, may treat `\\` as a separator. On Linux
  these should be inert; included because cross-platform sandboxes are where this bug
  actually ships.

## 8. Existence-gap timing shapes (no symlink needed at submit time)

"/newdir/../escape"  where `newdir` does not exist *yet*

- **Reasoning:** The resolver must handle this purely lexically (realizing components
  one by one fails at `newdir`). The question is whether, upon hitting a non-existent
  component, the implementation switches strategy — e.g. "realpath the longest existing
  prefix, then lexically join the rest." That hybrid is correct *only* if the lexical
  join re-applies the root clamp. This is my single highest-priority non-existent-target
  probe because the "longest existing prefix" strategy is the standard implementation
  recipe.

"/newdir/sub/../../../escape"

- **Reasoning:** Same trigger, unbalanced traversal: two components of non-existent
  tail with three steps up. If the hybrid joins `<root>` + `newdir/sub/../../../escape`
  and normalizes, fine; if it normalizes only the existing-prefix side, it escapes.

---

## Cases I was unsure about

- **"/link/../escape" (group 3, first item):** I am genuinely unsure whether it escapes
  because both resolutions are *defensible*: lexical gives an inside path that was never
  really the intended one; kernel gives the outside path. A careful resolver resolves
  symlinks as it walks and must then land outside — and must therefore **reject or
  re-clamp**, not silently return either. What I'm unsure about is which behavior the
  Atrium authors chose (reject vs lexical-flatten), and whether their flattening is
  consistent everywhere.
- **"/newdir/../escape" (group 8):** unsure for the same structural reason — the hybrid
  "realpath-longest-prefix" strategy is what most people write, and its correctness
  hinges on one small re-clamp step that is easy to forget. Could go either way.
- **"/a/b/../../link/sub/../../etc/passwd" (group 3):** unsure mainly about which of its
  several possible bugs it triggers; I'd expect *something* interesting but can't rank
  the outcomes.
- **"//../../x" (group 2):** POSIX's special-casing of exactly `//` is real, but whether
  the resolver's first tokenization treats the virtual root anchor as consumed or not
  is unguessable without code.
- **Symlink cases where the symlink appears *after* non-existent components** (e.g.
  "/nope/link/../escape"): if `nope` doesn't exist, `link` can't exist under it, so the
  case degenerates to pure lexical handling — I included the reasoning inline rather
  than a separate group, because I'm unsure it adds signal beyond group 8.

## Deliberately left out (guessed already covered)

- Plain existing-file controls like "/etc/passwd", "/./x", "/x//y" — sanity checks any
  existing list will have.
- `..` with no leading content at depth ≥1 that stays inside, e.g. "/a/../b" — the most
  obvious clamping smoke test, certainly present.
- Absolute-target symlink with no tricks: "/link" where link -> "/etc" — the canonical
  symlink test in every resolver test suite (I kept one as a control anchor but didn't
  build a group around it).
- `/proc/self/...`-style magic links and `/dev/std{in,out,err}` — these are *host*
  phenomena reachable only after an escape, not escape vectors through virtual paths.
- Bind-mount and hardlink tricks — outside the resolver's scope (it sees paths, not
  mount tables).
- Case-sensitivity / Unicode normalization probes ("/../" in exotic casing) — Linux
  filesystems are byte-oriented and case-sensitive; near-zero yield past the encoding
  controls already listed.
- TOCTOU swap-the-symlink races — require the harness to interleave filesystem mutation
  with resolution; that's a race-harness test, not a path string, so no list entry
  captures it.