# atrium-fileops — Phase 2a: the six file operations

Create a directory, write a file, read a file, list a directory, move,
delete. All of them route through `atrium-resolver` — every path argument is
a `VirtualPath` (the path as the agent wrote it), resolved to a `RealPath`
by `atrium_resolver::resolve` before anything touches the disk. `RealPath`'s
constructor is private, so there is no other way to obtain a real path
inside this crate: forgetting to resolve is a compile error.

Depends on `atrium-resolver` by **path** (`../resolver`) — code already in
this project and signed off hands-on. No external crates.

## Layout

```
fileops/
  Cargo.toml
  src/lib.rs        the six operations, FileOpError, VirtualPath/RealPath
  src/main.rs       `run` mode and `demo` mode (like resolver's)
  tests/fileops_tests.rs
  hand-test-2a.sh   the hands-on harness (the gate)
```

## CLI

```
atrium-fileops run --root <dir> create-dir <vpath>
atrium-fileops run --root <dir> write <vpath> --content <text>
atrium-fileops run --root <dir> read <vpath>
atrium-fileops run --root <dir> list <vpath>
atrium-fileops run --root <dir> move <from> <to>
atrium-fileops run --root <dir> delete <vpath> [--recursive]
atrium-fileops demo
```

Output names **virtual** paths only (`OK write /home/work/a.txt`). Real
resolved paths are never printed in default output; `--show-real` (off by
default) reveals them for debugging. This deliberately diverges from the
resolver's own CLI, which prints the real path on ACCEPT as a testing tool
— the operations must not hand the agent the sandbox's location on disk
(attack-list-2a.md §H.4/H.5).

## Behaviour highlights

- Refusal = nothing happens, and the reason names the specific thing: the
  `..` step, the symlink, the missing parent, the non-empty directory.
  There is no bare "denied".
- Resolver reasons pass through with their wording intact **except** the three
  variants that name a real on-disk location — `SymlinkEscapes`,
  `TraversalAboveRoot`, `EscapesRoot`. Those are re-worded from the resolver's
  structured fields so the reason still names the failing step while the host
  path is dropped (attack-list-2a.md §H.2–H.5, fixed 14 Sep 2026). If the
  resolver ever grows a reason that names a real path in a *new* variant, that
  match in `lib.rs` must be updated.
- `write` never creates missing parents; `create-dir` does, and is not an
  error on a directory that already exists.
- `delete` without `--recursive` refuses non-empty directories and the
  root. The environment root itself is never deleted or moved, whatever
  flags are given.
- **Recursive delete never follows a symlink.** Descent uses
  `symlink_metadata` per entry; links are removed *as links*. A tree
  containing a link pointing outside the sandbox loses the link — the
  outside is untouched (attack list §E.1, the most important line in the
  phase).

## §K: the symlink-object limitation (recorded, not fixed)

`resolve()` canonicalises an existing path **including its final
component**, so it returns a symlink's target rather than the link. The
consequence: a symlink cannot be operated on as an object.

- `delete /outside-link` — REFUSES. The link is inside the root and
  harmless, but resolving it lands outside, so it fails closed. Safe, and
  it means such a link can never be removed.
- `delete /inside-link --recursive` — removes the TARGET
  (`<root>/home/documents`) and leaves `/inside-link` in place, dangling.
- `move /inside-link /home/elsewhere` — moves the target; the link dangles.

These are safe (nothing outside the root is ever touched) and wrong (the
operation does not do what the path says). Not fixed in this phase: fixing
them requires a resolver change (a non-following resolution variant), which
is a stop-and-ask item and its own future phase with its own attack list.
See attack-list-2a.md §K.

## Running

```
cargo build
cargo test          # note: the §E.2 loop test hangs if links are followed
bash hand-test-2a.sh
```
