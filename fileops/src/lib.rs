//! atrium-fileops — Phase 2a: the six file operations (`BUILD-PLAN.md` §2a).
//!
//! Create a directory, write a file, read a file, list a directory, move,
//! delete. Every path argument is a `VirtualPath` (the path as the agent
//! wrote it); each operation resolves it through `atrium_resolver::resolve`
//! — the only function allowed to turn a virtual path into a real one
//! (`AGENT-RULES.md` §6, `DESIGN.md` §3.2) — and only then acts on the
//! resulting `RealPath`. A real path cannot be constructed any other way:
//! `RealPath`'s constructor is private to this crate, so forgetting to
//! resolve is a compile error, not a bug that ships.
//!
//! Behaviour required by attack-list-2a.md (all of it, tested):
//! - A path that fails to resolve is refused; nothing at all happens. No
//!   partial effects (§9.3.2).
//! - `write_file` does NOT create missing parents; it refuses naming the
//!   missing parent. `create_dir` creates parents and is not an error on an
//!   existing directory (§F.2 vs §F.3, §F.6).
//! - `delete_path` takes an explicit `recursive` flag. Without it: refuses
//!   non-empty directories (naming the dir) and refuses the root. With it:
//!   removes the tree. A recursive walk NEVER follows a symlink — links are
//!   removed *as links* (§E.1, §E.2). This is the one requirement whose
//!   failure mode is destroying the user's real files.
//! - The environment root itself is never deleted or moved, whatever flags
//!   are given (§E.6, §D.5).
//! - `list_dir` returns entries sorted by name, each labelled file,
//!   directory or symlink (§G.4).
//! - Every failure names the specific thing that was wrong — the resolver's
//!   own reasons pass through intact, never flattened to "denied".
//!
//! Known, deliberately NOT fixed: the symlink-object limitation
//! (attack-list-2a.md §K). `resolve()` follows a symlink's final component,
//! so an operation aimed at an inside-pointing link acts on its target and
//! not on the link. Fixing it requires a change to the signed-off resolver,
//! which this phase is forbidden to make. Recorded, reproduced, deferred.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use atrium_resolver::{resolve, ResolveError};

/// A path as the agent wrote it. Always starts with `/`.
///
/// Constructing one guarantees nothing about what the path points at; it
/// only fixes the spelling class. Containment is still decided by
/// `resolve()` inside each operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualPath(String);

impl VirtualPath {
    /// Build a virtual path from the agent's spelling. Relative paths are
    /// refused here (and again inside `resolve()`, which remains the only
    /// containment oracle per `AGENT-RULES.md` §6).
    pub fn new(path: &str) -> Result<VirtualPath, FileOpError> {
        if !path.starts_with('/') {
            return Err(FileOpError::Rejected(ResolveError::RelativePath {
                input: path.to_string(),
            }));
        }
        Ok(VirtualPath(path.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The parent of a virtual path, lexically ("/a/b" -> "/a", "/a" ->
    /// "/"). Used only to NAME the missing parent in a refusal message —
    /// never to construct a real path (that is `resolve()`'s job, §9.3.12).
    fn parent_lexical(&self) -> String {
        let trimmed = self.0.trim_end_matches('/');
        match trimmed.rfind('/') {
            None | Some(0) => "/".to_string(),
            Some(i) => trimmed[..i].to_string(),
        }
    }
}

impl fmt::Display for VirtualPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A path that has been through the resolver and is known to be inside the
/// environment root. The only type the operations act on. Its constructor
/// is private: nothing outside this crate can make one, and the only maker
/// inside this crate is `resolve()`.
#[derive(Debug)]
pub struct RealPath(PathBuf);

impl RealPath {
    fn as_path(&self) -> &Path {
        &self.0
    }

    /// The single doorway from virtual to real. Called at the top of every
    /// operation, for every path argument, before any disk access
    /// (§9.3.1). A resolver rejection passes through with its reason
    /// intact (§9.6).
    fn resolve(root: &Path, path: &VirtualPath) -> Result<RealPath, FileOpError> {
        match resolve(root, path.as_str()) {
            Ok(p) => Ok(RealPath(p)),
            Err(e) => Err(FileOpError::Rejected(e)),
        }
    }
}

/// One entry of a directory listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub kind: EntryKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
}

impl EntryKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            EntryKind::File => "file",
            EntryKind::Directory => "dir",
            EntryKind::Symlink => "link",
        }
    }
}

/// The one error type. Carries enough to name the specific failure: which
/// virtual path, which missing parent, which rule. Resolver rejections are
/// wrapped, never flattened. No variant says "denied" without a reason.
#[derive(Debug)]
pub enum FileOpError {
    /// The resolver rejected the path; its own reason passes through.
    Rejected(ResolveError),
    /// `write_file` / `move_path` destination: the parent directory does
    /// not exist. Creating parents is `create_dir`'s job (§F.2).
    ParentMissing { path: String, parent: String },
    /// The path does not exist inside the environment (operation-level
    /// absence — e.g. reading or deleting an absent name).
    NotFound { path: String },
    /// A file was required but the path is a directory.
    IsADirectory { path: String },
    /// A directory was required but the path is not one.
    NotADirectory { path: String },
    /// `create_dir` asked to create a directory where a file exists.
    ExistingFile { path: String },
    /// Non-recursive delete of a directory that still has contents.
    DirectoryNotEmpty { path: String, entries: usize },
    /// The environment root itself, which is never deleted or moved.
    RootProtected { operation: &'static str },
    /// The operating system refused the operation. The reason names the
    /// virtual path and the OS's own words; never a bare code.
    Io { path: String, reason: String },
}

impl fmt::Display for FileOpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FileOpError::Rejected(e) => {
                // The resolver's own `Display` text names the offending step,
                // but for two variants it names where that step *reached* —
                // a real host path. Passing it through verbatim would disclose
                // the sandbox's location on disk (DESIGN.md §3.1; attack-list-2a.md
                // §H.2/H.3/H.4). So those two variants are re-worded here, from
                // the resolver's STRUCTURED fields, to name the VIRTUAL path and
                // drop the real one. Every other variant is passed through intact
                // (§9.6): its text is already virtual-path-only.
                //
                // This is why the resolver must never grow a reason that names a
                // real path in a *new* variant without this match being updated.
                match e {
                    ResolveError::TraversalAboveRoot { component, .. } => write!(
                        f,
                        "Rejected: the `..` step `{}` climbed above the environment root (that step left the environment; where it reached on disk is not disclosed)",
                        component
                    ),
                    ResolveError::EscapesRoot { input, .. } => write!(
                        f,
                        "Rejected: `{}` escapes the environment root (a step left the environment; where it reached on disk is not disclosed)",
                        input
                    ),
                    ResolveError::SymlinkEscapes { link, .. } => write!(
                        f,
                        "Rejected: symlink `{}` points outside the environment root (its target is outside the environment, and its location is not disclosed)",
                        link
                    ),
                    other => write!(f, "{}", other),
                }
            }
            FileOpError::ParentMissing { path, parent } => write!(
                f,
                "refused: parent directory `{}` of `{}` does not exist; write-file does not create parent directories — use create-dir on `{}` first",
                parent, path, parent
            ),
            FileOpError::NotFound { path } => write!(
                f,
                "refused: `{}` does not exist inside the environment",
                path
            ),
            FileOpError::IsADirectory { path } => write!(
                f,
                "refused: `{}` is a directory, and a file was required",
                path
            ),
            FileOpError::NotADirectory { path } => write!(
                f,
                "refused: `{}` is not a directory, and a directory was required",
                path
            ),
            FileOpError::ExistingFile { path } => write!(
                f,
                "refused: `{}` is an existing file; create-dir cannot create a directory over a file",
                path
            ),
            FileOpError::DirectoryNotEmpty { path, entries } => write!(
                f,
                "refused: directory `{}` is not empty (it holds {} entr{}); a non-recursive delete only removes an empty directory — reconsider, or pass --recursive",
                path,
                entries,
                if *entries == 1 { "y" } else { "ies" }
            ),
            FileOpError::RootProtected { operation } => write!(
                f,
                "refused: the environment root itself (`/`) can never be {} — that is the sandbox, not a path inside it",
                operation
            ),
            FileOpError::Io { path, reason } => write!(
                f,
                "refused: the operating system refused the operation on `{}` ({})",
                path, reason
            ),
        }
    }
}

impl std::error::Error for FileOpError {}

/// The canonical environment root, for the root-protection check. Names
/// `<root>` rather than a host path in its error (`DESIGN.md` §3.1).
fn canonical_root(root: &Path) -> Result<PathBuf, FileOpError> {
    fs::canonicalize(root).map_err(|e| FileOpError::Io {
        path: "<environment root>".to_string(),
        reason: format!("could not canonicalise the environment root: {}", e),
    })
}

fn io_err(path: &VirtualPath, e: std::io::Error) -> FileOpError {
    FileOpError::Io {
        path: path.as_str().to_string(),
        reason: e.to_string(),
    }
}

// ---------------------------------------------------------------------------
// The six operations. Every one resolves every path argument BEFORE any
// disk access. Nothing panics. Nothing unwraps.
// ---------------------------------------------------------------------------

/// `create_dir(virtual_path)` — the directory and any missing parents.
/// Not an error if the directory already exists (§F.6); an error if a
/// file sits where the directory should be (§F.5).
pub fn create_dir(root: &Path, path: &VirtualPath) -> Result<(), FileOpError> {
    let real = RealPath::resolve(root, path)?;
    let p = real.as_path();
    match fs::metadata(p) {
        Ok(m) if m.is_dir() => Ok(()),
        Ok(_) => Err(FileOpError::ExistingFile {
            path: path.as_str().to_string(),
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(p).map_err(|e| io_err(path, e))
        }
        Err(e) => Err(io_err(path, e)),
    }
}

/// `write_file(virtual_path, bytes)` — overwrites an existing file;
/// refuses a missing parent (naming it, §F.2) and a directory target
/// (§F.4). Never creates parent directories.
pub fn write_file(root: &Path, path: &VirtualPath, bytes: &[u8]) -> Result<(), FileOpError> {
    let real = RealPath::resolve(root, path)?;
    let p = real.as_path();
    if p.is_dir() {
        return Err(FileOpError::IsADirectory {
            path: path.as_str().to_string(),
        });
    }
    // Parent must exist and be a directory. This is an existence check on
    // the resolved location only — containment was decided by resolve().
    let parent = p
        .parent()
        .expect("a resolved path always has the environment root above it");
    match fs::symlink_metadata(parent) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => {
            return Err(FileOpError::NotADirectory {
                path: path.parent_lexical(),
            })
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(FileOpError::ParentMissing {
                path: path.as_str().to_string(),
                parent: path.parent_lexical(),
            })
        }
        Err(e) => return Err(io_err(path, e)),
    }
    fs::write(p, bytes).map_err(|e| io_err(path, e))
}

/// `read_file(virtual_path)` -> the file's bytes. Refuses a directory
/// (§G.1) and an absent name (§G.3), each with its own reason.
pub fn read_file(root: &Path, path: &VirtualPath) -> Result<Vec<u8>, FileOpError> {
    let real = RealPath::resolve(root, path)?;
    let p = real.as_path();
    match fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() => Err(FileOpError::IsADirectory {
            path: path.as_str().to_string(),
        }),
        Ok(_) => fs::read(p).map_err(|e| io_err(path, e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(FileOpError::NotFound {
            path: path.as_str().to_string(),
        }),
        Err(e) => Err(io_err(path, e)),
    }
}

/// `list_dir(virtual_path)` -> entries sorted by name, each labelled
/// file / dir / link (§G.4). Refuses a non-directory (§G.2) and an
/// absent name, each with its own reason.
pub fn list_dir(root: &Path, path: &VirtualPath) -> Result<Vec<DirEntry>, FileOpError> {
    let real = RealPath::resolve(root, path)?;
    let p = real.as_path();
    match fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => {
            return Err(FileOpError::NotADirectory {
                path: path.as_str().to_string(),
            })
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(FileOpError::NotFound {
                path: path.as_str().to_string(),
            })
        }
        Err(e) => return Err(io_err(path, e)),
    }
    let mut out = Vec::new();
    let rd = fs::read_dir(p).map_err(|e| io_err(path, e))?;
    for entry in rd {
        let entry = entry.map_err(|e| io_err(path, e))?;
        // symlink_metadata: a symlink is listed as a link even when its
        // target exists — the listing reports what is THERE.
        let kind = match fs::symlink_metadata(entry.path()) {
            Ok(m) if m.file_type().is_symlink() => EntryKind::Symlink,
            Ok(m) if m.is_dir() => EntryKind::Directory,
            Ok(_) => EntryKind::File,
            Err(e) => return Err(io_err(path, e)),
        };
        out.push(DirEntry {
            name: entry.file_name().to_string_lossy().into_owned(),
            kind,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// `move_path(from, to)` — BOTH arguments are virtual paths, BOTH are
/// resolved before anything happens (§D.1 exists because checking one is
/// the natural mistake). The root is never the source (§D.5).
pub fn move_path(root: &Path, from: &VirtualPath, to: &VirtualPath) -> Result<(), FileOpError> {
    let real_from = RealPath::resolve(root, from)?;
    let real_to = RealPath::resolve(root, to)?;
    let canon_root = canonical_root(root)?;
    if real_from.as_path() == canon_root {
        return Err(FileOpError::RootProtected { operation: "moved" });
    }
    if real_to.as_path() == canon_root {
        return Err(FileOpError::RootProtected {
            operation: "moved onto",
        });
    }
    // Source must exist (the resolver accepts absent-inside paths, which is
    // right for resolution but meaningless for a move source).
    if let Err(e) = fs::symlink_metadata(real_from.as_path()) {
        return Err(if e.kind() == std::io::ErrorKind::NotFound {
            FileOpError::NotFound {
                path: from.as_str().to_string(),
            }
        } else {
            io_err(from, e)
        });
    }
    // Destination parent must exist and be a directory; same rule as write.
    let parent = real_to
        .as_path()
        .parent()
        .expect("a resolved path always has the environment root above it");
    match fs::symlink_metadata(parent) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => {
            return Err(FileOpError::NotADirectory {
                path: to.parent_lexical(),
            })
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(FileOpError::ParentMissing {
                path: to.as_str().to_string(),
                parent: to.parent_lexical(),
            })
        }
        Err(e) => return Err(io_err(to, e)),
    }
    fs::rename(real_from.as_path(), real_to.as_path()).map_err(|e| io_err(from, e))
}

/// `delete_path(virtual_path, recursive)`.
///
/// Without `recursive`: refuses a non-empty directory (naming it and its
/// count, §E.3), removes an empty directory or a file (§E.5, §E.7),
/// refuses an absent name (§E.8).
///
/// With `recursive`: removes the tree (§E.4). The walk NEVER follows a
/// symlink: `symlink_metadata` on every entry, and a link is removed as a
/// link (§E.1 — a tree containing a link to outside loses the link, and
/// the outside is untouched; §E.2 — a link loop terminates instantly).
///
/// The root itself is never removed, whatever flags are given (§E.6).
pub fn delete_path(root: &Path, path: &VirtualPath, recursive: bool) -> Result<(), FileOpError> {
    let real = RealPath::resolve(root, path)?;
    let canon_root = canonical_root(root)?;
    if real.as_path() == canon_root {
        return Err(FileOpError::RootProtected {
            operation: "deleted",
        });
    }
    let m = match fs::symlink_metadata(real.as_path()) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(FileOpError::NotFound {
                path: path.as_str().to_string(),
            })
        }
        Err(e) => return Err(io_err(path, e)),
    };
    if m.file_type().is_symlink() {
        // Resolved paths are never symlinks (resolve canonicalises), but
        // boring beats clever: remove it as a link, never follow it.
        return fs::remove_file(real.as_path()).map_err(|e| io_err(path, e));
    }
    if m.is_dir() {
        if recursive {
            remove_tree(real.as_path(), path)?;
            return fs::remove_dir(real.as_path()).map_err(|e| io_err(path, e));
        }
        let entries = fs::read_dir(real.as_path())
            .map_err(|e| io_err(path, e))?
            .count();
        if entries > 0 {
            return Err(FileOpError::DirectoryNotEmpty {
                path: path.as_str().to_string(),
                entries,
            });
        }
        return fs::remove_dir(real.as_path()).map_err(|e| io_err(path, e));
    }
    fs::remove_file(real.as_path()).map_err(|e| io_err(path, e))
}

/// Remove everything under `dir`. Called only from `delete_path`'s
/// recursive arm. Boring and obvious: read_dir, symlink_metadata per entry,
/// recurse into real directories, unlink everything else (including —
/// deliberately — every symlink, never follow one).
fn remove_tree(dir: &Path, origin: &VirtualPath) -> Result<(), FileOpError> {
    for entry in fs::read_dir(dir).map_err(|e| io_err(origin, e))? {
        let entry = entry.map_err(|e| io_err(origin, e))?;
        let m = fs::symlink_metadata(entry.path()).map_err(|e| io_err(origin, e))?;
        if m.file_type().is_symlink() {
            // §E.1/§E.2: a symlink is data, not a doorway. Remove the link.
            fs::remove_file(entry.path()).map_err(|e| io_err(origin, e))?;
        } else if m.is_dir() {
            remove_tree(&entry.path(), origin)?;
            fs::remove_dir(entry.path()).map_err(|e| io_err(origin, e))?;
        } else {
            fs::remove_file(entry.path()).map_err(|e| io_err(origin, e))?;
        }
    }
    Ok(())
}
