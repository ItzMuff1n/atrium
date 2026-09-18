//! Phase 2d — the filesystem watcher.
//!
//! `DESIGN.md` §3.3 states why this exists: `run commands` (Phase 2b) executes
//! **real programs**, so those programs change files *outside* Atrium's own
//! file-operation layer. The `ran` effect covers the command and its output; the
//! **watcher covers what the command did.** Without it the explorer goes stale.
//!
//! The model is a **security camera on the environment**. It observes and reports.
//! It does not gate, block, decide or act.
//!
//! ## The four ways this phase fails *silently*
//!
//! Every other phase fails by doing something it should not. A watcher fails by
//! **not reporting something and looking fine**. Those four modes are what most of
//! this crate is about, and each is answered in a named place:
//!
//! 1. **A change is lost and the run still looks clean.** inotify's queue is
//!    finite (`max_queued_events`, 16384 on this machine). When it overflows the
//!    kernel **drops events** and emits one record with `wd = -1`. That becomes
//!    [`Change::Overflow`] and it **fails the run** ([`Change::is_fault`]).
//!    Nothing here guesses what was missed — a gap is reported as a gap.
//! 2. **Part of the tree is never watched.** inotify is not recursive; one watch
//!    per directory is placed by a walk ([`Watcher::walk_and_watch`]), and a
//!    directory that cannot be watched is **reported**, not skipped in silence.
//! 3. **A watch dies or goes stale and nobody is told.** A dead watch reports
//!    exactly as much as an idle one: nothing. [`Watcher::resync`] re-derives
//!    every watch's path from the tree, and a watch whose directory is no longer
//!    inside the root is **removed and reported**, never left to attribute
//!    outside activity to an in-root path.
//! 4. **The watcher watches something it was not asked to watch.** A watch placed
//!    *through* a symlink reports changes made outside the root — host activity
//!    leaking into the sandbox's view, which `DESIGN.md` §3.1 forbids. Every
//!    `add_watch` passes `IN_DONT_FOLLOW`, and every reconstructed path is checked
//!    against the canonical root before it can become a [`Change`].
//!
//! ## No dependencies
//!
//! The inotify entry points are declared here as `extern "C"` symbols and resolved
//! against the libc Rust already links for `x86_64-unknown-linux-gnu`. Nothing is
//! fetched from a registry, so this is not the stop-and-ask dependency
//! `AGENT-RULES.md` §5 guards. Reasoning and the rejected alternatives (the `libc`
//! crate; calling `syscall()` directly; polling) are in `DECISIONS.md`.
//!
//! ## What this crate deliberately does not do
//!
//! No cage (2e), no gate (Phase 4), no effects, no SQLite, no log (Phase 3), no
//! UI, no agent-facing surface. It is not routed through the resolver: it takes a
//! **real** host path (`--root`), exactly as `snapshot/` does, and constructs no
//! real path from agent input.
//!
//! ## Linux only
//!
//! `DESIGN.md` targets this machine and v1 has no portability requirement. The
//! `extern "C"` symbols are Linux glibc symbols; this crate does not build
//! elsewhere and does not pretend to.

use std::collections::{HashMap, HashSet};
use std::ffi::{CString, OsStr, OsString};
use std::fmt;
use std::fs;
use std::io;
use std::os::raw::{c_char, c_int};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// The kernel interface — declared, not imported
// ---------------------------------------------------------------------------

extern "C" {
    fn inotify_init1(flags: c_int) -> c_int;
    fn inotify_add_watch(fd: c_int, pathname: *const c_char, mask: u32) -> c_int;
    fn inotify_rm_watch(fd: c_int, wd: c_int) -> c_int;
    fn read(fd: c_int, buf: *mut u8, count: usize) -> isize;
    fn __errno_location() -> *mut c_int;
}

fn errno() -> i32 {
    unsafe { *__errno_location() }
}

const IN_NONBLOCK: c_int = 0o4000;
const IN_CLOEXEC: c_int = 0o2000000;

/// A file's contents changed. **Consumed, not reported on its own** — see
/// [`Change::Modified`] and `attack-list-2d.md` §F.1.
const IN_MODIFY: u32 = 0x0000_0002;
/// An inode's attributes changed (permissions, owner, timestamps).
const IN_ATTRIB: u32 = 0x0000_0004;
/// A file was opened for writing and closed again. The one signal reported as
/// `Modified`.
const IN_CLOSE_WRITE: u32 = 0x0000_0008;
/// An entry was moved out of a watched directory.
const IN_MOVED_FROM: u32 = 0x0000_0040;
/// An entry was moved into a watched directory.
const IN_MOVED_TO: u32 = 0x0000_0080;
/// An entry was created in a watched directory.
const IN_CREATE: u32 = 0x0000_0100;
/// An entry was deleted from a watched directory.
const IN_DELETE: u32 = 0x0000_0200;
/// The watched directory itself was deleted.
const IN_DELETE_SELF: u32 = 0x0000_0400;
/// The watched directory itself was moved. **Ambiguous on its own**: it means the
/// directory this watch follows is somewhere else, not that it is gone.
const IN_MOVE_SELF: u32 = 0x0000_0800;
/// Events were lost: the queue filled. Arrives with `wd = -1`.
const IN_Q_OVERFLOW: u32 = 0x0000_4000;
/// The kernel has dropped this watch. The only signal that a watch ended.
const IN_IGNORED: u32 = 0x0000_8000;
/// Set on an event that concerns a directory.
const IN_ISDIR: u32 = 0x4000_0000;
/// Fail unless `pathname` is a directory.
const IN_ONLYDIR: u32 = 0x0100_0000;
/// Do not dereference `pathname` if it is a symlink.
const IN_DONT_FOLLOW: u32 = 0x0200_0000;

/// The mask placed on every watched directory.
///
/// `IN_ACCESS` is deliberately **absent**: a read is not a change, and including it
/// would make `ls`, `stat` and taking a snapshot flood the stream
/// (`attack-list-2d.md` §B.4–B.6, §I.7). `IN_MODIFY` is present because the kernel
/// pairs it with `CLOSE_WRITE`, and the close is what gets reported.
pub const WATCH_MASK: u32 = IN_ATTRIB
    | IN_CLOSE_WRITE
    | IN_MODIFY
    | IN_MOVED_FROM
    | IN_MOVED_TO
    | IN_CREATE
    | IN_DELETE
    | IN_DELETE_SELF
    | IN_MOVE_SELF;

/// One raw event as the kernel writes it. Layout is `struct inotify_event` from
/// `sys/inotify.h`; the name follows the fixed part, NUL-padded to alignment.
#[repr(C)]
#[derive(Clone, Copy)]
struct InotifyEvent {
    wd: c_int,
    mask: u32,
    cookie: u32,
    len: u32,
}

const EVENT_HEADER_BYTES: usize = std::mem::size_of::<InotifyEvent>();

// ---------------------------------------------------------------------------
// Errors and refusals
// ---------------------------------------------------------------------------

/// Everything that can go wrong starting a watcher, each naming the specific
/// problem. `attack-list-2d.md` §G requires a refusal to say *what* was wrong.
#[derive(Debug)]
pub enum WatchError {
    /// A usage error (exit 2): the arguments are wrong.
    Usage(String),
    /// The root could not be used.
    ///
    /// **This variant carries the real path on purpose.** The reader is the user,
    /// who needs to know which directory was refused — the same deliberate
    /// exception `snapshot/` makes (`DECISIONS.md`). The prohibition is on the
    /// *change stream*, not on a refusal (`attack-list-2d.md` §H.2).
    Root { path: PathBuf, detail: String },
}

impl WatchError {
    pub fn exit_code(&self) -> u8 {
        match self {
            WatchError::Usage(_) => 2,
            WatchError::Root { .. } => 1,
        }
    }
}

impl fmt::Display for WatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WatchError::Usage(s) => write!(f, "usage: {s}"),
            WatchError::Root { path, detail } => {
                write!(f, "cannot watch {}: {detail}", path.display())
            }
        }
    }
}

impl std::error::Error for WatchError {}

// ---------------------------------------------------------------------------
// The change record
// ---------------------------------------------------------------------------

/// Why a watch ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoneReason {
    /// The watched directory was deleted (`IN_DELETE_SELF`).
    Deleted,
    /// The watched directory was moved (`IN_MOVE_SELF`) and its new location
    /// could not be found inside the root.
    Moved,
    /// The kernel dropped the watch (`IN_IGNORED` alone).
    Ignored,
    /// The watch's directory can no longer be found anywhere inside the root, and
    /// the kernel did not say which of the two it was. **The honest reason**: a
    /// directory moved out of the root and one deleted are not distinguishable
    /// from the watch's own event stream alone.
    Vanished,
}

impl GoneReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            GoneReason::Deleted => "deleted",
            GoneReason::Moved => "moved",
            GoneReason::Ignored => "dropped",
            GoneReason::Vanished => "no longer inside the environment (deleted, or moved out)",
        }
    }
}

/// One thing that happened inside the environment root.
///
/// These are **not** effects. `DESIGN.md` §6.3's fields (`kind`, `count`, `label`,
/// `surface`) and the effect kinds themselves belong to Phase 3; this is the raw
/// observation the effect stream will be built from. Keeping them separate is why
/// this crate can be verified on its own.
///
/// Every path here is a **virtual** path (the agent's view: `/home/documents/x`),
/// never the root's real location on disk, unless [`Options::show_real`] is set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// A new entry appeared in the tree.
    Created { path: OsString, is_dir: bool },
    /// An existing file's contents changed. Emitted **once per edit**, at close.
    Modified { path: OsString },
    /// An entry that existed no longer does.
    Deleted { path: OsString, is_dir: bool },
    /// An entry left the directory it was in. Carries the kernel's pairing cookie.
    MovedFrom {
        path: OsString,
        cookie: u32,
        is_dir: bool,
    },
    /// An entry arrived in the directory it is now in.
    MovedTo {
        path: OsString,
        cookie: u32,
        is_dir: bool,
    },
    /// Metadata changed — most often permissions.
    Attributed { path: OsString },
    /// **A watch ended.** The kernel will say nothing more about this directory.
    /// Silently dropping this would make a dead watch look exactly like an idle one.
    DirectoryGone { dir: OsString, reason: GoneReason },
    /// **Events were lost.** The kernel's queue filled and it dropped changes it had
    /// not delivered. `dropped_before` is how many events had been drained
    /// successfully before the gap — the only honest thing that can be said.
    Overflow { dropped_before: u64 },
    /// **Something is wrong with the watching itself** — most seriously, an event
    /// whose path does not resolve under the root. Should be impossible; if it
    /// happens it is reported loudly rather than rendered as a change.
    Error { detail: String },
}

impl Change {
    /// A **fault** is a record meaning the report is not complete: events were lost,
    /// a watch ended, or something outside the root was seen. A run that produced
    /// any fault is not a clean run, and the CLI exits non-zero for it
    /// (`attack-list-2d.md` §J.2, §K.7).
    pub fn is_fault(&self) -> bool {
        matches!(
            self,
            Change::Overflow { .. } | Change::Error { .. } | Change::DirectoryGone { .. }
        )
    }

    /// The path this record concerns, if it has one.
    pub fn path(&self) -> Option<&OsStr> {
        match self {
            Change::Created { path, .. }
            | Change::Modified { path }
            | Change::Deleted { path, .. }
            | Change::Attributed { path } => Some(path),
            Change::MovedFrom { path, .. } | Change::MovedTo { path, .. } => Some(path),
            Change::DirectoryGone { dir, .. } => Some(dir),
            Change::Overflow { .. } | Change::Error { .. } => None,
        }
    }

    /// The one-word tag used in the line format, stable and greppable.
    pub fn tag(&self) -> &'static str {
        match self {
            Change::Created { .. } => "CREATE",
            Change::Modified { .. } => "MODIF",
            Change::Deleted { .. } => "DELETE",
            Change::MovedFrom { .. } => "MOVEFROM",
            Change::MovedTo { .. } => "MOVETO",
            Change::Attributed { .. } => "ATTRIB",
            Change::DirectoryGone { .. } => "GONE",
            Change::Overflow { .. } => "OVERFLOW",
            Change::Error { .. } => "ERROR",
        }
    }

    /// The report line for this record. One record per line, greppable by its tag.
    pub fn line(&self) -> String {
        match self {
            Change::Created { path, is_dir } => {
                format!("CREATE   {}{}", render_path(path), suffix(*is_dir))
            }
            Change::Modified { path } => format!("MODIF    {}", render_path(path)),
            Change::Deleted { path, is_dir } => {
                format!("DELETE   {}{}", render_path(path), suffix(*is_dir))
            }
            Change::MovedFrom {
                path,
                cookie,
                is_dir,
            } => format!(
                "MOVEFROM {} cookie={cookie}{}",
                render_path(path),
                suffix(*is_dir)
            ),
            Change::MovedTo {
                path,
                cookie,
                is_dir,
            } => format!(
                "MOVETO   {} cookie={cookie}{}",
                render_path(path),
                suffix(*is_dir)
            ),
            Change::Attributed { path } => format!("ATTRIB   {}", render_path(path)),
            Change::DirectoryGone { dir, reason } => {
                format!("GONE     {} ({})", render_path(dir), reason.as_str())
            }
            Change::Overflow { dropped_before } => {
                format!("OVERFLOW dropped-before={dropped_before}")
            }
            Change::Error { detail } => format!("ERROR    {detail}"),
        }
    }
}

fn suffix(is_dir: bool) -> &'static str {
    if is_dir {
        " dir"
    } else {
        ""
    }
}

// ---------------------------------------------------------------------------
// Rendering a path: bytes in, bytes out
// ---------------------------------------------------------------------------

/// Render a path for the report.
///
/// A filename on Linux is an **arbitrary byte string** and is not guaranteed to be
/// valid UTF-8 (`attack-list-2d.md` §C.4). Converting lossily would print
/// `bad-??-name.txt` for `bad-\xff\xfe-name.txt`, and **two distinct files could
/// print identically** (§C.5). So valid UTF-8 is printed as itself, and any other
/// byte as `\xNN` — reversible, so distinct names stay distinct.
///
/// A newline in a name is a real case (§C.3) and is escaped rather than allowed to
/// break the one-record-per-line guarantee.
pub fn render_path(p: &OsStr) -> String {
    match std::str::from_utf8(p.as_bytes()) {
        Ok(s) if !s.contains('\n') && !s.contains('\r') => s.to_string(),
        _ => {
            let mut out = String::new();
            for &b in p.as_bytes() {
                match b {
                    b'\n' => out.push_str("\\n"),
                    b'\r' => out.push_str("\\r"),
                    b'\t' => out.push_str("\\t"),
                    0x20..=0x7e => out.push(b as char),
                    _ => out.push_str(&format!("\\x{b:02X}")),
                }
            }
            out
        }
    }
}

// ---------------------------------------------------------------------------
// The watcher
// ---------------------------------------------------------------------------

/// Options for a watcher.
#[derive(Debug, Clone)]
pub struct Options {
    /// The environment root, a **real host path** (not a virtual one).
    pub root: PathBuf,
    /// Print real host paths instead of virtual ones.
    ///
    /// **Off by default, and that default is the point.** `DESIGN.md` §3.1: the
    /// agent never learns the real path exists. The exception is for the human
    /// reading the tool's output and matches `shell/`'s `--show-real`.
    pub show_real: bool,
    /// How long the reader waits for more events before reporting the batch.
    ///
    /// **A batching delay, not a completeness mechanism.** The kernel coalesces
    /// nothing (§B.3: 500 creates produced 500 records), so no wait is needed for
    /// accuracy. It exists only so a burst prints as a batch.
    pub settle_ms: u64,
}

impl Options {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Options {
            root: root.into(),
            show_real: false,
            settle_ms: 150,
        }
    }
}

/// A directory being watched, and what it is called.
#[derive(Debug, Clone)]
struct WatchedDir {
    /// Real path on disk, as last derived from the tree.
    real: PathBuf,
    /// The path as the agent would name it (real path minus the canonical root).
    virt: OsString,
    /// The inode. **The identity that matters**: a `wd` follows an inode, and a
    /// path is only a name that inode currently has.
    ino: u64,
}

/// The watcher. Owns the inotify instance and the `wd` → directory table.
///
/// ## Why the `wd` table is the dangerous part
///
/// `inotify_add_watch` returns a small integer (`wd`) and the kernel **never tells
/// you the path again**. Every event is `(wd, name)`, so a path is reconstructed as
/// `table[wd] + "/" + name`. The table is load-bearing, and it goes wrong in ways
/// that were **measured**, not assumed (15 Sep 2026):
///
/// - **A rename does not re-key it.** After `mv root/old root/new`, a write inside
///   the renamed directory still arrives **on the old wd number** — a wd follows an
///   *inode*. Observed: `wd=2 CREATE name="after.txt"` after the rename.
/// - **A watch can die.** When the root is replaced by a rename the kernel sends
///   `ATTRIB`, `DELETE_SELF`, `IGNORED` and then nothing — observed: later writes
///   produced **zero** events.
/// - **A moved-out watch keeps reporting.** Observed, and this is the sharp one: a
///   watched directory moved *out of the root* **keeps its watch and its children's
///   watches**, and writes made outside the root then arrive on those wds. A
///   watcher that trusted its stale table would name an **in-root** path for
///   activity that happened outside the environment — a change reported that did
///   not happen in the environment at all.
///
/// The answer to all three is the same: [`Watcher::resync`] re-derives every
/// watch's path **from the tree** by inode, rather than trusting the table across a
/// move. A watch whose inode is no longer anywhere inside the root is removed and
/// reported, because continuing to use it would attribute outside activity to an
/// in-root path.
pub struct Watcher {
    file: fs::File,
    /// Canonical root. Every reported path is checked against this.
    canon_root: PathBuf,
    /// `wd` → directory.
    dirs: HashMap<c_int, WatchedDir>,
    /// wds whose path may be stale, because a directory was moved.
    suspects: HashSet<c_int>,
    /// wds we have explicitly removed; late events for these are stragglers and are
    /// ignored rather than reported as anomalies.
    retired: HashSet<c_int>,
    /// How many events have been drained so far, for the overflow record's count.
    drained: u64,
    /// How many times a resync has run. Reported by the CLI; a resync is not an
    /// error, but it is a thing that happened.
    resyncs: u64,
    show_real: bool,
    /// Watches placed at startup that failed, reported once by the first drain.
    unwatchable: Vec<(OsString, String)>,
}

impl Watcher {
    /// Start watching `opts.root`, placing one watch per directory in the tree.
    ///
    /// Fails only if the root itself cannot be used. A **subdirectory** that cannot
    /// be watched is not a failure: it is recorded and reported as a
    /// [`Change::Error`] by the first [`Watcher::drain`], because one unreadable
    /// directory must not blind the whole root (`attack-list-2d.md` §D.7).
    pub fn start(opts: &Options) -> Result<Watcher, WatchError> {
        let root = &opts.root;

        if root.as_os_str().is_empty() {
            return Err(WatchError::Usage("--root must not be empty".into()));
        }
        // A relative root is refused rather than guessed at — the same rule
        // `snapshot/` applies to host paths (§G.3).
        if !root.is_absolute() {
            return Err(WatchError::Usage(format!(
                "--root must be an absolute path, got '{}'",
                root.display()
            )));
        }

        let md = fs::metadata(root).map_err(|e| WatchError::Root {
            path: root.clone(),
            detail: e.to_string(),
        })?;
        if !md.is_dir() {
            return Err(WatchError::Root {
                path: root.clone(),
                detail: "not a directory".to_string(),
            });
        }

        // `canonicalize` is correct **here and only here**: the root itself is
        // supplied by the app, and every reported path must be compared against a
        // canonical form. It is never applied to a path we are about to report.
        let canon_root = fs::canonicalize(root).map_err(|e| WatchError::Root {
            path: root.clone(),
            detail: e.to_string(),
        })?;

        let fd = unsafe { inotify_init1(IN_NONBLOCK | IN_CLOEXEC) };
        if fd < 0 {
            let e = io::Error::last_os_error();
            return Err(WatchError::Root {
                path: root.clone(),
                detail: format!("inotify_init1: {e}"),
            });
        }
        let file = unsafe { fs::File::from_raw_fd(fd) };

        let mut w = Watcher {
            file,
            canon_root,
            dirs: HashMap::new(),
            suspects: HashSet::new(),
            retired: HashSet::new(),
            drained: 0,
            resyncs: 0,
            show_real: opts.show_real,
            unwatchable: Vec::new(),
        };
        let r = w.canon_root.clone();
        w.walk_and_watch(&r);

        // **Define the moment the report begins, and discard what precedes it.**
        //
        // A real defect, found by this phase's own test suite flaking — not by
        // reasoning. `inotify` delivers the three events of one `open`/`write`/`close`
        // at three different moments, and `add_watch` can land *between* them:
        // observed, a file written just before the walk placed its watch delivered
        // its `IN_CLOSE_WRITE` alone, with no `IN_CREATE` and no `IN_MODIFY`, after
        // the watch existed — a change reported for a file nothing had touched since
        // the watcher started (same inode, same length, same mtime to the
        // nanosecond).
        //
        // Reporting it would attribute a change to a moment it did not happen in.
        // The honest boundary is: **the watcher reports changes from the moment
        // `start` returns.** Whatever the kernel queued while the walk was running
        // belongs to the state this watcher is starting from, so it is drained and
        // discarded here rather than surfacing later as a phantom edit.
        //
        // The initial tree is not lost by this: `drain` starts from the live
        // filesystem, and anything genuinely changed after this point still arrives.
        //
        // **Only kernel events are discarded.** A directory that could not be
        // watched is a fact about this watcher's own coverage, owned by the caller's
        // first `drain`, so it is put back rather than swallowed here. (Found by
        // running the suite: discarding everything here silently swallowed that
        // report, which is the exact failure §D.7 forbids.)
        let unwatchable = std::mem::take(&mut w.unwatchable);
        let _ = w.drain();
        w.unwatchable = unwatchable;
        Ok(w)
    }

    /// The canonical root.
    pub fn root(&self) -> &Path {
        &self.canon_root
    }

    /// How many watches are held.
    pub fn live_watches(&self) -> usize {
        self.dirs.len()
    }

    /// How many directories were seen but could not be watched at startup.
    pub fn unwatchable(&self) -> &[(OsString, String)] {
        &self.unwatchable
    }

    /// How many path re-derivations have run.
    pub fn resyncs(&self) -> u64 {
        self.resyncs
    }

    /// Walk `dir` and place a watch on every directory under it.
    ///
    /// `symlink_metadata`, never `metadata`: a directory reached **through** a
    /// symlink is not descended into. Combined with `IN_DONT_FOLLOW` on every
    /// `add_watch`, this is what keeps a watch from being placed on something
    /// outside the root (`DESIGN.md` §3.1). Iterative, so a deep tree cannot blow
    /// the stack.
    fn walk_and_watch(&mut self, dir: &Path) {
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            self.add_watch(&d);
            let rd = match fs::read_dir(&d) {
                Ok(rd) => rd,
                Err(e) => {
                    // A directory we cannot list is reported, not skipped silently.
                    self.unwatchable
                        .push((self.virtual_of(&d), format!("read_dir: {e}")));
                    continue;
                }
            };
            for ent in rd.flatten() {
                let p = ent.path();
                // symlink_metadata: do not follow. A symlink to a directory is NOT
                // descended into, and no watch is placed through it.
                if let Ok(md) = fs::symlink_metadata(&p) {
                    if md.is_dir() {
                        stack.push(p);
                    }
                }
            }
        }
    }

    /// Place one watch on `dir`, recording the outcome.
    fn add_watch(&mut self, dir: &Path) {
        let c = match CString::new(dir.as_os_str().as_bytes()) {
            Ok(c) => c,
            Err(_) => {
                // A NUL in a path cannot be expressed to the kernel at all.
                self.unwatchable
                    .push((self.virtual_of(dir), "path contains a NUL byte".into()));
                return;
            }
        };
        let mask = WATCH_MASK | IN_DONT_FOLLOW | IN_ONLYDIR;
        let wd = unsafe { inotify_add_watch(self.file.as_raw_fd(), c.as_ptr(), mask) };
        if wd < 0 {
            let e = errno();
            self.unwatchable.push((
                self.virtual_of(dir),
                format!(
                    "inotify_add_watch: {} (errno {e})",
                    io::Error::from_raw_os_error(e)
                ),
            ));
            return;
        }
        // The inode is this watch's real identity; the path is only its current
        // name. `symlink_metadata` because we must not follow a link here either.
        let ino = match fs::symlink_metadata(dir) {
            Ok(md) => md.ino(),
            Err(_) => 0,
        };
        self.retired.remove(&wd);
        self.dirs.insert(
            wd,
            WatchedDir {
                real: dir.to_path_buf(),
                virt: self.virtual_of(dir),
                ino,
            },
        );
    }

    /// The agent's name for a real path: the real path with the canonical root
    /// prefix removed, or `/` for the root itself.
    ///
    /// This is the whole of the disclosure boundary. `DESIGN.md` §3.1: the agent
    /// never learns the real path exists.
    pub fn virtual_of(&self, real: &Path) -> OsString {
        match real.strip_prefix(&self.canon_root) {
            Ok(rest) if rest.as_os_str().is_empty() => OsString::from("/"),
            Ok(rest) => {
                let mut s = OsString::from("/");
                s.push(rest);
                s
            }
            // Not under the root. The caller turns this into an ERROR rather than
            // printing a real path.
            Err(_) => OsString::from("<outside the environment>"),
        }
    }

    /// The path to put in a report: virtual by default, real only on request.
    fn report_path(&self, real: &Path) -> OsString {
        if self.show_real {
            real.as_os_str().to_os_string()
        } else {
            self.virtual_of(real)
        }
    }

    /// Reconstruct the real path an event names: `table[wd] + "/" + name`.
    ///
    /// The containment check lives here, so **no change record can be built from a
    /// path outside the root** (§H.5).
    fn event_path(&self, wd: c_int, name: &[u8]) -> Result<PathBuf, Change> {
        let Some(dir) = self.dirs.get(&wd) else {
            return Err(Change::Error {
                detail: format!("event for an unknown watch descriptor {wd}"),
            });
        };
        let mut real = dir.real.clone();
        if !name.is_empty() {
            real.push(OsStr::from_bytes(name));
        }
        if !real.starts_with(&self.canon_root) {
            return Err(Change::Error {
                detail: format!(
                    "a change outside the environment root was seen via watch {wd}; refusing to \
                     report it as a change. This should be impossible with IN_DONT_FOLLOW, so \
                     the watching itself is wrong"
                ),
            });
        }
        Ok(real)
    }

    /// Read everything waiting, and turn it into [`Change`] records.
    ///
    /// Never blocks: the descriptor is non-blocking, so this returns when the queue
    /// is empty.
    pub fn drain(&mut self) -> Vec<Change> {
        let mut out = Vec::new();

        // Watches that failed at startup are reported first, once, and each one says
        // plainly that changes there will not be seen.
        for (virt, detail) in std::mem::take(&mut self.unwatchable) {
            out.push(Change::Error {
                detail: format!(
                    "{} cannot be watched ({detail}); changes in it will NOT be reported",
                    render_path(&virt)
                ),
            });
        }

        let mut buf = vec![0u64; 65536 / 8];
        loop {
            let n = unsafe {
                read(
                    self.file.as_raw_fd(),
                    buf.as_mut_ptr() as *mut u8,
                    buf.len() * 8,
                )
            };
            if n <= 0 {
                break;
            }
            let n = n as usize;
            let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, n) };
            let mut off = 0usize;
            while off + EVENT_HEADER_BYTES <= n {
                let ev = unsafe { &*(bytes.as_ptr().add(off) as *const InotifyEvent) };
                let name_start = off + EVENT_HEADER_BYTES;
                let name_len = ev.len as usize;
                let name: Vec<u8> = if name_len > 0 && name_start + name_len <= n {
                    let sl = &bytes[name_start..name_start + name_len];
                    // The kernel NUL-pads to alignment; the name ends at the first NUL.
                    let end = sl.iter().position(|b| *b == 0).unwrap_or(sl.len());
                    sl[..end].to_vec()
                } else {
                    Vec::new()
                };
                self.drained += 1;
                off = name_start + name_len;

                // IN_Q_OVERFLOW arrives with wd = -1 and is the only trace of a gap.
                // Handled first, deliberately: nothing else about this batch can be
                // trusted to be complete.
                if ev.mask & IN_Q_OVERFLOW != 0 || ev.wd == -1 {
                    out.push(Change::Overflow {
                        dropped_before: self.drained.saturating_sub(1),
                    });
                    // The table may be wrong in ways we cannot see, so re-derive it.
                    self.suspects.extend(self.dirs.keys().copied());
                    continue;
                }

                // A straggler for a watch we deliberately removed is expected right
                // after a removal and is not an anomaly.
                if self.retired.contains(&ev.wd) {
                    continue;
                }

                if let Some(c) = self.one_event(ev, &name) {
                    out.push(c);
                }
            }
        }

        // Re-derive paths for any watch whose directory moved. This is what stops a
        // moved-out subtree from being reported under its old in-root path.
        if !self.suspects.is_empty() {
            let mut gone = self.resync();
            out.append(&mut gone);
        }

        out
    }

    /// Re-derive every watch's path **from the tree**, by inode.
    ///
    /// This is the answer to a `wd` following an inode rather than a path, and to
    /// the fact — **observed** — that a watch keeps reporting after its directory is
    /// moved out of the root. Trusting the table across a move is what would make
    /// this watcher report outside activity as an in-root change.
    ///
    /// What it does:
    ///
    /// 1. Walks the root (`symlink_metadata`, no follow) and maps inode → path.
    /// 2. For each watch: if its inode is in the tree, its path is corrected. If its
    ///    inode is **not** in the tree, the watch is removed with
    ///    [`GoneReason::Vanished`] — we cannot tell a deletion from a move out of
    ///    the root, and the kernel does not say, so the report does not pretend.
    /// 3. Places watches for any in-root directory that has none, which also closes
    ///    the new-directory race for any directory that existed when this ran.
    ///
    /// Returns the records for watches that ended. A resync is not itself an error.
    fn resync(&mut self) -> Vec<Change> {
        self.resyncs += 1;

        // 1. The tree, as inode → path.
        let mut by_ino: HashMap<u64, PathBuf> = HashMap::new();
        let mut stack = vec![self.canon_root.clone()];
        while let Some(d) = stack.pop() {
            if let Ok(md) = fs::symlink_metadata(&d) {
                if md.is_dir() {
                    by_ino.entry(md.ino()).or_insert_with(|| d.clone());
                }
            }
            if let Ok(rd) = fs::read_dir(&d) {
                for ent in rd.flatten() {
                    let p = ent.path();
                    if let Ok(md) = fs::symlink_metadata(&p) {
                        if md.is_dir() {
                            stack.push(p);
                        }
                    }
                }
            }
        }

        // 2. Correct or retire every watch.
        let mut out = Vec::new();
        let mut known: HashSet<u64> = HashSet::new();
        let mut to_remove: Vec<c_int> = Vec::new();
        // The virtual name is computed through `self`, so it is taken before the
        // table is borrowed mutably.
        let canon = self.canon_root.clone();
        let virt_of = |real: &Path| -> OsString {
            match real.strip_prefix(&canon) {
                Ok(rest) if rest.as_os_str().is_empty() => OsString::from("/"),
                Ok(rest) => {
                    let mut s = OsString::from("/");
                    s.push(rest);
                    s
                }
                Err(_) => OsString::from("<outside the environment>"),
            }
        };
        for (wd, dir) in self.dirs.iter_mut() {
            match by_ino.get(&dir.ino) {
                Some(new_path) if new_path != &dir.real => {
                    dir.real = new_path.clone();
                    dir.virt = virt_of(new_path);
                    known.insert(dir.ino);
                }
                Some(_) => {
                    known.insert(dir.ino);
                }
                None => {
                    out.push(Change::DirectoryGone {
                        dir: dir.virt.clone(),
                        reason: GoneReason::Vanished,
                    });
                    to_remove.push(*wd);
                }
            }
        }
        for wd in to_remove {
            let _ = unsafe { inotify_rm_watch(self.file.as_raw_fd(), wd) };
            self.dirs.remove(&wd);
            self.retired.insert(wd);
        }

        // 3. Watch any in-root directory that has no watch.
        for (ino, path) in &by_ino {
            if !known.contains(ino) && !self.dirs.values().any(|d| d.ino == *ino) {
                self.add_watch(path);
            }
        }

        self.suspects.clear();
        out
    }

    /// Turn one kernel event into at most one [`Change`].
    ///
    /// Returns `None` for events deliberately consumed rather than reported
    /// (`IN_MODIFY` alone, and duplicate death notices for one watch).
    fn one_event(&mut self, ev: &InotifyEvent, name: &[u8]) -> Option<Change> {
        let is_dir = ev.mask & IN_ISDIR != 0;
        let wd = ev.wd;

        // -- a directory moved: its path is now unknown, so re-derive it -----
        //
        // Deliberately NOT reported as gone. Observed: MOVE_SELF arrives on a
        // directory's own wd even for a rename *within* the root, where the watch
        // stays perfectly valid. The resync decides which it was.
        if ev.mask & IN_MOVE_SELF != 0 {
            self.suspects.insert(wd);
            return None;
        }

        // -- the watched directory is gone -----------------------------------
        if ev.mask & IN_DELETE_SELF != 0 {
            let virt = self
                .dirs
                .get(&wd)
                .map(|d| self.report_path(&d.real))
                .unwrap_or_else(|| OsString::from("/"));
            let _ = unsafe { inotify_rm_watch(self.file.as_raw_fd(), wd) };
            self.dirs.remove(&wd);
            self.retired.insert(wd);
            return Some(Change::DirectoryGone {
                dir: virt,
                reason: GoneReason::Deleted,
            });
        }
        if ev.mask & IN_IGNORED != 0 {
            // IGNORED always follows a watch the kernel dropped, including after our
            // own removal. If the entry is already gone, this is the tail of a death
            // already reported, so it is not reported twice.
            let entry = self.dirs.remove(&wd);
            self.retired.insert(wd);
            return entry.map(|d| Change::DirectoryGone {
                dir: if self.show_real {
                    d.real.as_os_str().to_os_string()
                } else {
                    d.virt
                },
                reason: GoneReason::Ignored,
            });
        }

        // -- an ordinary entry event -----------------------------------------
        let real = match self.event_path(wd, name) {
            Ok(r) => r,
            Err(e) => return Some(e),
        };
        let virt = self.report_path(&real);

        if ev.mask & IN_CREATE != 0 {
            if is_dir {
                // Watch it now, so its contents are seen. The remaining window is one
                // loop turn and is a recorded race (§D.3), not papered over. A walk
                // rather than a single add, so contents that appeared in the same
                // instant are covered too.
                self.walk_and_watch(&real);
            }
            return Some(Change::Created { path: virt, is_dir });
        }
        if ev.mask & IN_DELETE != 0 {
            return Some(Change::Deleted { path: virt, is_dir });
        }
        if ev.mask & IN_MOVED_FROM != 0 {
            if is_dir {
                // Its wd (and its children's) are now at an unknown path.
                self.mark_subtree_suspect(&real);
            }
            return Some(Change::MovedFrom {
                path: virt,
                cookie: ev.cookie,
                is_dir,
            });
        }
        if ev.mask & IN_MOVED_TO != 0 {
            if is_dir {
                self.mark_subtree_suspect(&real);
                // **Observed defect, found by the independent blind list (§1.3).**
                // A single `add_watch` here watches the top of a moved-in tree and
                // nothing below it: `mv bigdir/ root/` delivered one MOVED_TO, and
                // writes inside `bigdir/sub/` were invisible **forever** — the tree
                // view shows a populated folder as one empty directory, and nothing
                // ever re-walks it. The whole subtree has to be walked.
                self.walk_and_watch(&real);
            }
            return Some(Change::MovedTo {
                path: virt,
                cookie: ev.cookie,
                is_dir,
            });
        }
        if ev.mask & IN_CLOSE_WRITE != 0 {
            // The one reported signal for "contents changed". IN_MODIFY is consumed
            // below, so a three-write edit produces one record.
            return Some(Change::Modified { path: virt });
        }
        if ev.mask & IN_ATTRIB != 0 {
            return Some(Change::Attributed { path: virt });
        }

        // IN_MODIFY alone: consumed on purpose. Reporting it would produce one
        // record per write buffer for a single logical edit (§F.1).
        None
    }

    /// Mark every watch at or under `path` as needing a path re-derivation.
    ///
    /// Called when a directory moves, because that subtree's watches keep reporting
    /// on their old wds while their paths have changed — or, if the move was out of
    /// the root, while they are no longer inside it at all.
    fn mark_subtree_suspect(&mut self, path: &Path) {
        let hit: Vec<c_int> = self
            .dirs
            .iter()
            .filter(|(_, d)| d.real == path || d.real.starts_with(path))
            .map(|(wd, _)| *wd)
            .collect();
        self.suspects.extend(hit);
    }
}
