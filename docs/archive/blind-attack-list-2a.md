# Adversarial attack list — sandboxed file-operation layer

Threat model recap: six ops (mkdir -p, write, read, list, move, delete -r),
an already-verified path validator that rejects `..` climbs, escaping
symlinks, absolute host paths, NUL bytes, and >255-byte components, and that
follows symlinks when resolving. The promise: nothing outside the environment
root is ever created, read, modified, or deleted.

## Group A — TOCTOU races between validation and use

1. **Symlink swap on read.** Call read on `/home/documents/report.txt`. If the
   validator resolves the path, sees a plain file, and returns a real path that
   is opened later, race it: in a tight loop from another process inside the
   root, alternate `report.txt` between being a real file and a symlink to
   `/etc/passwd`. Expect: some fraction of reads leak `/etc/passwd`. The fix is
   openat2-style RESOLVE_BENEATH or re-checking on the opened fd, not on the path.
2. **Symlink swap on write.** Same race for write: write to `/tmp/out.txt`
   (inside root). Parent passes validation as a real directory; before open(),
   swap it for a symlink to `/etc/cron.d`. Expect: file `evil` created at
   `/etc/cron.d/evil` — a write outside the root with attacker-controlled content.
3. **mkdir -p race.** Ask to create `/a/b/c/d`. If the implementation validates
   the whole target string first, then runs mkdir parents: another actor inside
   the root replaces `/a/b` with a symlink to `/` between the check and the
   mkdir of `c`. Expect: `/c/d` created on the host. The per-component walk must
   re-validate each level against the currently-open directory fd.
4. **Move race.** Move `/safe/a` to `/safe/b`. Between validation of both
   paths and the rename() syscall (or the copy+delete fallback for
   cross-fs), swap `/safe` for a symlink out. Expect: rename lands outside, or
   the post-move delete half of copy+unlink deletes something outside.
5. **Recursive-delete race.** Call delete -r on `/d`. While the tree walk is
   mid-descent, replace a subdirectory it hasn't entered yet,
   `/d/sub`, with a symlink to `/home/realuser`. If the walker uses open()/fstatat
   with O_NOFOLLOW per component this fails closed; if it re-resolves paths from
   strings at each step, it recurses out and deletes real user files.
6. **Delete race — mount-like swap.** Same walk, but swap `/d/sub` for a
   symlink to `/` mid-traversal. Worst case the walker deletes host paths
   breadth-first until it hits permissions. Even an EACCES storm shows the walk
   escaped the root.
7. **List race.** List `/d` where `d` is swapped to a symlink to `/home`
   between validate and opendir. This is disclosure, not destruction: response
   leaks host filenames and types.

## Group B — symlink placement subtleties (final vs. middle components)

8. **Symlink as final component of delete.** Inside the root create
   `link -> ../../etc` (the validator should reject at creation time if the
   write op were allowed to create symlinks — but the six ops can't create
   symlinks, so plant it via move: move an existing dangling symlink, or assume
   the "already exists on disk" clause: the root shipped with
   `/planted -> /etc`). Delete `/planted` non-recursively. Correct behavior:
   unlink the symlink itself. Buggy behavior: follow and try to delete `/etc`.
9. **Recursive delete through final-component symlink.** Delete -r `/planted`.
   Expect (bug): walker follows the final symlink and deletes `/etc`
   recursively. A correct implementation unlinks the symlink, period.
10. **Symlink as final component of move destination.** Move `/x` to
    `/planted` where `/planted -> /etc/hostname`. If the mover opens the
    destination after full resolution for a copy-fallback, it may truncate and
    overwrite `/etc/hostname`. If it renames over the symlink it's safe — the
    interesting case is the copy+unlink path overwriting the target.
11. **Middle-component symlink that stays inside but misleads.** Plant
    `/m -> /other-inside-root`. Validate `/m/secret`; resolution lands inside,
    so it's allowed — fine per the promise, but worth confirming the resolved
    path, not the literal string, is what every op actually uses, since a mix
    (validate resolved, operate on literal) re-opens every race in Group A.
12. **Symlink chain.** Plant `/a -> /b`, `/b -> /c`, `/c -> /etc`. ELOOP
    limits (40 on Linux) and per-hop resolution: confirm the validator
    re-checks after every hop and doesn't give up after the first resolution
    and pass the partially-resolved remainder to the op.
13. **Dangling-symlink write.** `/dangling -> /does/not/exist` inside root.
    Write to `/dangling`. Open with O_CREAT follows the link and creates
    `/does/not/exist` — inside, so safe, but if the parent dirs don't exist
    write "fails if parent missing" per spec; confirm the failure isn't a
    confusing success-report. Then make `/dangling -> /etc/newfile` (outside):
    if validation resolves lazily or only checks the literal parent, O_CREAT
    creates `/etc/newfile`.
14. **Dangling-symlink mkdir-through.** mkdir -p `/planted/sub` where
    `/planted -> /etc`. mkdir on a path whose final component exists as a
    symlink to a dir should fail EEXIST — but mkdir -p style walkers that
    stat() first will see the target dir exists and proceed; that's fine while
    it points inside, catastrophic if it points out and the walk isn't
    re-validated per component.

## Group C — hard links

15. **Hard link read.** Pre-place inside the root a hard link to a host file,
    e.g. `/loot` is another name for `/home/user/.ssh/id_rsa` (same
    filesystem). The validator sees no symlink, no `..`: path is fully inside
    the root. Read returns the host file's contents. The promise "nothing
    outside is read" is broken by inode aliasing, not by any path property —
    unless the environment root is its own mount point (MS_NOSUID-style
    isolation or a separate tmpfs), hard links to host files are
    indistinguishable from ordinary files. Attack: just read `/loot`.
16. **Hard link delete.** `/loot` hard-linked to host file. Delete `/loot`
    only decrements nlink — the host file survives, so this is safe-ish, but
    combined with 15 it proves the isolation is a path illusion. Attack to
    demonstrate, not to destroy.
17. **Hard link + move across filesystems.** Move a hard-linked file to a dir
    that happens to be a different filesystem inside the root (if the root
    spans mounts). Copy+unlink fallback semantics vs rename() can differ on
    permissions and partial failure — mainly a "wrong thing inside" case: a
    failed copy mid-move may leave the source deleted or a truncated dest,
    while reporting success.

## Group D — the recursive delete walk itself

18. **Self-root delete.** Delete -r `/`. Does the validator treat the root
    itself as "inside the root"? Expect: entire environment wiped. Arguably
    allowed by the spec but almost surely not intended — the op should refuse
    to delete the root itself.
19. **Delete a mount point inside the root.** If anything is mounted beneath
    the root (bind mount of a host dir, tmpfs), delete -r on `/mnt` crosses
    into it unless the walker checks st_dev boundaries. Expect: files on the
    host filesystem backing the mount are deleted even though the path is
    "inside". The walker must either not cross st_dev or explicitly reject
    mount points.
20. **Deletion during concurrent tree growth.** Delete -r `/d` while another
    actor inside the root continuously creates `/d/a0/a1/.../aN` deeper than
    the walker's frontier. Expect either: delete returns success while entries
    remain (success-report lie), or it loops indefinitely (DoS). A correct
    implementation reports partial failure or uses rename-to-quarantine then
    delete so the tree is frozen.
21. **Depth/name-length DoS inside the rule.** Components up to 255 bytes are
    allowed. Create a chain of nested dirs each of 255-byte names as deep as
    the walker permits, then trigger a recursive delete or list. Expect stack
    exhaustion (recursive walker), PATH_MAX overflow if the code builds
    path strings instead of using *at() syscalls (ENAMETOOLONG failure is
    benign; silently truncating the string and operating on the prefix is not).

## Group E — move semantics

22. **Move dir into itself.** Move `/a` to `/a/b`. rename() fails EINVAL — but
    a copy+unlink fallback or a buggy walk could partially copy a tree into
    itself, growing it until disk exhaustion, then unlink the original.
    Expect: success report after destroying `/a` or filling the filesystem.
23. **Move over a non-empty directory.** Move `/x` onto `/y` where `/y` is a
    non-empty dir. rename() fails ENOTEMPTY; a "helpful" fallback might first
    recursively delete `/y` — deleting things the agent never asked to delete.
24. **Move a file onto an existing path that is a symlink to outside.** Plant
    `/dest -> /etc/important`. Move `/src` to `/dest`. rename() replaces the
    symlink (safe); a copy-fallback that opens `/dest` for writing truncates
    `/etc/important`. The move's destination handling must never follow a
    final symlink for writing.
25. **Move across the root boundary via destination symlink parent.** Dest
    `/planted/foo` where `/planted -> /etc`: if validation resolved the dest
    string once and the mover later does rename(resolved_src, "/planted/foo")
    using the literal, the kernel follows `/planted` again — same race as A4.

## Group F — name and encoding edge cases ("wrong thing inside" bucket)

26. **Unicode confusables.** Two names that differ only in normalization
    (NFC vs NFD, e.g. `é` as one codepoint vs e+combining accent) — on APFS
    they collide, on ext4 they don't. Write `caf\u00e9/x` and `cafe\u0301/x`:
    on a normalizing filesystem one silently overwrites or merges with the
    other — data loss inside the root with an all-success report.
27. **Trailing-dot / trailing-space names.** Write to `/foo.` or `/foo `.
    Harmless on Linux, catastrophic if this root is ever backed by a Windows or
    SMB layer where `foo.` == `foo` — one file clobbers another.
28. **`.` and `..` as literal final components where the spec forgot one.**
    Validator is specified as rejecting `..` climbs. Try delete -r `/d/.` and
    move `/x` to `/d/.`. On Linux these fail EINVAL, but confirm the validator
    normalizes rather than relying on kernel errors — `/d/./` after a future
    refactor could become `/d` and delete more than intended.
29. **255-byte component at a path boundary.** Create a name of exactly 255
    bytes, then one of 255 bytes that becomes 256 in a different encoding
    after any normalization pass. If the length check runs before
    normalization and the op runs after, a >255 component reaches the kernel
    (ENAMETOOLONG at best).
30. **Case collision.** `/Foo` and `/foo` on a case-insensitive backing store.
    Write both with different contents; the second clobbers the first while
    the op log shows two successful distinct writes.

## Group G — reporting and partial-failure lies (safe but wrong)

31. **mkdir -p success when a component is a file.** mkdir -p `/f/g` where
    `/f` is a regular file. Must fail; watch for an implementation that stat()s
    the final path only and reports success because "something exists".
32. **List on a file.** List `/file.txt`. Should fail ENOTDIR; confirm it
    doesn't get silently treated as a single-entry listing of itself.
33. **Delete non-recursive on a non-empty dir.** Delete `/d` without the flag
    where `/d` has children. Must fail ENOTEMPTY; a loose implementation may
    quietly recurse anyway ("make delete just work"), deleting a tree the
    caller believed protected by the missing flag.
34. **Write to a directory path.** Write content to `/d` (a dir). Must fail
    EISDIR; a fallback might unlink+rmdir the directory to make room for the
    file.
35. **Partial recursive delete reported as success.** Fill `/d` with a mix of
    deletable files and one with permissions such that unlink fails (root-owned,
    immutable attr, or an ETXTBSY executable). Delete -r must report failure
    and ideally leave a precise residue report; the common bug is returning
    "ok" on best-effort deletion.
36. **Read of a special file inside the root.** If the root ever contains a
    fifo, socket, or device node (planted during setup), read must reject
    non-regular files — reading a fifo blocks forever (DoS); reading a device
    node could be worse. Also `write` to a fifo: blocks.
