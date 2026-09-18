# Blind Attack List: Filesystem-Change Watcher in a Sandboxed Agent Workspace

## What I was given

A desktop application gives AI agents a closed workspace — one directory on a Linux machine called the environment root. Agents can freely create, read, write, move, and delete files inside it, and can run real shell commands whose working directory is inside it. Host access is supposed to be off: the agent should not reach anything outside the root and should not learn the root's real path. Because shell commands mutate files without going through the application's own file-operations code, a separate watcher component observes the environment root and reports what changed (create / modify / delete / move, and where) so the human's live file-tree view stays correct. It uses Linux inotify, attaches one watch per directory, and builds its initial picture by walking the tree at startup. It is not recursive at the kernel level. It only observes and reports; it blocks nothing. My job is to enumerate every way this watcher could be blind, wrong, misleading, broken, or abusable — purely from knowledge of how inotify and Linux filesystems behave.

---

## 1. Changes the watcher never sees at all (silence that misleads)

### 1.1 The startup race — changes during the initial tree walk
The watcher must walk the whole tree and then attach a watch to each directory it found. Between "walk directory D" and "attach watch to D", anything that happens in D is lost forever: the file created in that gap is neither found by the walk (already scanned) nor caught by inotify (watch not yet attached). Worse, a file created *and* deleted in the gap leaves no trace. A burst at startup (an agent bootstrap script writing many files) almost guarantees hits in this window.
- **Try:** spawn many `touch`/`rm` loops in a deep subdirectory while the watcher starts (especially with a large pre-existing tree to slow the walk).
- **Correct:** every change appears in the report afterward (or is reconciled by a re-scan). **Broken:** report is clean, tree view shows the old state, and nothing ever notices the drift.

### 1.2 Newly created directories are unwatched until a re-walk hook sees them
inotify is not recursive. When `mkdir /root/a/b` happens, the watcher gets an `IN_CREATE` for `b` on `/root/a`'s watch — but `b` itself has no watch until the watcher adds one. Anything placed inside `b` between mkdir and watch-attachment is invisible: files in `b` will never appear, ever, because no further event fires about them and no walk will ever re-run.
- **Try:** `mkdir d && touch d/f` in a single tight sequence (from C, no shell gap), or better `mkdir d; chmod` nothing; then check if `d/f` ever appears.
- **Correct:** `d/f` shows up. **Broken:** `d` appears as an empty directory forever. The viewer sees a completely successful, plausible state that is wrong.

### 1.3 Whole subtrees moved *into* a watched directory
`mv /somewhere/dir /root/watched_dir/` (or a rename from another watch into a tree the watcher doesn't follow) delivers a single `IN_MOVED_TO` for the top directory. The contents riding along inside it generate no events anywhere. If the watcher doesn't recursively scan on every `IN_MOVED_TO` directory event, an entire populated subtree appears as one empty folder.
- **Try:** `mv /tmp/bigdir` (with subfiles) into the root; or within the root, `mv a/watched b/unwatched_new` variants.
- **Correct:** every member of the moved tree appears. **Broken:** only the top folder appears, empty.

### 1.4 Files changed via memory-mapped writes with no further touch
inotify reports `IN_MODIFY` on writes through the page cache, so ordinary mmap writes do produce events — but the events fire on the *flushing* writeback, which can be delayed, batched, or happen long after the write. A viewer can be shown a stale file as "current" for an arbitrarily long stretch of wall-clock time even though the bytes on disk already differ.
- **Try:** mmap a file, write bytes, don't call msync, keep running; watch when (if) the modify event arrives.
- **Correct:** prompt modify event. **Broken/delayed:** event arrives tens of seconds later or only at close; the "live" tree was confidently stale the whole time.

### 1.5 Changes made underneath a mount point inside the root
If something inside the root is (or becomes) a mount point, inotify watches the filesystem object beneath the mount: it sees the underlying directory, not the mounted one. Writes inside the mounted filesystem produce no events on the watch. Conversely events from the covered directory may be reported for files the viewer can no longer even see by that path.
- **Try:** bind-mount or tmpfs-mount a directory inside the root over an existing one; write files inside the mount.
- **Correct:** changes visible. **Broken:** total silence while the visible tree changes; or events for invisible paths.

### 1.6 Activity through hard links by a different path
If the same inode is reachable by two paths inside the root and the write happens via the second path, the event names only the path used. A watcher keyed to one path (or a viewer showing one alias) shows silence where the user was looking. (Also see §4 for the outside-the-root version.)
- **Try:** `ln a b`; write via `b`; check that `a`'s display updates.
- **Correct:** both names updated / event disambiguated. **Broken:** one name shown as changed, the other stale — or event attributed confusingly.

### 1.7 Events for files in directories moved away before event processing
A file is created and then its parent directory is renamed elsewhere before the watcher drains its queue. The watcher can hold an event for a path that no longer exists at that path. If it resolves events lazily by path string, it will report a change at a nonexistent location — silence about the real one.
- **Try:** create `d/f`, immediately `mv d e`, before watcher processes.
- **Correct:** reported as create-then-move, or resolved to final path. **Broken:** phantom event on `d/f`, nothing on `e/f`.

### 1.8 Swap-file / atomic-save editors doing replace outside the notify window
Editors that write `file.tmp` then `rename(file.tmp, file)` produce create/delete/move triples, not a modify. That's usually handled — but a watcher that only surfaces "modify" may show *no change at all* for the file the user cares about, since the original inode was only `IN_DELETE_SELF`'d (see §2.3) and the new file technically "appeared" rather than "changed".
- **Try:** use atomic-replace writes on a watched file.
- **Correct:** viewer shows the file content updated. **Broken:** viewer shows the file untouched while every byte changed.

---

## 2. Reports that are technically true but not what happened (lies of interpretation)

### 2.1 Rename reported as delete+create, move reported as unrelated ops
inotify gives `IN_MOVED_FROM` / `IN_MOVED_TO` pairs carrying a shared cookie. If the watcher doesn't pair cookies (or pairs them with too generous a window), a rename shows as "file deleted, unrelated file created". The viewer animates destruction and birth instead of continuity; any UI tying history/comments/version-state to "the file" loses the thread.
- **Try:** `mv a b` within one directory; `mv` across directories.
- **Correct:** reported as a move. **Broken:** reported as delete+create; pairing logic visible in the events.

### 2.2 Move cookies from unrelated events paired together
Cookies are a 32-bit rolling counter. Under churn, if the watcher pairs "nearby" cookies heuristically instead of exactly, or holds unmatched `IN_MOVED_FROM`s too long, a new object's create can be wrongly glued to an old delete — showing a file "moved" between two places it never connected.
- **Try:** interleave many `mv` and `touch`+`rm` operations rapidly.
- **Correct:** exact pairs only. **Broken:** false move records stitched from unrelated events.

### 2.3 `IN_DELETE_SELF` / `IN_IGNORED` on the inode under the path
When the watched file/dir inode is deleted (even if another file is immediately renamed into its path), the watch on that inode dies and the kernel sends `IN_IGNORED`. If the watcher confuses "my watch died" with "the path is gone", it may drop all future events at that path or report the path as deleted when a same-named replacement is live and changing.
- **Try:** atomic-replace loop on one file many times.
- **Correct:** uninterrupted reports. **Broken:** the watcher goes deaf on that path after the first replace, reporting nothing ever after.

### 2.4 Directory replace via rename over an existing directory
`rename()` can replace an empty directory with another directory. Events seen: unlink-ish events on the target and move on the source. If the watcher doesn't re-attach watches correctly, the *live* directory at that path is now a different inode wearing the old name — the watcher may keep watching the corpse and report nothing about the live tree at that path, while every report looks normal.
- **Try:** `mkdir a b; mv -T a b` (or the rmdir-then-rename dance).
- **Correct:** events continue under path `b`. **Broken:** silence at `b`; watch stuck on unlinked inode.

### 2.5 Modify events that aren't (and missed events that are)
`IN_MODIFY` fires once per write syscall batch — a 1-byte tweak and a 1GB rewrite both look like "modified". A viewer that animates "file was edited" cannot tell a real edit from a touch-adjacent artifact. Conversely, `chmod`, `chown`, timestamp changes come as `IN_ATTRIB`, not `IN_MODIFY`; a watcher that ignores attrib reports "nothing happened" while permissions changed — which matters in a sandbox view.
- **Try:** `chmod 000 file` only; `touch` only; one-byte append.
- **Correct:** attribute changes reported distinctly. **Broken:** attrib silent; size-less "modified" treated as equivalent for huge and tiny writes.

### 2.6 Coalesced events: 40 writes, 1 event
The kernel coalesces consecutive identical events. A writer that appends steadily generates one `IN_MODIFY`. Any consumer estimating activity level, rate, or "how much changed" from event counts is systematically misled downward — or a consumer that samples file content once per event still works, but one that counts edits miscounts wildly.
- **Try:** hammer a file with 10,000 appends; count events.
- **Correct:** high or explicitly-coalesced counts. **Broken:** one event; viewer thinks a single small edit occurred.

---

## 3. Broken, mangled, or ambiguous naming in the report

### 3.1 Non-UTF-8 and arbitrary-byte filenames
Linux filenames are byte sequences, not UTF-8. A name containing `\xff\xfe` or a lone `\n` is legal. A watcher that decodes as UTF-8 (or feeds names into JSON) will either crash on some names, mojibake them, or encode replacement characters — after which the reported name no longer resolves to any real file. A name containing a literal newline can also inject fake "extra events" into line-oriented downstream consumers.
- **Try:** `touch $'ba\nd'`, `touch $'\xff\xfe'`.
- **Correct:** names round-trip exactly (bytes or escaped). **Broken:** mangled, dropped, or injection-shaped output, possibly a crash of the watcher on decode.

### 3.2 Path depth and length limits
Paths longer than 4096 bytes (PATH_MAX) are individually legal as components but cannot be spelled whole by libc path functions. If the watcher reconstructs full path strings, deep trees (easy to build from a shell: nested `mkdir` loop) make paths it cannot express, so report entries get truncated, fail resolution, or are silently dropped.
- **Try:** build a tree ~250 levels deep with 16-char names and edit the deepest file.
- **Correct:** deep events reported intact. **Broken:** truncation, resolution failure, or nothing reported at all past some depth.

### 3.3 Same name via rename swap (identity confusion)
`mv a tmp && mv b a && mv tmp b` swaps contents under the same two names. The watcher sees a burst of moves; if the viewer tracks content by path (natural), the files look unchanged in the tree while their contents are transposed. The report is "correct" as a list of moves and totally wrong as "what the user thinks happened".
- **Try:** the three-move swap above, fast.
- **Correct:** content re-check or move sequence surfaced. **Broken:** tree view identical, everything silently permuted underneath.

### 3.4 Case and normalization gotchas on foreign filesystems
If the environment root lives on a case-insensitive or normalizing filesystem (a mount of exFAT/NTFS/some network FS, or the root itself is a mount), `Foo` and `foo` may be the same file or may not, and names may be rewritten by the filesystem after creation (inotify reports what the FS committed, which may differ from the requested name). A watcher trusting the requested name reports a file that doesn't exist by that name.
- **Try:** create `Foo` then `foo`; create names with composed/decomposed unicode.
- **Correct:** events keyed to actual committed names. **Broken:** phantom-name events; duplicates that aren't.

### 3.5 `.` / `..` / trailing-slash and symlink-in-path resolution
If the watcher reconstructs absolute paths by joining strings, names containing path separators smuggled as literal components (impossible in one component but possible across a rename that crosses directories) or reports containing `..` segments can make the logical path point outside the root — see §5.4. Even without malice, a watcher normalizing paths differently from the viewer can disagree about which file changed.

---

## 4. The watcher itself breaking, silently

### 4.1 Watch-descriptor exhaustion
inotify has `max_user_watches` (commonly 8192–524288 but possibly less). A wide tree can exceed it. When the watcher can't add a watch, the default failure mode in many implementations is to log-and-continue: the tree view shows perfectly normal, merely permanently blind below the Nth directory. This is the "looks completely successful while wrong" archetype.
- **Try:** create 10,000+ directories (or lower the limit) and edit files in late-created dirs.
- **Correct:** loud error + fallback (rescan/refuse). **Broken:** silent partial coverage.

### 4.2 Queue overflow (`IN_Q_OVERFLOW`)
inotify's per-instance event queue is bounded (~16K events typical). A burst — `rm -rf` of a big tree, a build, a loop of touches — overflows it. The kernel's notification is a single `IN_Q_OVERFLOW` event; every dropped change is unrecoverable by definition. If the watcher doesn't respond to Q_OVERFLOW with a full rescan, the tree view is now wrong everywhere the dropped events touched, forever, with a clean-looking report.
- **Try:** `find . -exec touch {} +` over tens of thousands of files in a tight loop; or `rm -rf` a large tree instantly.
- **Correct:** explicit overflow detection and full-tree reconciliation. **Broken:** silent permanent drift; or overflow event swallowed.

### 4.3 Watch silently removed on directory delete (`IN_IGNORED` bookkeeping bugs)
When a watched directory is deleted or unmounted, the kernel drops the watch and emits `IN_IGNORED`. If the watcher's bookkeeping (descriptor→path map) mishandles this — e.g., fd reuse races — a *new* watch for a *new* directory can reuse the old descriptor; events for directory X can then be labeled as directory Y. Misattribution with zero errors anywhere.
- **Try:** rapid delete/recreate cycles of directories.
- **Correct:** names always match reality. **Broken:** events cross-labeled between directories.

### 4.4 Watcher process death, wedge, or event-loop stall
The watcher is separate code; if it crashes or its event loop blocks (e.g., a consumer callback doing slow work inline, backpressure into a UI thread, deadlock on a mutex), the application's file tree stops updating with no visible error unless there's a heartbeat/liveness signal. Everything the agent does after that moment is simply absent from the "live" view — the most dangerous failure, because it looks like the agent is idle.
- **Try:** kill/stop the watcher, or make a burst that stalls its processing, then keep editing.
- **Correct:** UI shows staleness/disconnect indicator; resync on restart. **Broken:** frozen "live" tree shown as current.

### 4.5 Startup watch on wrong root / root replaced between walk and watch
If the root directory itself is deleted and recreated (or bind-mounted over) between resolution and watching, the watcher watches the dead inode and the whole workspace report is empty or stale while everything looks fine.
- **Try:** replace the root via rename dance while watcher starts.
- **Correct:** detects root invalidation, errors loudly. **Broken:** silent total blindness.

### 4.6 Blocking reads on the inotify fd
If the watcher's queue empties and its read blocks while other processing doesn't run (single-threaded loop), lags accumulate; combined with 4.2 this converts small stalls into permanent loss.

---

## 5. Sandbox-boundary breakage caused or worsened by the watcher

### 5.1 Following a symlink that points outside the root
If the root contains a symlink to `/etc` or to anywhere host-side, and the startup walk (or on-event traversal for `IN_MOVED_TO` directories) follows symlinks when deciding what to watch, the watcher will attach watches *outside the sandbox*. From then on it reports events naming and describing host paths — direct, continuous exfiltration of host filesystem activity into the agent-visible report stream, and a leak of the rootless layout of the host.
- **Try:** `ln -s /etc root/etc_link` then create/modify host files (if any path is writable) or just observe watch count growth and event names.
- **Correct:** symlink treated as a file, never traversed; `lstat` not `stat`. **Broken:** host paths appear in reports.

### 5.2 Hard links to files outside the root
If the root sits on the same filesystem as host locations, `ln /host/file root/alias` gives the agent a view (via the watcher) of every change the host makes to that inode — write notifications, attrib changes — plus content access if the watcher samples contents. The watcher turns a hard-link escape into a *live surveillance feed*.
- **Try:** create such a link and have the host side touch or modify the file.
- **Correct:** hard-link creation blocked elsewhere, and watcher treats link count/exotic metadata warily. **Broken:** host writes appear as in-workspace events.

### 5.3 Rename-based escape and information flow via move cookies
If anything outside the root is watched or reachable, `IN_MOVED_FROM`/`IN_MOVED_TO` cookies can pair across the boundary: a report can effectively narrate "this file used to be at /host/secret/path and is now in your workspace" or vice versa. Even if content is unreadable, the movement event itself leaks host topology and activity.
- **Try:** move files across the boundary (to the extent anything outside is reachable).
- **Correct:** no cross-boundary events possible because no outside watch exists. **Broken:** boundary-crossing narratives in the report.

### 5.4 Root path disclosure in the report itself
The watcher's outputs must be root-relative. If any code path emits absolute paths (errors, overflow messages, "watching /home/realuser/..." logs forwarded to UI, `IN_MOVED_FROM` full-path payloads), the report hands the agent the secret real location of the root — which the sandbox explicitly wanted hidden. A single leaked absolute path breaks the location secrecy property with no exploit at all.
- **Try:** trigger error paths (queue overflow, watch failure) and inspect all report text for absolute prefixes.
- **Correct:** every path strictly root-relative, errors sanitized. **Broken:** any absolute path anywhere; even metadata like watch counts can help an agent fingerprint the host.

### 5.5 Symlink swap TOCTOU in the walk
During the initial walk, a directory entry checked as a real directory can be replaced with an outside-pointing symlink between the check and the traversal (a classic time-of-check/time-of-use). A racing agent process can steer the walk outside the root even if the walk "checks" for symlinks.
- **Try:** race a loop that swaps a dir entry between `mkdir` and `ln -s` during watcher startup.
- **Correct:** openat-style traversal pinned by fd, `O_NOFOLLOW`. **Broken:** traversal escapes the root and watches host directories.

### 5.6 Watching pseudo-files or device nodes created inside the root
If the agent can `mknod` a device or bind a FIFO/socket and the watcher tries to open/read the "file" to characterize it (size sniffing, hashing for content-change verification, previews), it can block forever on FIFO reads, or read from a device node — stalling the watcher (see §4.4) or touching host devices.
- **Try:** create a FIFO; create device nodes if CAP allows.
- **Correct:** watcher never reads special files; metadata-only. **Broken:** wedge or device access.

---

## 6. Burst, overwhelm, and resource attacks

### 6.1 Event-storm denial of coverage
Even below the overflow threshold, a sustained storm forces the watcher to spend all its time draining the queue; the tree view lags arbitrarily behind reality. If consumer logic samples current state at processing time rather than event time, the report's ordering becomes fiction: events are attributed to the state of the moment they're *read*, not the moment they *happened* — e.g., a create event reported after the file was already deleted appears as a phantom created file.
- **Try:** sustained 10k-events/sec churn for a minute, then inspect viewer consistency.
- **Correct:** ordering preserved or resync triggered. **Broken:** phantoms, out-of-order lies, or staleness without notice.

### 6.2 Watch-count DoS via directory explosion
Creating 100k empty directories is cheap for the agent and expensive for the watcher: watch descriptors, memory for the descriptor map, path strings on every event. This is an amplification attack: a tiny shell loop exhausts per-process fd limits (each watch is an fd-adjacent kernel object; inotify instances also count) or memory, killing the watcher or the host app.
- **Try:** `mkdir` loop to six figures.
- **Correct:** hard cap with loud "tree too large" signal. **Broken:** crash, or partial blind coverage as in §4.1.

### 6.3 Long-name / deep-tree memory blowup in per-event payload
Every event carries a path string. Deep trees make every event payload huge; storms of deep events multiply memory. An agent nesting deep + churning can balloon watcher memory with ordinary-looking operations.
- **Try:** deep tree plus modify storm at depth.
- **Correct:** bounded buffers, truncation strategy that still identifies the file. **Broken:** OOM / truncation-induced ambiguity.

### 6.4 Coalescing exploited to hide activity
An attacker *wanting* to be quiet on the viewer can exploit the coalescing in §2.6: many real changes collapse to a handful of innocuous "modified" dots. If the UI animates importance by event count, big changes can be made to look trivial.
- **Try:** rewrite a file 5,000 times vs. once; compare viewer emphasis.
- **Correct:** magnitude signalled by content diff. **Broken:** equal visual weight; stealth by churn.

---

## 7. Correct-looking reports that are wrong (the worst category)

### 7.1 Rescan-reconciliation with stale mtime logic
If overflow recovery or dedup logic decides "was this change already seen?" by comparing mtimes/sizes, then mtime forgery (`utimensat` can set any timestamps, no clock control needed) and size-preserving edits make a changed file look unchanged: the rescan skips it, and the viewer shows old content with a fresh-looking tree.
- **Try:** edit a file's bytes without changing its size, then reset mtime to the old value.
- **Correct:** content-hash or ctime-based checking catches it. **Broken:** reconciliation certifies a stale view as current.

### 7.2 "Create" events for files that were never created (rename-replacements)
Any logic that treats `IN_CREATE`/`IN_MOVED_TO` as "new file" without checking whether the path previously held something will show a replaced file as newborn: resetting its UI history, undo stack, or "added by agent at T" timeline. A malicious agent can rewrite a workspace file and have it presented as if it had just appeared, unconnected to its past.
- **Try:** overwrite a watched file via rename; check whether UI shows continuity.
- **Correct:** replacement distinguished from creation. **Broken:** history severed, plausible-looking fresh file.

### 7.3 Sticky "deleted" state after rapid delete-create
If the watcher dedups or debounces events ("file X created... file X deleted... file X created" → collapse to net effect), rapid sequences can net to "created" with the intermediate delete invisible — or collapse the wrong way and show "deleted" for a live file. Debounce is a lie generator for any fast agent.
- **Try:** create/delete/create within one debounce window with *different* contents each time.
- **Correct:** final state matches reality; history honest. **Broken:** wrong net state rendered confidently.

### 7.4 Inode-reuse identity confusion
If the watcher tracks identity by inode, rapid delete+recreate reuses inodes; the "same" file by inode number is a different file. Move-pairing or continuity logic keyed to inode can report a deletion and an unrelated creation as a continuity.
- **Try:** delete and immediately recreate many files, watch continuity labels.
- **Correct:** identity by path+content or explicit move cookies only. **Broken:** false identity.

---

## 8. Uncertainties — what I'd need to check on a real machine

- Whether inotify fires `IN_MODIFY` promptly for mmap-only writes or only on writeback/close, and at what granularity on this kernel — my §1.4 depends on it.
- The exact queue-overflow threshold (`/proc/sys/fs/inotify/max_queued_events`) and per-user watch/instance limits in effect here — §4.1/§4.2 thresholds are guesswork without them.
- Whether the environment root is on a real ext4/xfs/btrfs volume or on an overlay/fuse/bind layer — overlayfs and FUSE historically have event-fidelity quirks (missing events, wrong events) that would each be their own list entry; I can't assert them without knowing the mount stack.
- How the watcher pairs move cookies (exact match vs. heuristic window), whether it handles `IN_Q_OVERFLOW` with a rescan, whether the walk uses `lstat`/`openat(O_NOFOLLOW)`, and whether reports are root-relative — these are implementation facts I was not given; §5 assumes worst case in places.
- Whether the watcher verifies content (hash/diff) or trusts events alone — §7.1 is only exploitable if it trusts mtimes/sizes.
- Timing of watch attachment after `IN_MOVED_TO` for directories and `IN_CREATE` for dirs — §1.2/§1.3 gaps exist only as wide as that window.
- Whether special files (FIFO/device/unix socket) are creatable at all inside the sandbox (CAP_MKNOD, mount flags) — §5.6 depends on that.
- Whether the agent can create hard links to host files (same-filesystem constraint may make §5.2 moot — or trivial).
