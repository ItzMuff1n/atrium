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
//!   chains and loops.
//! - Lexical attacks (NUL bytes, relative paths, empty paths) are rejected
//!   with per-case reasons before any filesystem call.
//! - Phase 1b: a path whose target does not exist is ACCEPTED as long as
//!   its resolved location sits inside the root. The first component that
//!   canonicalises as not-found switches the rest of the walk to textual
//!   mode: nothing further is stat-ed, the remainder is normalised
//!   component by component (`//` collapses, `.` elided, `x/..` removes
//!   `x`), and a `..` that pops above the root is rejected naming the step.
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

/// Ceiling for following dangling symlinks (links whose target is absent,
/// so `canonicalize` cannot follow them for us). Linux uses MAXSYMLINKS
/// 40; existing links are followed by the filesystem and never touch this
/// counter.
const MAX_DANGLING_SYMLINKS: usize = 40;

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

/// How a containment failure is reported. The main virtual-path walk names
/// the offending virtual step; a walk performed on behalf of a symlink
/// target names the symlink instead (the agent asked for the link, and it
/// is the link's target that is outside).
#[derive(Debug, Clone)]
enum ErrMode {
    /// Errors name virtual-path steps. Carries the original virtual input
    /// for the whole-input EscapesRoot reason.
    Virtual { input: String },
    /// Errors name the symlink whose target is being walked.
    Symlink { link_virtual: String },
}

impl ErrMode {
    fn escape(&self, reached: &Path) -> ResolveError {
        match self {
            ErrMode::Virtual { input } => ResolveError::EscapesRoot {
                input: input.clone(),
                reached: reached.display().to_string(),
            },
            ErrMode::Symlink { link_virtual } => ResolveError::SymlinkEscapes {
                link: link_virtual.clone(),
                target: reached.display().to_string(),
            },
        }
    }
    fn above_root(&self, step: &str, reached: &Path) -> ResolveError {
        match self {
            ErrMode::Virtual { .. } => ResolveError::TraversalAboveRoot {
                component: step.to_string(),
                reached: reached.display().to_string(),
            },
            ErrMode::Symlink { link_virtual } => ResolveError::SymlinkEscapes {
                link: link_virtual.clone(),
                target: reached.display().to_string(),
            },
        }
    }
    fn os_error(&self, step: &str, reason: String) -> ResolveError {
        let path = match self {
            ErrMode::Virtual { .. } => step.to_string(),
            ErrMode::Symlink { link_virtual } => link_virtual.clone(),
        };
        ResolveError::OsError { path, reason }
    }
    fn symlink_loop(&self, step: &str) -> ResolveError {
        let link = match self {
            ErrMode::Virtual { .. } => step.to_string(),
            ErrMode::Symlink { link_virtual } => link_virtual.clone(),
        };
        ResolveError::SymlinkLoop { link }
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

/// The two-mode component walk shared by the virtual path and by symlink
/// targets.
///
/// - `canon_root`: canonical environment root; containment is checked
///   against it at every step.
/// - `base`: the confirmed, canonical position the walk starts from. For
///   the virtual path this is the root itself; for a relative symlink
///   target it is the link's (canonical) directory; for an absolute target
///   it is `/`.
/// - `pieces`: the components to walk.
/// - `mode`: how failures are named (virtual steps vs. the symlink).
/// - `depth`: dangling-symlink recursion depth.
///
/// Returns the resolved location and whether any part of it was absent
/// (informational; the walk accepts absent locations inside the root).
/// Every returned location is inside `canon_root`.
fn walk(
    canon_root: &Path,
    base: PathBuf,
    pieces: &[Piece],
    mode: &ErrMode,
    depth: usize,
) -> Result<(PathBuf, bool), ResolveError> {
    // `current` is the position as spelled so far (pre-canonical).
    let mut current = base;
    // Textual mode: once Some, the walk has left the filesystem. The stack
    // is the position as components relative to the canonical root, so an
    // empty stack is the root and a `..` pop on it is a climb above it.
    let mut textual: Option<Vec<OsString>> = None;
    // Names the current step for reasons: virtual spellings for the main
    // walk, host spellings for target walks (used only inside reasons).
    let mut walked: Vec<String> = Vec::new();
    let mut absent_seen = false;

    for piece in pieces {
        let comp_os = piece.os();
        walked.push(comp_os.to_string_lossy().into_owned());
        let step_label = format!("/{}", walked.join("/"));

        if let Some(stack) = textual.as_mut() {
            // Textual mode: nothing is stat-ed. Normalise lexically.
            absent_seen = true;
            if piece.is_parent() {
                if stack.pop().is_none() {
                    return Err(mode.above_root(&step_label, canon_root));
                }
            } else {
                stack.push(comp_os.to_os_string());
            }
            continue;
        }

        // Filesystem mode (Phase 1's walk): append, canonicalise (which
        // follows symlinks and resolves `..` on disk), verify containment.
        current.push(comp_os);
        match std::fs::canonicalize(&current) {
            Ok(canonical) => {
                if !canonical.starts_with(canon_root) {
                    return Err(classify_escape(&current, piece, &walked, &canonical, mode));
                }
                current = canonical;
            }
            Err(e) => match e.kind() {
                std::io::ErrorKind::NotFound => {
                    absent_seen = true;
                    if piece.is_parent() {
                        // `<prefix>/..` failed NotFound: the prefix must be
                        // a dangling symlink (an ordinary existing directory
                        // always has a `..`). Resolve the link's target,
                        // then apply the `..` to it textually. The target
                        // decides, as for any symlink (requirement 6).
                        // current is `<link>/..`; the link is current
                        // without the trailing ".." pushed this iteration.
                        let mut link = current.clone();
                        link.pop();
                        let link_mode = ErrMode::Symlink {
                            link_virtual: format!("/{}", walked[..walked.len() - 1].join("/")),
                        };
                        let target = follow_dangling(canon_root, &link, &link_mode, depth)?;
                        // Apply this `..` to the target lexically, then
                        // continue textually from there.
                        let mut stack = rel_components(canon_root, &target, mode)?;
                        if stack.pop().is_none() {
                            return Err(mode.above_root(&step_label, canon_root));
                        }
                        current = target;
                        textual = Some(stack);
                        continue;
                    }
                    let is_symlink = std::fs::symlink_metadata(&current)
                        .map(|m| m.file_type().is_symlink())
                        .unwrap_or(false);
                    if is_symlink {
                        // Dangling symlink: the link exists, its target does
                        // not. The target is what matters (requirement 6):
                        // outside the root rejects whether the target exists
                        // or not; inside the root the resolved target
                        // location becomes the new position.
                        let link_mode = ErrMode::Symlink {
                            link_virtual: step_label.clone(),
                        };
                        let resolved = follow_dangling(canon_root, &current, &link_mode, depth)?;
                        let stack = rel_components(canon_root, &resolved, mode)?;
                        current = resolved;
                        textual = Some(stack);
                        continue;
                    }
                    // Ordinary absent name inside the root: switch to
                    // textual mode. `current` (position including this
                    // component) is inside the root because the previous
                    // canonical position was; derive the stack from it.
                    let stack = rel_components(canon_root, &current, mode)?;
                    textual = Some(stack);
                }
                _ => {
                    let reason = e.to_string();
                    return Err(if reason.contains("Too many levels of symbolic links") {
                        mode.symlink_loop(&step_label)
                    } else {
                        mode.os_error(&step_label, reason)
                    });
                }
            },
        }
    }

    if let Some(stack) = textual {
        let mut out = canon_root.to_path_buf();
        for n in &stack {
            out.push(n);
        }
        // Containment is guaranteed by construction (pops are bounded at
        // the root); assert it anyway, boringly.
        if !out.starts_with(canon_root) {
            return Err(mode.escape(&out));
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

/// Phase 1's per-case rejection selection for a step that canonicalised
/// outside the root. Unchanged from Phase 1, except escapes are named per
/// `mode` when walking a symlink target.
fn classify_escape(
    current: &Path,
    piece: &Piece,
    walked: &[String],
    canonical: &Path,
    mode: &ErrMode,
) -> ResolveError {
    let step_label = format!("/{}", walked.join("/"));
    let prev_is_symlink = {
        let mut prev = current.to_path_buf();
        prev.pop();
        std::fs::symlink_metadata(&prev)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
    };
    let this_is_symlink = std::fs::symlink_metadata(current)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);

    if piece.is_parent() && prev_is_symlink {
        // The directory we `..`'d out of was itself an escaping symlink
        // (e.g. /link-to-etc/../home).
        let mut link = current.to_path_buf();
        link.pop();
        ResolveError::SymlinkEscapes {
            link: display_virtual_prev(walked),
            target: std::fs::canonicalize(&link)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "<unresolvable>".to_string()),
        }
    } else if piece.is_parent() {
        mode.above_root(&step_label, canonical)
    } else if this_is_symlink {
        ResolveError::SymlinkEscapes {
            link: step_label,
            target: canonical.display().to_string(),
        }
    } else {
        mode.escape(canonical)
    }
}

/// Resolve the target of a dangling symlink (the link exists, its target
/// is absent, so `canonicalize` could not follow it). The result is the
/// location the target would occupy: its canonical longest-existing prefix
/// plus the lexically-normalised absent remainder. It is checked against
/// the root; a target outside rejects whether it exists or not.
///
/// The prefix is canonicalised whole — never walked step by step from the
/// host root — so the check mirrors what Phase 1 does for a followed link:
/// containment is decided at the point the target *lands*, and ancestors
/// of the environment root (which are outside it by definition) are never
/// mistaken for an escape.
fn follow_dangling(
    canon_root: &Path,
    link: &Path,
    mode: &ErrMode,
    depth: usize,
) -> Result<PathBuf, ResolveError> {
    if depth >= MAX_DANGLING_SYMLINKS {
        return Err(mode.symlink_loop(&link.display().to_string()));
    }
    let target = std::fs::read_link(link).map_err(|e| {
        mode.os_error(
            &link.display().to_string(),
            format!("could not read symlink: {}", e),
        )
    })?;

    // Make the target absolute: absolute targets stand as written,
    // relative targets hang off the link's directory. The walk only ever
    // follows links whose own position was canonical, so that directory
    // is already canonical.
    let mut full = if target.is_absolute() {
        target
    } else {
        let mut dir = link.to_path_buf();
        dir.pop();
        dir.join(target)
    };

    // Pop trailing components until canonicalize succeeds. The components
    // popped are the absent remainder (plus any `..`/`.` segments mingled
    // with it), kept in order.
    let mut remainder: Vec<OsString> = Vec::new();
    let canonical = loop {
        match std::fs::canonicalize(&full) {
            Ok(p) => break p,
            Err(e) => match e.kind() {
                std::io::ErrorKind::NotFound => {
                    // If the name just dropped is itself a symlink, it is
                    // still a symlink: recurse into its target (with the
                    // depth ceiling) and then append the remainder we had
                    // already peeled off.
                    let is_link = std::fs::symlink_metadata(&full)
                        .map(|m| m.file_type().is_symlink())
                        .unwrap_or(false);
                    if is_link {
                        let resolved = follow_dangling(canon_root, &full, mode, depth + 1)?;
                        let mut out = resolved;
                        // The peeled remainder re-applies lexically. `..`
                        // pops are textually applied below by the same
                        // normalisation used everywhere else.
                        let mut stack = rel_components(canon_root, &out, mode)?;
                        for name in remainder.iter().rev() {
                            if name == OsStr::new("..") {
                                if stack.pop().is_none() {
                                    return Err(
                                        mode.above_root(&full.display().to_string(), canon_root)
                                    );
                                }
                            } else if name == OsStr::new(".") {
                                // elided
                            } else {
                                stack.push(name.clone());
                            }
                        }
                        out = canon_root.to_path_buf();
                        for n in &stack {
                            out.push(n);
                        }
                        if !out.starts_with(canon_root) {
                            return Err(mode.escape(&out));
                        }
                        return Ok(out);
                    }
                    match full.file_name() {
                        Some(name) => {
                            remainder.push(name.to_os_string());
                            full.pop();
                        }
                        None => {
                            // Reached "/" without canonicalising: nothing
                            // exists at all. Report honestly.
                            return Err(mode.os_error(
                                &link.display().to_string(),
                                "no component of the symlink target exists".to_string(),
                            ));
                        }
                    }
                }
                _ => {
                    let reason = e.to_string();
                    return Err(if reason.contains("Too many levels of symbolic links") {
                        mode.symlink_loop(&full.display().to_string())
                    } else {
                        mode.os_error(&full.display().to_string(), reason)
                    });
                }
            },
        }
    };

    // The existing prefix must land inside the root — this is the exact
    // containment check a canonicalised link gets in Phase 1.
    if !canonical.starts_with(canon_root) {
        return Err(mode.escape(&canonical));
    }

    // The absent remainder is normalised lexically on top of the prefix:
    // `.` elided, `x/..` removes `x`, a `..` popping past the canonical
    // root boundary is an escape.
    let mut stack = rel_components(canon_root, &canonical, mode)?;
    for name in remainder.iter().rev() {
        if name == OsStr::new("..") {
            if stack.pop().is_none() {
                return Err(mode.above_root(&link.display().to_string(), canon_root));
            }
        } else if name == OsStr::new(".") {
            // elided
        } else {
            stack.push(name.clone());
        }
    }
    let mut out = canon_root.to_path_buf();
    for n in &stack {
        out.push(n);
    }
    if !out.starts_with(canon_root) {
        return Err(mode.escape(&out));
    }
    Ok(out)
}

/// The virtual spelling of all-but-the-last walked component, for naming a
/// symlink in a `..` rejection.
fn display_virtual_prev(walked: &[String]) -> String {
    if walked.len() <= 1 {
        return "/".to_string();
    }
    format!("/{}", walked[..walked.len() - 1].join("/"))
}

/// Temporary probe: deliberately mis-formatted, so `cargo fmt --check` fails.
/// The argument list is broken across lines where rustfmt requires it on one line.
fn probe_red_ci(
    a: i32,
    b: i32
) -> i32 {
    a + b
}
