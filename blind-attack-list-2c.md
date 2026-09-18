# Attack list: environment snapshot/restore

## A. Completeness of capture (promise 1)

1. Fill the environment with a mixed tree: regular files (empty, small, several GB), nested directories, symlinks, an executable, a broken symlink. Snapshot, then wipe the environment, restore. Every item must be back byte-for-byte, with same names, types, and contents. Use `diff -r` plus `find env -type l` comparisons, and compare file permissions bits (exec bit) even though times/ownership are exempt.
2. Include an empty file, an empty directory, and a directory chain of nothing but empty directories. After restore, all must exist — many copy loops skip empty dirs.
3. Include a deeply nested path (500+ levels of single-character dirs with a file at the bottom). Snapshot and restore must succeed (watch for path-length or recursion limits in the implementer's language, or a `cp -r` hitting ELOOP/ENAMETOOLONG).
4. Include a directory containing 100k tiny files. Snapshot must capture all of them; list/restore must complete in reasonable time and memory (no quadratic listing, no per-file process spawn).
5. Include files with names containing newline, tab, backslash, trailing space, leading dash (`-rf`), UTF-8 multibyte, invalid UTF-8 byte sequences (e.g. `\xff\xfe`), and a name that is literally `..`-like content such as `..hidden`. All must round-trip. Also try a name equal to `.` or `..` — creation must fail or be impossible; the snapshot machinery must not break because of them.
6. Include a file whose name differs from another only by case (`Foo` vs `foo`) and only by NFC/NFD Unicode normalization. On Linux both pairs coexist; neither may be lost or merged.
7. Include FIFOs, unix-domain sockets, and character/block-ish special files the user can create (e.g. `mkfifo`). Decide the promised behavior: either they are captured as their type (restorable by the same user — FIFOs and sockets are mknod-able by non-root) or cleanly skipped with documented behavior. What must NOT happen is the restore producing a regular file in their place, or the snapshot dying with an unhandled error, or (worst) the copy routine opening a FIFO and blocking forever on read.
8. Include a FIFO and start snapshot while nothing has it open. Create must not hang.
9. Include device files if creatable, or files with setuid bit set. Setuid bit is a "special attribute" (exempt), but the restore must not create a setuid-root file if some future privilege is present; verify no dangerous mode bits leak in.
10. Include several hard-linked files (`ln a b`). Identity need not be preserved (exempt), but both names must exist after restore with identical contents, and restore must not crash on discovering the same inode twice.

## B. Symlinks — the classic failure zone

11. A symlink inside the environment pointing to another file inside the environment. After snapshot+restore it must still be a symlink with the same target text — not replaced by a copy of the target file's contents.
12. A symlink pointing to a location OUTSIDE the environment (e.g. `/etc/passwd` or a sibling directory). On snapshot, the program must copy the link itself, not follow it and copy the outside file's contents into the store. On restore, the link must come back as a link. (If the copier follows links, promise 1 "nothing outside it" is broken and the store now contains outside data.)
13. A symlink pointing to the environment's own root, or a symlink cycle (`a -> b`, `b -> a`). Snapshot must terminate and capture the links, not loop or recurse until stack/path exhaustion.
14. A symlink pointing inside the environment whose target is then modified. Snapshot taken before the change, file changed, restore — the file's content must be the ORIGINAL (captured via the real path), and the link must still point at the same name.
15. A dangling symlink (target doesn't exist) inside the environment. Must be captured and restored as a dangling symlink. A naive copy that stats-then-opens will either skip it silently (restore loses it — promise 3 broken) or error out.
16. A symlink whose target is a DIRECTORY, nested inside other directories. Copier must not descend through it duplicating the tree into the store; the restored link must be a link.
17. Relative vs absolute symlink targets: `link -> sub/file` and `link -> /abs/path/env/sub/file`. Both must round-trip with their target text preserved (no rewriting, no resolving).
18. A symlink in the store path itself: pre-place a symlink inside the store directory pointing outside it, then restore — program must not write through that link to the outside target.

## C. Snapshot does not modify the environment (promise 2)

19. Record the full state (names, sizes, mode bits, symlink targets, and for this test also mtimes/ownership) before create; create a snapshot; re-record. Nothing may differ. Particularly: atime changes (must snapshot with O_NOATIME or a copy method that sets but that's exempt? — no: atime is NOT in the exempt list; check `noatime`-sensitive behavior or accept and document), directory mtimes touched by reading them, temp files created inside the environment during snapshot, and lock files left behind.
20. Run create twice in a row; the second snapshot's content must equal the first (no contamination from snapshot-of-snapshot temp artifacts).
21. Create a snapshot, then delete nothing and do nothing, then restore immediately. Environment must be byte-identical afterward — restore of an untouched tree must not mangle anything.

## D. Restore correctness and removal of the new (promise 3)

22. Snapshot a tree, then: modify a file, delete a file, delete a whole directory, create a new file, create a new directory with files, rename a file, chmod a file. Restore. Result must equal the snapshot exactly: content restored, deletions undone, creations removed, rename reverted, mode reset.
23. A file created after the snapshot at a path where the snapshot has a DIRECTORY, and a directory created where the snapshot has a FILE. Restore must replace them with the correct type, not fail or merge.
24. A symlink created after the snapshot pointing outside the environment, sitting at a path where the snapshot has a file. Restore must remove the symlink, not follow it and overwrite the OUTSIDE target — this is the single most dangerous restore bug: make the link point to a sacrificial file outside the env, restore, and verify the outside file is untouched.
25. Same but nested: post-snapshot, create `env/dirlink -> /outside`, then something at `env/dirlink/evil` cannot exist, but craft restore so the old tree contains a real directory at `dirlink`. Restore must replace the symlink with a directory, not write the restored files through the link into `/outside`.
26. Read-only files created after the snapshot (mode 444) in directories that must be emptied; restore must be able to delete them (chmod or force) without prompting or failing.
27. Files held open by a running process while restore deletes and recreates them. Restore should either succeed with sensible semantics (deleted-but-open file persists for the process) or fail cleanly — it must not leave a half-restored tree.
28. After restore, run a full compare against the store copy: file counts, per-file hashes, symlink targets, types. Must be exact, not "close".

## E. Cross-environment containment (promise 4)

29. Take a snapshot of environment A, then attempt to restore it into environment B. Must be refused (or at minimum loud-verified explicit), never silent success.
30. Craft: copy the store directory of environment A over the store of environment B (attacker-style), then list/restore in B. Restores must be rejected.
31. Two environments with identical internal file layouts. Snapshots must still be distinguishable; metadata (environment identity) must be captured at create time and checked at restore, not derivable from the snapshot's file contents.
32. Rename the environment directory between snapshot and restore (canonical identity vs path identity). Decide behavior; whatever it is, it must not be "silently restores the wrong tree."

## F. Store location (promise 5)

33. Configure/create with the store directory set INSIDE the environment (`env/.snapshots`). Must be REFUSED at create time.
34. Store set to a path that isn't inside the environment literally but reaches it via symlink (`env/link/store` where `link -> /tmp/store`): the symlink case. Must be refused on the resolved path, or at minimum the copy must not recurse into itself.
35. Even with store outside: verify the created snapshot does NOT contain a copy of the store. If the store is `real_store/` next to `env/`, confirm no self-capture; and specifically test store-as-sibling-via-bind-mount or hardlink games if the platform allows.
36. Agent-controlled attempt: from inside the environment, agent creates `env/store` as a symlink to the real store directory, then a snapshot is taken. Snapshot must not follow that link and duplicate/corrupt the store; restore must not delete the real store through the link.

## G. Incomplete or tampered snapshots (promise 6)

37. Kill the snapshot process midway (SIGKILL during a large create). The store must not now contain a snapshot that `list` shows as valid. Either the partial copy is cleaned up, or it is marked/markable as incomplete and restore REFUSES it.
38. After the kill, take a fresh snapshot; create must succeed and restore of the good one must work — the aborted attempt must not poison the store.
39. Take a valid snapshot, then flip one byte in one file inside the store copy. Restore must refuse (checksum/manifest mismatch) and the environment must be completely untouched by the refusal.
40. Truncate the store's manifest/metadata file; append garbage to it; replace a file entry's hash with the hash of a different file. All must be detected and refused.
41. Delete one file from inside the store snapshot. Restore must refuse — not "restore everything else and silently omit that file."
42. Add an EXTRA unexpected file inside the store snapshot directory. Detected and refused (or explicitly ignored by a manifest — but then verify the extra file is NOT restored).
43. Corrupt the store mid-restore: start restore of a huge snapshot, kill it halfway. The environment is now half-restored — check what the next restore does: it must either complete correctly from scratch or loudly refuse; it must not consider the half state "restored". (This tests whether restore is atomic-ish or at least restartable.)
44. Refusal safety: make the environment valuable-modified, attempt restore of a tampered snapshot, confirm refusal, then confirm environment matches pre-attempt state exactly — refusal itself must be non-destructive, i.e. validation happens BEFORE any deletion of the current tree.
45. Timestamp/mtime-only tampering in the store (touch a stored file). If integrity is content-hashed this is fine to allow; if integrity is mtime/size-based this MUST fail — pin down which the implementation claims and test that it actually detects content change with same size+mtime.

## H. Names as paths (promise 7)

46. Label `../../etc` or `../../../../tmp/x` on create: must be rejected or safely sanitized; verify no directory appears outside the store.
47. Label `..`, `.`, `.`, empty string, `/`, name containing `/`, name containing NUL (if the API is a string interface, NUL should be rejected pre-syscall).
48. Label that is an absolute path (`/tmp/x`).
49. Label with UTF-8 homoglyphs and with whitespace-only content; list/restore must not confuse them with other ids or trim them into a collision.
50. Id-based restore where id is attacker-chosen: pass `../othersnap` as the id to restore. Must not restore a different snapshot by escaping the snapshot's directory.
51. Two labels differing only by case, or by normalization, or one being a prefix of the other (`snap` vs `snapshot`) — no collision, no overwrite of each other.
52. Label collision with an existing snapshot: create twice with the same label. Defined behavior required (reject, suffix, or overwrite — but never a corrupt merged state between the two).
53. Very long label (4000 chars) — graceful rejection or handling, no crash, no filesystem error leaking as "success".
54. Symlink pre-planted: attacker creates `store/<next-snapshot-name>` as a symlink to `/victim` before create with that predictable label. Create must not write the copy through the link into `/victim`; restore must not read through it.

## I. Races and interruptions

55. While a snapshot is being created over a large tree, have a background process inside the environment continuously create/delete/rename files. The snapshot should either complete as a consistent-enough best effort (documented) or detect churn; it must NOT crash, and the stored snapshot must not be recorded as valid-and-complete if files it "captured" are half-written (decide and pin the semantics; then test restore produces something self-consistent, not a torn file).
56. Same race during restore: another process (the still-running agent) writes into the environment mid-restore. After restore reports success, verify final state; and ideally confirm restore either excludes concurrent agents (locking) or documents the hazard. At minimum it must not corrupt the STORE.
57. Two concurrent `snapshot create` invocations. Must not interleave into one broken snapshot; either serialized or isolated per-snapshot temp space.
58. Concurrent create and restore. One must win cleanly; no half states, no store corruption.
59. SIGKILL the restore midway over a large tree; run `list`; attempt restore again. The system must recover (see 43) — no permanent "restore in progress" lock left dangling.
60. Fill the disk (or the store's filesystem) during create. Must fail loudly, mark snapshot invalid, and leave both environment and prior snapshots intact. Then free space and confirm create works again.
61. Same, disk full during RESTORE — hardest one: current tree already deleted, copy-back half done. Check the implementation's story: staging-then-swap, or at least honest failure. Test that it doesn't report success on a half-restored environment.
62. Out of inodes (store with many tiny files on a small filesystem) — like 60.
63. Permissions removed from store directory mid-operation (chmod 000 as the same user, if running as same user; or store on unmounted mountpoint). Errors must surface as failures, never as empty snapshots.

## J. "Looks like success but is wrong"

64. Snapshot an environment, then compare the store against the env with `diff -r` AND a type-aware walker (`find -printf '%y'` per entry). A byte-identical diff can hide: symlinks materialized into files, FIFOs becoming files, a hardlink broken (exempt) or CREATED where none existed, exec bits dropped.
65. Restore, then immediately take a NEW snapshot of the restored environment. The two snapshots' manifests/hashes must match exactly. Idempotence failure here reveals drift (e.g. symlink handling differs between first capture and re-capture).
66. Snapshot list: verify it shows exactly the snapshots that exist, with correct association (label↔id), and does not list partial ones as good (see 37) or list directories an attacker dropped into the store (see 42) as restorable without integrity checks.
67. Restore the "latest" snapshot when list ordering is by mtime: tamper mtimes in store so an older snapshot sorts newest; confirm restore-by-id vs restore-latest behavior is explicit, not mtime-driven guesswork.
68. Locale trickery: filenames that sort/compare differently under LC_ALL=C vs UTF-8 locales; run create under one locale and restore under another. Nothing may be lost or mis-deduplicated due to locale-dependent name collation.
69. A file whose content is being appended while read (growing log) — captured snapshot will be some prefix; after "successful" restore the file must match what the manifest recorded, and the snapshot must have recorded exactly what it stored (length/hash of stored bytes), not of a later state.
70. Sparse file (e.g. `truncate -s 1T` plus one written block). Layout exempt, but: create must not attempt to materialize a terabyte of zeros (must finish using holes or at least stream sanely), restore must produce logically identical content, and `list`/quota behavior must not report absurd sizes as errors.

## Genuinely hard or impossible to guarantee

- **A truly consistent point-in-time copy of a live tree.** Without filesystem-level snapshots (btrfs/LVM/ZFS) or freezing the agent, any walk-based copy is a smear: files captured at different moments, and a renaming agent can make the same file appear twice or zero times. At best you get documented best-effort semantics or a lock that pauses the agent.
- **Crash-safe restore in the general case.** "Delete current tree, then copy back" cannot survive a power cut or ENOSPC midway unless restore stages the new tree beside the old and swaps atomically — and even directory-level atomic swap of a populated tree is not truly atomic on Linux (rename is atomic per name, not per tree).
- **Same-moment identity across rename of the environment.** Whether "a snapshot from a different environment" should follow the inode/device of the original directory, its canonical path, or a declared identity token is a policy choice the spec doesn't pin down; any implementation must pick, and tests 29–32 pin whichever it picked.
- **Guaranteeing nothing-outside-is-touched in the presence of planted symlinks.** A racing agent can swap a directory component for a symlink between the implementation's check and use (classic TOCTOU). Only openat2-style `RESOLVE_NO_SYMLINKS`-ish traversal closes it; pure checking never does.
- **atime neutrality.** Promise 2 ("create modifies nothing") arguably includes access times, and reliably avoiding atime updates needs O_NOATIME (with ownership constraints) or mounting noatime/relatime luck — not fully in the program's control.
- **Store integrity against a same-user attacker.** Hashes/manifests detect corruption, but the agent runs as the same user and, if it can reach the store path, can rewrite store AND manifest. The design only works if the store is genuinely outside the agent's reach (permission/mount boundary); "outside the directory" on the same filesystem as the same user is a weak guarantee worth stating honestly.
