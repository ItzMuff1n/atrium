# atrium-snapshot — Phase 2c

Snapshot and restore of the environment root. The undo the design promised in
`DESIGN.md` §3.4.

```
atrium-snapshot snapshot create  --root <dir> --store <dir> --label <name>
atrium-snapshot snapshot list    --store <dir>
atrium-snapshot snapshot restore --root <dir> --store <dir> --id <name>
atrium-snapshot demo
atrium-snapshot fixtures --root <dir> [--outside <dir>]
```

Exit codes: `0` success, `1` refusal (understood and declined), `2` usage error.

## The one rule that shapes everything

**The store must be outside the environment root.** A store inside the root is
**refused**, not warned about.

The reason is the whole point of the phase: a save point that the boss can delete
is not a save point. A shell command inside the environment (Phase 2b) can reach
anything the user can reach, so a snapshot stored inside the environment could be
destroyed by the very thing it protects against. `snapshot create --root R
--store R/snaps` exits 1.

This also closes open item **§N.23** from `attack-list-2b.md` — "a command can
write anywhere in the environment root, including wherever the app itself keeps
state". The app's undo state is the first durable thing the design would put
inside a sandbox's blast radius, and it is kept out of it.

## What a snapshot is, and what makes one restorable

A directory in the store is **not** a snapshot. A directory *plus a completed
record* is.

```
<store>/<label>/            the copied tree
<store>/<label>.snapshot    the record — written last, after the copy verified
```

`list` shows a snapshot only when both exist and the record ends with its
completion line. A directory with no record is what a create process killed
halfway leaves behind: not listed, not restorable, and **left on disk** as
evidence rather than tidied away.

The record names every entry — kind, mode, size and SHA-256 — so a snapshot whose
contents were changed after it was taken is **refused on restore, before the
environment is touched**. A file whose bytes changed but whose length did not is
caught by the hash; a file whose length changed is caught without opening it.

Path names in the record are stored as **raw bytes**, self-delimiting by length,
so a filename containing a newline, a space, or bytes that are not valid UTF-8
round-trips exactly.

## What it deliberately does not do

- **No copy-on-write.** `DESIGN.md` §3.4 says "copy-on-write if the filesystem
  supports it, otherwise a plain copy"; this takes the plain copy, and
  `DECISIONS.md` records why (the root's filesystem is 2e's decision and may be a
  tmpfs, which cannot reflink at all; `std` cannot issue `FICLONE`; COW changes
  cost, not behaviour). A measurement of what a copy costs is in
  `phase-2c-evidence.txt`.
- **No cage.** This tool confines nothing. A shell command under 2b can reach the
  store and the outside directory. That is 2e.
- **Not automatic.** This is the mechanism; "before every task" is the caller's
  job, and the caller (the agent loop) is Phase 5.
- **Not agent-facing.** It takes real host paths and is deliberately **not**
  routed through the path resolver: the resolver turns *virtual* paths into real
  ones, and there is no virtual path here. It is called by the app, as the gate
  is. See `DECISIONS.md`.
- **Not a defence against a same-user attacker.** The record detects accidental
  alteration. It is not cryptographic, and the store sits in the user's own
  account.

## Deliberately not preserved

These are decisions, not oversights (`attack-list-2c.md` §I). The harness prints
them in every run so the boundary is visible where the results are read.

- **Modification times.** `std` cannot set a timestamp; doing so needs a crate or
  an external program, and this project is `std`-only. After a restore, files look
  freshly written.
- **Hard links.** Two names sharing one file come back as two files with identical
  contents. The data survives; the inode identity does not.
- **Ownership, ACLs, extended attributes, sparse structure.**
- **Directory modification times.**
- **Access times** are neither preserved nor defended against — reading a file to
  hash it can update its atime. Recorded in `attack-list-2c.md` §K.7.

## Why the code is shaped the way it is

Three decisions worth knowing before changing anything:

1. **Every metadata call is `symlink_metadata`, never `metadata`.** A symlink is
   copied as a link with its exact target bytes and is never dereferenced — in
   either direction. A dangling link is a **success**, not an error. This single
   choice is the whole of §E, and a naive recursive copy gets it wrong silently:
   it both destroys the link structure and can pull host data into the store.
   `clear_contents` follows the same rule, so a symlink planted at a path where a
   restore expects a file is deleted rather than written through.

2. **Content is hashed on its way past, not re-read afterwards.** `copy_entries`
   hashes and writes in the same pass and returns what it actually wrote; that is
   what gets recorded. Measured on 5,000 small files, verifying the copy by
   re-reading it cost **5,592 ms of a 5,873 ms** snapshot — the first read of a
   just-written file is ~90× slower than the second. Hashing in flight makes that
   read unnecessary and makes the guarantee stronger, because it attests to the
   bytes actually written. The hashes from the earlier walk are compared against
   them as well, which is what catches a file that changed while the snapshot was
   being taken.

3. **Restore refuses before it deletes anything.** Validation order: record parses
   → origin root matches → snapshot verifies against its own record → **the
   environment's contents are confirmed removable** → only then is anything
   deleted. The fourth step exists because of a defect found by the blind list: a
   read-only directory made the delete step fail *after* other files had already
   been deleted, leaving a half-cleared environment — the undo mechanism causing
   the loss it exists to prevent. See `attack-list-2c.md` §N.1.

## Verifying it

```
cd "/home/muffin/VibeCodeProjects/atrium/crates/snapshot"
bash hand-test-2c.sh
```

111 lines. Watch each one: a line prints `[ok  ]` or `[FAIL]`, and the run exits
non-zero if anything failed. Three independent checks run alongside the lines and
fail the run regardless of what the program printed about itself:

- `!! OUTSIDE TOUCHED` — a real directory outside the root *and* the store
  changed. The root contains a symlink pointing into that directory, so a copy
  that follows links trips this.
- `!! STORE CHANGED` — a REFUSED line wrote into the store.
- `!! ROOT GONE` — the environment root directory itself disappeared.

Sections `H` and `I` are printed, not checked: what this tool is not, and what it
deliberately does not preserve. Everything else is a line.

A passing line and a failing line differ by one printed word, so a run cannot be
checked by eye for correctness — only for the absence of that word. Reading the
script is what establishes that the right lines ran. Same caveat as Phases 1, 1b,
2a and 2b.

## Dependencies

**None.** Not even a path reference to `atrium-resolver` — deliberately, and this
is the one place this crate differs from `fileops/` and `shell/`. Those two
resolve virtual paths; this one resolves nothing, and if it ever needs the
resolver that is the sign that agent input has crept into a tool that must not be
agent-facing.

SHA-256 is implemented in `src/sha256.rs` (FIPS 180-4, tested against the
published vectors) rather than pulled from a crate, because adding a registry
dependency is a stop-and-ask and the record's checksums are what make "a damaged
snapshot is never restored" true — a weak hash was not an option either.
