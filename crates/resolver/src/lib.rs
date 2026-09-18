//! atrium-resolver — the Atrium sandbox path resolver (Phases 1 + 1b).
//!
//! DESIGN.md §3.2: every virtual path an agent supplies is resolved and
//! checked against the environment root before anything runs.
//!
//! Rules implemented here (Phase 1 brief §8.3, Phase 1b brief §9.4):
//! - Virtual paths are absolute within the environment. Relative paths are
//!   rejected with a reason, not silently rooted.
//! - Resolve fully, then check — never check, then resolve. Containment is
//!   verified at every intermediate step, not just the final target. In
//!   particular `..` is resolved by the filesystem (via `canonicalize`) at
//!   each step, AFTER any symlinks in the prefix have been followed — a
//!   purely lexical `..` collapse would let `/link-out/../home` pass.
//! - Symlinks are canonicalised before the containment check, including
//!   chains and loops. Every hop of a chain is containment-checked, not just
//!   the final location: a chain that leaves the root at any hop is refused
//!   even if it comes back in (ruling 5, 18 Sep 2026 — docs/DECISIONS.md). A
//!   target may be *spelled* through the root's ancestors (that is how an
//!   absolute path is written), but a hop that *lands* on an ancestor has left
//!   the root and is refused.
//! - Lexical attacks (NUL bytes, relative paths, empty paths) are rejected
//!   with per-case reasons before any filesystem call.
//! - Phase 1b: a path whose target does not exist is ACCEPTED as long as
//!   its resolved location sits inside the root. A component that does not
//!   exist is normalised textually (`//` collapses, `.` elided, `x/..`
//!   removes `x`), and a `..` that would climb above the root is rejected
//!   naming the step.
//! - Textual handling applies ONLY to components that genuinely do not
//!   exist. The walk stays on the filesystem for every component that can be
//!   reached: an absent component earlier in the path never stops a later one
//!   from being canonicalised and containment-checked, and a `..` that
//!   returns the walk to an existing position is resolved against the disk
//!   again. (Corrected 18 Sep 2026 -- a component that exists on disk is
//!   always canonicalised and checked, whatever came before it. The earlier
//!   walk treated the first not-found component as switching the whole
//!   remainder to textual mode, so a path like `/nope/../NAME`, where NAME
//!   is a symlink pointing outside the root, reached that symlink without it
//!   ever being followed. See `tests/regression_textual_escape.rs` and
//!   docs/STATUS.md.)
//! - A dangling symlink is still a symlink: its target is what matters,
//!   whether or not the target exists. A target outside the root rejects,
//!   whatever the target's existence (ruling K.1a).
//! - Component length is checked by the resolver itself, in bytes: any
//!   single component over 255 bytes is rejected with the resolver's own
//!   reason, whether the path exists or not (DECISIONS.md, "Phase 1b —
//!   three resolver rulings"; brief §9.4 item 10). PATH_MAX is not
//!   enforced.
//!
//! This function never creates, modifies, or deletes anything on the
//! filesystem. It only reads (canonicalises / stats) metadata.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::{Component, Path, PathBuf};

/// Linux NAME_MAX: a single path component may be at most 255 bytes.
/// Bytes, not characters — measured on the UTF-8 encoding. Enforced by the
/// resolver itself so the verdict is identical whether the path exists or
/// not (Phase 1b, item 10).
const NAME_MAX_BYTES: usize = 255;

/// Ceiling for the resolver's OWN symlink-hop walk (`walk_link_target`),
/// which checks containment at every hop rather than letting the filesystem
/// follow the whole chain at once. Linux uses MAXSYMLINKS 40 for the chain
/// it follows; this mirrors that ceiling so a loop is refused rather than
/// walked forever.
const MAX_LINK_HOPS: usize = 40;

/// Structured rejection. `Display` gives a human-readable reason naming the
/// offending component and the rule violated (Phase 1 brief §8.3 item 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    /// The virtual path was the empty string.
    EmptyPath,
    /// The virtual path was not absolute within the environment.
    RelativePath { input: String },
    /// The path contains a NUL byte. Defense in depth: the OS would reject
    /// it too, but the resolver names it with its own reason (Phase 1 §8.6).
    NullByte { input: String },
    /// A single component exceeds 255 bytes (NAME_MAX, in bytes). Checked
    /// by the resolver itself, never delegated to the filesystem, so the
    /// verdict is the same for existing and absent paths.
    NameTooLong { component: String, bytes: usize },
    /// A `..` component climbed above the environment root (the resolved
    /// prefix landed outside the root after that step).
    TraversalAboveRoot { component: String, reached: String },
    /// A step resolved outside the environment root without a symlink being
    /// the cause (e.g. the first `..` from the root).
    EscapesRoot { input: String, reached: String },
    /// A symlink inside the root points outside it.
    SymlinkEscapes { link: String, target: String },
    /// A symlink chain leaves the environment root at an intermediate step
    /// and returns to it, so the path that comes out is inside while the
    /// traversal was not. Strictness wins: the chain is refused, naming the
    /// hop that left. (Ruling, 18 Sep 2026 — docs/DECISIONS.md.)
    SymlinkChainLeavesRoot { link: String, target: String },
    /// A symlink chain loops (ELOOP from the operating system, or a loop
    /// among dangling links the resolver followed itself).
    SymlinkLoop { link: String },
    /// The path (or a component of it) does not exist inside the
    /// environment. Under Phase 1b absence inside the root is accepted;
    /// this remains for API stability and for cases where absence still
    /// cannot be resolved (none in the current walk).
    NotFound { path: String },
    /// The operating system refused the path while resolving (e.g. a
    /// component that exists but is not a directory — ENOTDIR).
    OsError { path: String, reason: String },
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResolveError::EmptyPath => {
                write!(
                    f,
                    "Rejected: the empty path gives nothing to resolve within the environment root"
                )
            }
            ResolveError::RelativePath { input } => {
                write!(
                    f,
                    "Rejected: `{}` is a relative path; virtual paths must be absolute within the environment (start with `/`)",
                    input
                )
            }
            ResolveError::NullByte { input } => {
                write!(
                    f,
                    "Rejected: `{}` contains a NUL byte (U+0000); NUL is never valid in a path component",
                    input
                )
            }
            ResolveError::NameTooLong { component, bytes } => {
                write!(
                    f,
                    "Rejected: the component `{}` is {} bytes long; a single path component may be at most {} bytes (NAME_MAX)",
                    truncate_for_display(component),
                    bytes,
                    NAME_MAX_BYTES
                )
            }
            ResolveError::TraversalAboveRoot { component, reached } => {
                write!(
                    f,
                    "Rejected: the `..` step `{}` climbed above the environment root (this step reached `{}`, which is outside it)",
                    component, reached
                )
            }
            ResolveError::EscapesRoot { input, reached } => {
                write!(
                    f,
                    "Rejected: `{}` escapes the environment root (this step reached `{}`)",
                    input, reached
                )
            }
            ResolveError::SymlinkEscapes { link, target } => {
                write!(
                    f,
                    "Rejected: symlink `{}` points outside the environment root (its target is `{}`)",
                    link, target
                )
            }
            ResolveError::SymlinkChainLeavesRoot { link, target } => {
                write!(
                    f,
                    "Rejected: the symlink chain through `{}` leaves the environment root (it reached `{}`, outside it) -- a chain that leaves the root is refused even if it later returns",
                    link, target
                )
            }
            ResolveError::SymlinkLoop { link } => {
                write!(
                    f,
                    "Rejected: symlink `{}` is part of a symlink loop or an over-long chain (the operating system or the resolver reported too many levels)",
                    link
                )
            }
            ResolveError::NotFound { path } => {
                write!(
                    f,
                    "Rejected: `{}` does not exist inside the environment root",
                    path
                )
            }
            ResolveError::OsError { path, reason } => {
                write!(
                    f,
                    "Rejected: the operating system refused `{}` while resolving (`{}`)",
                    path, reason
                )
            }
        }
    }
}

impl std::error::Error for ResolveError {}

/// Keep a reason string readable when the offending component is hundreds
/// of bytes long: show the head of the component, not all of it.
fn truncate_for_display(s: &str) -> String {
    const MAX: usize = 64;
    if s.chars().count() <= MAX {
        s.to_string()
    } else {
        let head: String = s.chars().take(MAX).collect();
        format!("{}…", head)
    }
}

/// One meaningful path component: RootDir and CurDir contribute nothing
/// and are dropped when the path is split.
#[derive(Debug, Clone)]
enum Piece {
    Parent,
    Name(OsString),
}

impl Piece {
    fn os(&self) -> &OsStr {
        match self {
            Piece::Parent => OsStr::new(".."),
            Piece::Name(n) => n.as_os_str(),
        }
    }
    fn is_parent(&self) -> bool {
        matches!(self, Piece::Parent)
    }
}

/// Split a path into meaningful pieces. RootDir (`/`), `.` and repeated
/// separators are elided; `..` and ordinary names are kept, in order.
fn split_pieces(path: &Path) -> Vec<Piece> {
    path.components()
        .filter_map(|c| match c {
            Component::ParentDir => Some(Piece::Parent),
            Component::Normal(n) => Some(Piece::Name(n.to_os_string())),
            Component::RootDir | Component::CurDir | Component::Prefix(_) => None,
        })
        .collect()
}

/// How a containment failure is reported. The walk names the step the agent
/// asked for, so a rejection points at the path the agent wrote rather than at
/// a host path it never saw.
#[derive(Debug, Clone)]
enum ErrMode {
    /// Errors name virtual-path steps. Carries the original virtual input
    /// for the whole-input EscapesRoot reason.
    Virtual { input: String },
}

impl ErrMode {
    fn escape(&self, reached: &Path) -> ResolveError {
        match self {
            ErrMode::Virtual { input } => ResolveError::EscapesRoot {
                input: input.clone(),
                reached: reached.display().to_string(),
            },
        }
    }
    fn above_root(&self, step: &str, reached: &Path) -> ResolveError {
        ResolveError::TraversalAboveRoot {
            component: step.to_string(),
            reached: reached.display().to_string(),
        }
    }
    fn os_error(&self, step: &str, reason: String) -> ResolveError {
        ResolveError::OsError {
            path: step.to_string(),
            reason,
        }
    }
    fn symlink_loop(&self, step: &str) -> ResolveError {
        ResolveError::SymlinkLoop {
            link: step.to_string(),
        }
    }
    /// A symlink chain left the root at a hop and may or may not have returned.
    fn symlink_leaves(&self, step: &str, reached: &Path) -> ResolveError {
        ResolveError::SymlinkChainLeavesRoot {
            link: step.to_string(),
            target: reached.display().to_string(),
        }
    }
}

/// A hop whose target lands outside the root, named as the symlink that made
/// the hop — the form `blind_textual.rs` and `regression_textual_escape.rs`
/// already saw for `/nope/../café` and friends.
fn link_escapes(link_label: &str, reached: &Path) -> ResolveError {
    ResolveError::SymlinkEscapes {
        link: link_label.to_string(),
        target: reached.display().to_string(),
    }
}

/// Resolve an agent-supplied virtual path against the environment root.
///
/// - `root` is the real host path of the environment root (must exist).
/// - `virtual_path` is the agent-supplied path, absolute *within* the
///   environment (`/home/documents` means `<root>/home/documents`).
///
/// On success returns the real `PathBuf`, guaranteed to sit inside `root`
/// (or be `root` itself). On failure returns a `ResolveError` whose
/// `Display` names the offending component and rule.
///
/// Never creates, modifies, or deletes anything on the filesystem.
pub fn resolve(root: &Path, virtual_path: &str) -> Result<PathBuf, ResolveError> {
    // Empty: named before anything else.
    if virtual_path.is_empty() {
        return Err(ResolveError::EmptyPath);
    }
    // NUL: defense in depth, our own named reason before any OS call.
    if virtual_path.contains('\0') {
        return Err(ResolveError::NullByte {
            input: virtual_path.to_string(),
        });
    }
    // Relative: rejected outright, not silently rooted (Phase 1 §8.3.1).
    let path = Path::new(virtual_path);
    if !path.is_absolute() {
        return Err(ResolveError::RelativePath {
            input: virtual_path.to_string(),
        });
    }

    // Canonicalise the root once; every containment check compares against
    // the real root, not whatever spelling the caller used.
    let canon_root = std::fs::canonicalize(root).map_err(|e| ResolveError::OsError {
        path: root.display().to_string(),
        reason: format!("could not canonicalise environment root: {}", e),
    })?;

    // Split into pieces and apply the resolver's own length check (item
    // 10): per component, in bytes, before any filesystem call, so the
    // verdict is identical whether the name exists or not.
    let pieces = split_pieces(path);
    for piece in &pieces {
        if let Piece::Name(n) = piece {
            let bytes = n.as_encoded_bytes().len();
            if bytes > NAME_MAX_BYTES {
                return Err(ResolveError::NameTooLong {
                    component: n.to_string_lossy().into_owned(),
                    bytes,
                });
            }
        }
    }

    let mode = ErrMode::Virtual {
        input: virtual_path.to_string(),
    };
    walk(&canon_root, canon_root.clone(), &pieces, &mode, 0).map(|(p, _absent)| p)
}

/// The component walk shared by the virtual path and by symlink targets.
///
/// - `canon_root`: canonical environment root; containment is checked
///   against it at every step.
/// - `base`: the confirmed, canonical position the walk starts from. For
///   the virtual path this is the root itself; for a relative symlink
///   target it is the link's (canonical) directory; for an absolute target
///   it is `/`. `base` exists.
/// - `pieces`: the components to walk.
/// - `mode`: how failures are named (virtual steps vs. the symlink).
/// - `depth`: dangling-symlink recursion depth.
///
/// Returns the resolved location and whether any part of it was absent
/// (informational; the walk accepts absent locations inside the root).
/// Every returned location is inside `canon_root`.
///
/// The walk keeps two things, and the distinction between them is the whole
/// point:
///
/// - `current`, a **confirmed position**: canonical, existing, inside the
///   root. It is the last component of the path that genuinely exists.
/// - `pending`, the **absent remainder**: ordinary names, in order, that were
///   found not to exist directly below `current`. Nothing in `pending` is
///   ever stat-ed, because nothing in it exists.
///
/// A component that exists is therefore always canonicalised and
/// containment-checked, no matter what came before it in the path. An absent
/// component earlier on can no longer stop a later one from being followed —
/// which is exactly the escape this replaced: the first not-found component
/// used to switch the rest of the walk to text, so a symlink name reached
/// after it was never followed, and a path like `/nope/../NAME` returned a
/// location that canonicalised outside the root.
///
/// `..` is the one component that can move between the two. Under `pending`
/// it cancels the last absent name lexically; under a confirmed position it
/// is handed to the filesystem, so a file followed by `..` reports the
/// operating system's own ENOTDIR (ruling 2) rather than being normalised
/// away, and a `..` that would leave the root is refused here.
fn walk(
    canon_root: &Path,
    base: PathBuf,
    pieces: &[Piece],
    mode: &ErrMode,
    _depth: usize,
) -> Result<(PathBuf, bool), ResolveError> {
    let mut current = base;
    let mut pending: Vec<OsString> = Vec::new();
    // Names the current step for reasons: the virtual spelling the agent
    // wrote, accumulated so a rejection names the whole step (`/trap/escape`),
    // not the last component alone.
    let mut walked: Vec<String> = Vec::new();
    let mut absent_seen = false;

    for piece in pieces {
        let comp_os = piece.os();
        walked.push(comp_os.to_string_lossy().into_owned());
        let step_label = format!("/{}", walked.join("/"));

        if piece.is_parent() {
            if pending.pop().is_some() {
                // The `..` cancels the last component that does not exist,
                // lexically. No filesystem call: there was nothing there to
                // consult.
                absent_seen = true;
                continue;
            }
            // The `..` applies to the confirmed position, which exists. Ask
            // the filesystem. A file followed by `..` is ENOTDIR and the
            // operating system's own reason is reported (ruling 2); a `..`
            // out of the root is caught by the containment check below.
            current.push(comp_os);
            match std::fs::canonicalize(&current) {
                Ok(canonical) => {
                    if !canonical.starts_with(canon_root) {
                        // A `..` that moves the walk outside the root is a
                        // climb, whatever the position is: ancestors of the
                        // root are only ever traversed as part of *spelling* a
                        // target from the host root, never as a step of the
                        // agent's own path. Named as the climb, so a rejection
                        // names the step that climbed.
                        return Err(mode.above_root(&step_label, &canonical));
                    }
                    current = canonical;
                }
                Err(e) => {
                    let reason = e.to_string();
                    return Err(if reason.contains("Too many levels of symbolic links") {
                        mode.symlink_loop(&step_label)
                    } else {
                        mode.os_error(&step_label, reason)
                    });
                }
            }
            continue;
        }

        // An ordinary name below something that does not exist: it cannot
        // exist either (a path needs its parent), so it is text, and no
        // symlink can be reached through it.
        if !pending.is_empty() {
            absent_seen = true;
            pending.push(comp_os.to_os_string());
            continue;
        }

        // A name directly below the confirmed position. Split it into its
        // symlink hops and check containment at EVERY hop (ruling, 18 Sep
        // 2026), then place the result: a confirmed position if it exists, an
        // absent remainder if it does not.
        let (location, exists, any_absent) =
            walk_link_target(canon_root, &current, comp_os, &step_label, mode)?;
        if any_absent {
            absent_seen = true;
        }
        if exists {
            current = location;
        } else {
            pending = rel_components(canon_root, &location, mode)?;
            current = canon_root.to_path_buf();
        }
    }

    if !pending.is_empty() {
        let mut out = current;
        for n in &pending {
            out.push(n);
        }
        // Containment, comparing CANONICAL paths: canonicalise the longest
        // existing prefix of the result and compare that to the canonical
        // root. The remainder holds absent names only — every one of them was
        // found not to exist directly under a position that does exist — so
        // no symlink can be followed inside it and no `..` can appear in it.
        let mut probe = out.clone();
        let existing = loop {
            match std::fs::canonicalize(&probe) {
                Ok(c) => break Some(c),
                Err(_) => {
                    if !probe.pop() {
                        break None;
                    }
                }
            }
        };
        match existing {
            Some(canonical) if canonical.starts_with(canon_root) => {}
            _ => return Err(mode.escape(&out)),
        }
        return Ok((out, absent_seen));
    }

    Ok((current, absent_seen))
}

/// Components of `p` relative to the canonical root, as a stack. `p` must
/// be inside the root; if it is not, that is an escape under `mode`.
fn rel_components(
    canon_root: &Path,
    p: &Path,
    mode: &ErrMode,
) -> Result<Vec<OsString>, ResolveError> {
    match p.strip_prefix(canon_root) {
        Ok(rel) => Ok(rel
            .components()
            .filter_map(|c| match c {
                Component::Normal(n) => Some(n.to_os_string()),
                _ => None,
            })
            .collect()),
        Err(_) => Err(mode.escape(p)),
    }
}

/// True when `p` is inside the environment root, is the root itself, or is an
/// **ancestor** of it — the three positions the walk may pass THROUGH.
///
/// The ancestor case is why this exists. An absolute symlink target such as
/// `<root>/home/documents` is spelled with the host path of the root in it, so
/// walking that spelling passes through `/`, `/tmp`, the directory the root
/// sits in. Those are outside the root and they are the *way in* to it: they
/// are how the address is written, not a departure from the environment.
///
/// Passing through is not the same as landing. A hop that *ends* at an ancestor
/// — `<root>/a -> /home` — has left the root, and is refused (ruling, 18 Sep
/// 2026). A hop that ends at `/etc` or at a directory beside the root has left
/// it too. This predicate is used for positions the walk is traversing and for
/// the position a hop lands on; the landing check pairs it with a containment
/// test so an ancestor landing is caught.
fn on_the_way_in(canon_root: &Path, p: &Path) -> bool {
    p.starts_with(canon_root) || canon_root.starts_with(p)
}

/// The components of a raw symlink target, in order, with `.` and repeated
/// separators elided and `..` kept as a component (its meaning is positional:
/// it applies to where the walk actually is, not to the spelling).
fn target_pieces(target: &Path) -> Vec<OsString> {
    target
        .components()
        .filter_map(|c| match c {
            Component::RootDir | Component::CurDir => None,
            Component::ParentDir => Some(OsString::from("..")),
            Component::Normal(n) => Some(n.to_os_string()),
            // Windows drive prefixes; this resolver is Linux-only, and a
            // prefix cannot occur here anyway. Unreachable, not ignored.
            Component::Prefix(_) => None,
        })
        .collect()
}

/// The canonical directory a relative symlink target hangs off: the link's own
/// directory. The walk only ever reaches a link at a position it has already
/// canonicalised, so this is a confirmation, not a new traversal.
fn link_base(canon_root: &Path, link: &Path, mode: &ErrMode) -> Result<PathBuf, ResolveError> {
    let mut dir = link.to_path_buf();
    dir.pop();
    let canonical = std::fs::canonicalize(&dir).map_err(|_| {
        mode.os_error(
            &link.display().to_string(),
            "the symlink's own directory could not be canonicalised".to_string(),
        )
    })?;
    if !canonical.starts_with(canon_root) {
        return Err(mode.escape(&canonical));
    }
    Ok(canonical)
}

/// Resolve `pieces` from the canonical position `base`, following every symlink
/// **one hop at a time** and checking containment as the walk arrives at each
/// position (ruling, 18 Sep 2026).
///
/// This is the whole fix. The resolver used to hand a component to
/// `canonicalize`, which follows an entire chain — out of the root and back —
/// and reports only where it lands, so an out-and-back chain looked contained.
/// Here each hop is read with `read_link` and re-entered as its own walk, so
/// the position outside the root is reached as a *step* and refused there.
///
/// A hop that leaves the root fails immediately, naming the link: the link is
/// named by `origin_label` while the walk is still inside the link's own target,
/// and by the host spelling of the link once `crossed` is true — that is, once
/// the walk has left the root and is travelling through the outside, where a
/// virtual spelling no longer points anywhere real.
///
/// Returns the location and whether it exists. The location is inside the root
/// unless an error is returned.
/// Resolve `pieces` from the canonical position `base`, following every symlink
/// **one hop at a time** and checking containment as the walk arrives at each
/// position (ruling, 18 Sep 2026).
///
/// This is the fix. The resolver used to hand a component to `canonicalize`,
/// which follows an entire chain — out of the root and back — and reports only
/// where it lands, so an out-and-back chain looked contained. Here each hop is
/// read with `read_link` and re-entered as its own walk, so the position
/// outside the root is reached as a *step*, and the departure is recorded when
/// it happens rather than judged from the landing.
///
/// Two different things are checked, and the ruling needs both:
///
/// - **Traversal** may pass through any ancestor of the root (and the root, and
///   anything inside it). That is not a departure: an absolute target that
///   points inside is spelled `<root>/home/documents`, and `<root>` is written
///   with the host paths above it, so the walk must travel `/`, `/tmp`, the
///   root's parent to arrive. See `on_the_way_in` / `traversable`.
/// - **Landing** may not. A hop that ends at an ancestor — `<root>/a -> /home`
///   — or at a directory beside the root, or at `/etc`, has left the
///   environment. The departure is recorded in `seen_out` at the moment it
///   happens, so a chain that leaves and comes back is refused even though its
///   final location is inside the root.
///
/// `..` is applied to a confirmed position by the filesystem, so a file
/// followed by `..` reports the operating system's own ENOTDIR (ruling 2)
/// rather than being normalised away. A `..` that moves the walk outside the
/// root is a climb and is named as one.
///
/// Returns the location and whether it exists. Whether the chain left is
/// reported through `seen_out`, not through the return value.
#[allow(clippy::too_many_arguments)]
fn resolve_from(
    canon_root: &Path,
    base: &Path,
    pieces: &[OsString],
    origin_label: &str,
    mode: &ErrMode,
    hops: &mut usize,
    seen_out: &mut Option<(String, PathBuf)>,
) -> Result<(PathBuf, bool), ResolveError> {
    let mut current = base.to_path_buf();
    let mut pending: Vec<OsString> = Vec::new();

    for name in pieces {
        if name == OsStr::new("..") {
            if pending.pop().is_some() {
                // Cancels the last absent name, lexically: nothing was there.
                continue;
            }
            // Applies to a confirmed position, so ask the filesystem. A file
            // followed by `..` reports the operating system's own ENOTDIR
            // (ruling 2), which is why this is not a textual pop.
            current.push("..");
            match std::fs::canonicalize(&current) {
                Ok(c) => {
                    if !canon_root.starts_with(&c) {
                        // A climb that leaves the root — the ancestors of the
                        // root are only ever traversed while spelling a target
                        // from the host root, never as a `..` of the walk.
                        return Err(mode.above_root(origin_label, &c));
                    }
                    current = c;
                }
                Err(e) => {
                    current.pop();
                    if e.kind() == std::io::ErrorKind::NotFound {
                        return Err(mode.above_root(origin_label, canon_root));
                    }
                    return Err(mode.os_error(origin_label, e.to_string()));
                }
            }
            continue;
        }
        if name == OsStr::new(".") {
            continue;
        }
        if !pending.is_empty() {
            // Below something absent: it cannot exist either, so it is text.
            pending.push(name.clone());
            continue;
        }

        current.push(name);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                *hops += 1;
                if *hops > MAX_LINK_HOPS {
                    return Err(mode.symlink_loop(origin_label));
                }
                let target = std::fs::read_link(&current).map_err(|e| {
                    mode.os_error(
                        &current.display().to_string(),
                        format!("could not read symlink: {}", e),
                    )
                })?;
                // Name the step the agent wrote while the walk is still inside
                // the root; once it has left, only a host spelling means
                // anything.
                let label = if seen_out.is_some() || !current.starts_with(canon_root) {
                    current.display().to_string()
                } else {
                    origin_label.to_string()
                };
                let (hop_base, hop_pieces) = if target.is_absolute() {
                    (PathBuf::from("/"), target_pieces(&target))
                } else {
                    (
                        link_base(canon_root, &current, mode)?,
                        target_pieces(&target),
                    )
                };
                // The hop's target is walked from its own base. A position
                // outside the root is reached here as a step, so the departure
                // is seen rather than inferred from where the chain lands.
                let (location, exists) = resolve_from(
                    canon_root,
                    &hop_base,
                    &hop_pieces,
                    &label,
                    mode,
                    hops,
                    seen_out,
                )?;
                if seen_out.is_none() && !on_the_way_in(canon_root, &location) {
                    // The link's target lands outside: it has left the root,
                    // whatever happens next. Recorded once, so the link that
                    // made the hop is the one named, together with the position
                    // it reached.
                    *seen_out = Some((label, location.clone()));
                }
                if exists {
                    current = location;
                } else if location.starts_with(canon_root) {
                    // The target is absent, so every component beneath it is
                    // too. Move to the root and carry the location as the
                    // absent remainder, exactly as the main walk does:
                    // `current` and `pending` must not both name it, or a later
                    // `..` cancels the remainder while the position still holds
                    // it and the walk steps back onto a path that is not there.
                    pending = rel_components(canon_root, &location, mode)?;
                    current = canon_root.to_path_buf();
                } else {
                    // Absent AND outside the root. Left as the position so the
                    // caller names it as the escape it is; `rel_components`
                    // would raise a whole-input escape here and lose the fact
                    // that a symlink's target is what left.
                    current = location;
                }
            }
            Ok(_) => {
                // A real entry that is not a link. `base` is canonical and
                // every position is canonicalised as it is reached, so this is
                // already its canonical form.
                if seen_out.is_none() && !on_the_way_in(canon_root, &current) {
                    // The walk has arrived somewhere that is neither inside the
                    // root nor on the way in to it — a directory beside the
                    // root, `/etc`, the host root. That is the chain leaving,
                    // and the position reached is the evidence for it.
                    *seen_out = Some((origin_label.to_string(), current.clone()));
                }
            }
            Err(_) => {
                // Either genuinely absent, or the operating system refused the
                // path. A name below an existing FILE is ENOTDIR, and that is
                // reported as the OS's own reason (ruling 2) rather than being
                // read as an absent name — otherwise `/notes.txt/newfile` would
                // be accepted as an absent path inside the root.
                match std::fs::canonicalize(&current) {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        // Absent, and not a link. It joins the remainder.
                        pending.push(name.clone());
                        current.pop();
                    }
                    Err(e) => return Err(mode.os_error(origin_label, e.to_string())),
                    Ok(_) => {
                        // Stat failed but canonicalise succeeded: race, not a
                        // steady state. Treat the entry as absent for this walk.
                        pending.push(name.clone());
                        current.pop();
                    }
                }
            }
        }
    }

    if pending.is_empty() {
        return Ok((current, true));
    }
    let mut out = current;
    for n in &pending {
        out.push(n);
    }
    Ok((out, false))
}

/// Walk one virtual component to its resolved location, following the symlink
/// hops inside it one at a time so containment is checked at every hop
/// (ruling, 18 Sep 2026).
///
/// The chain has left the root if any hop *landed* outside it, even when the
/// walk came back in; that is the ruling, and it is why `seen_out` is consulted
/// before the final location is trusted.
///
/// Returns `(location, exists, any_absent)`: where the component resolved,
/// whether that location exists (as opposed to being an absent remainder), and
/// whether anything along the way was absent.
fn walk_link_target(
    canon_root: &Path,
    dir: &Path,
    name: &OsStr,
    step_label: &str,
    mode: &ErrMode,
) -> Result<(PathBuf, bool, bool), ResolveError> {
    let mut hops = 0usize;
    let mut seen_out: Option<(String, PathBuf)> = None;
    let (location, exists) = resolve_from(
        canon_root,
        dir,
        &[name.to_os_string()],
        step_label,
        mode,
        &mut hops,
        &mut seen_out,
    )?;

    if !location.starts_with(canon_root) {
        // The chain ends outside the root: at an ancestor of the root (a link
        // whose target is `/home` when the root sits below it — the chain left
        // without ever traversing a position that is not an ancestor), or
        // somewhere further out.
        let label = match seen_out {
            Some((label, _)) => label,
            None => step_label.to_string(),
        };
        return Err(link_escapes(&label, &location));
    }
    if let Some((label, left_at)) = seen_out {
        // It left and came back: refused anyway (ruling, 18 Sep 2026). The
        // position named is where the chain left, not where it ended.
        return Err(mode.symlink_leaves(&label, &left_at));
    }
    Ok((location, exists, !exists))
}
