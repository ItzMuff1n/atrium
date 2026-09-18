# atrium-resolver (Phases 1 + 1b)

The Atrium sandbox path resolver — the only code in the project allowed to
turn a virtual (agent-visible) path into a real host path (DESIGN.md §3.2).
One library function, `resolve(root, virtual_path)`, plus a thin
command-line harness and tests. `std` only, no dependencies.

## Function contract

```rust
pub fn resolve(root: &Path, virtual_path: &str) -> Result<PathBuf, ResolveError>
```

- Virtual paths are absolute *within* the environment: `/home/documents`
  means `<root>/home/documents`. Relative paths are rejected with a reason.
- Resolution walks component by component, canonicalising (following
  symlinks, resolving `..`) on the real filesystem **at every step**, and
  checks containment against the canonical root **at every step** — never
  only at the end. This catches `/link-out/../home/documents`.
- Rejections are structured (`ResolveError`) with per-case human-readable
  reasons naming the offending component and rule.
- The function never creates, modifies, or deletes anything. It only reads
  (canonicalise/stat) metadata.

## Harness

```sh
atrium-resolver resolve --root <dir> <virtual-path>   # ACCEPT/REJECT one path
atrium-resolver demo                                  # fixture env + attack list, one command
atrium-resolver fixtures --root <dir>                 # create attack-list D/I fixtures in <dir>
```

## Scope: Phases 1 and 1b

Existing paths resolve exactly as Phase 1. Under Phase 1b, a path whose
target **does not exist yet** is accepted as long as its resolved location
sits inside the root: the first genuinely-absent component switches the
rest of the walk to textual mode (nothing further is stat-ed; `//`
collapses, `.` is elided, `x/..` removes `x`, a `..` above the root
rejects naming the step). A dangling symlink is still a symlink: its
target is what matters — outside the root rejects, whether or not the
target exists (DECISIONS.md, "Phase 1b — three resolver rulings").
Component length is checked by the resolver itself: any single component
over 255 **bytes** rejects, whether the path exists or not. `PATH_MAX` is
deliberately not enforced.

## Known limits (filesystem-level, not path-level — brief §9.6)

No path resolver can catch these; they are recorded honestly rather than
pretended away:

- **Hardlinks** inside the root pointing at outside inodes are
  indistinguishable from ordinary files at path-resolution time.
- **Bind mounts** inside the root present the same problem class.
- **TOCTOU**: a symlink inside the root could be swapped to point outside
  *after* resolution but before the caller uses the path. The real defence
  belongs to the file-operations layer (Phase 2, e.g. `open` with
  `O_NOFOLLOW`), not to this resolver.
- **Namespace population is not the resolver's job.** No name is reserved
  or special-cased — `/etc`, `/proc`, `/root` and friends are ordinary
  names under the root; the resolver contains everything by construction
  (DECISIONS.md, "The environment namespace follows the Linux tree").

## Tests

`cargo test` covers every line of `attack-list.md` sections A–I **and**
`attack-list-1b.md` sections A–K (rejections with specific,
escape-naming reasons; acceptances verified contained). The three
component-length cases (255-byte accept as fixture and absent, 256-byte
reject, 200-Hebrew-char/400-byte reject) are tested on absent names so
the rejection can only come from the resolver's own check.

**One exception, stated rather than implied:** the three lines in section F
containing a NUL byte cannot be passed through the `resolve` harness at all —
a command-line argument is terminated by a NUL byte by definition. They are
covered by the test suite only (`section_f_unicode_and_lookalikes`,
`section_h_nasty_combinations`), which calls the resolver in-process. The
hands-on pass therefore does not exercise them. Recorded in `attack-list.md`
section F.
