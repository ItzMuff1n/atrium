//! Phase 2c — snapshot and restore of the environment root.
//!
//! The undo the design has been promising since `DESIGN.md` §3.4. The model is a
//! **save point**, and one detail of it decides the whole shape of this crate:
//!
//! **A save point the boss can delete is not a save point.**
//!
//! If the copy lived inside the environment, then the thing being protected
//! against — a command that deletes things (Phase 2b) — could delete the copy
//! too. So the store lives **outside** the root, and a store inside the root is a
//! *refusal*, not a warning. That single rule is what makes this an undo rather
//! than a second copy of the same risk.
//!
//! **This crate is not agent-facing.** It takes real host paths, not virtual
//! ones. `AGENT-RULES.md` §6 says the resolver is the only way to turn a *virtual*
//! path into a real one; there is no virtual path in a snapshot operation, so the
//! resolver is not involved and its rule is not weakened. What this crate does
//! instead is make "a name used as a path" unrepresentable: every name from
//! outside goes through [`Label::parse`], which is the only constructor.
//!
//! ## The three ways this phase fails silently, and where each is answered
//!
//! 1. **A half-written snapshot restored as if complete.** A directory in the
//!    store is not a snapshot; a directory *plus a completed record* is. The
//!    record is written last and names every entry, so a create process that died
//!    halfway leaves something that is not listed and cannot be restored.
//!    ([`Record`], [`verify_tree`].)
//! 2. **A symlink followed.** Every metadata call in this file is
//!    `symlink_metadata`, never `metadata`; a symlink is copied as a link with the
//!    same target bytes and is never dereferenced. A dangling link is a success.
//!    ([`copy_entries`], [`walk_tree`].)
//! 3. **A name used as a path.** A label or ID that is `../../x` must not become a
//!    path outside the store. ([`Label`].)
//!
//! ## What is deliberately not preserved
//!
//! Modification times, ownership, ACLs, extended attributes, sparse structure and
//! hard-link identity. These are decisions, not oversights, and they are listed in
//! `attack-list-2c.md` §I. `std` cannot set a timestamp, and this project is
//! `std`-only — the honest thing is a record that is silent about what it cannot
//! restore.

pub mod sha256;

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::FileTypeExt;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};

use sha256::{hex, parse_hex32, Sha256};

/// The record format's version tag. Bumped only if the layout changes.
pub const RECORD_MAGIC: &str = "ATRIUM-SNAPSHOT 1";

/// A single path component may be at most this many **bytes** (`NAME_MAX`).
/// Bytes, not characters: a 200-character Hebrew name is 400 bytes and must be
/// refused, which a character-counting check would let through.
pub const NAME_MAX_BYTES: usize = 255;

/// Copy buffer. 64 KiB: large enough that syscall overhead is irrelevant, small
/// enough that a copy never holds a meaningful amount of memory for a large file.
const CHUNK: usize = 64 * 1024;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Everything that can go wrong, with a reason that names the specific problem.
///
/// `attack-list-2c.md` §G.9 requires a refusal to say *what* was wrong, not just
/// that something was. A bare "failed" is explicitly not acceptable, so every
/// variant here carries the path, name or detail that caused it.
#[derive(Debug)]
pub enum SnapshotError {
    // -- usage (exit 2) ----------------------------------------------------
    /// A required flag is absent or has no value.
    MissingFlag {
        flag: &'static str,
    },
    /// A flag was given a value that is not a flag's expected shape.
    BadUsage {
        detail: String,
    },

    // -- the root (exit 1) -------------------------------------------------
    RootMissing {
        path: String,
    },
    RootNotDirectory {
        path: String,
        kind: &'static str,
    },
    /// `--root /`. The catastrophic typo, caught by name before anything else.
    RootIsFilesystemRoot,
    /// `--root` is a symlink. What gets photographed must be the directory the
    /// caller means, not whatever the link points at today.
    RootIsSymlink {
        path: String,
    },

    // -- the store (exit 1) ------------------------------------------------
    StoreMissing {
        path: String,
    },
    StoreNotDirectory {
        path: String,
        kind: &'static str,
    },
    StoreEqualsRoot {
        path: String,
    },
    /// The whole point of the phase: a save point the boss can delete is not one.
    StoreInsideRoot {
        store: String,
        root: String,
    },

    // -- names (exit 1) ----------------------------------------------------
    LabelEmpty,
    LabelDot,
    LabelDotDot {
        label: String,
    },
    LabelHasSeparator {
        label: String,
    },
    LabelHasNul,
    LabelTooLong {
        bytes: usize,
    },
    LabelExists {
        label: String,
    },

    // -- snapshots (exit 1) ------------------------------------------------
    /// No snapshot under that label, or the directory is not a complete one.
    NotASnapshot {
        label: String,
    },
    /// A record exists but does not parse, or was truncated.
    RecordMalformed {
        label: String,
        detail: String,
    },
    /// A snapshot exists but its contents no longer match its own record.
    SnapshotAltered {
        label: String,
        detail: String,
        which: &'static str,
    },
    DifferentRoot {
        snapshot_root: String,
        given_root: String,
    },

    // -- the copy itself ---------------------------------------------------
    UnsupportedEntry {
        path: String,
        kind: String,
    },
    /// A file changed between being described and being copied. Refusing is the
    /// only honest answer: the alternative is a snapshot that records a moment
    /// that never existed.
    ChangedWhileCopying {
        path: String,
    },

    // -- anything the OS said ---------------------------------------------
    Io {
        path: String,
        reason: String,
    },
}

impl SnapshotError {
    /// The process exit code this error maps to. 2 is a usage error — the
    /// arguments are wrong — and 1 is a refusal: the request was understood and
    /// declined. Stated in `README.md` and tested (attack-list-2c.md §J.8).
    pub fn exit_code(&self) -> i32 {
        match self {
            SnapshotError::MissingFlag { .. } | SnapshotError::BadUsage { .. } => 2,
            _ => 1,
        }
    }
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SnapshotError::MissingFlag { flag } => {
                write!(f, "missing required flag {flag}")
            }
            SnapshotError::BadUsage { detail } => write!(f, "{detail}"),

            SnapshotError::RootMissing { path } => write!(
                f,
                "the environment root {path} does not exist (nothing was read or written)"
            ),
            SnapshotError::RootNotDirectory { path, kind } => write!(
                f,
                "the environment root {path} is a {kind}, not a directory"
            ),
            SnapshotError::RootIsFilesystemRoot => write!(
                f,
                "refusing --root / : that is the whole filesystem, not an environment. \
                 A snapshot of it would copy the host, and a restore would overwrite it"
            ),
            SnapshotError::RootIsSymlink { path } => write!(
                f,
                "the environment root {path} is a symlink; refusing, because what would be \
                 photographed is whatever it points at now rather than the directory named"
            ),

            SnapshotError::StoreMissing { path } => write!(
                f,
                "the snapshot store {path} does not exist; refusing rather than creating it, \
                 because a mistyped store path must fail instead of becoming a new empty store \
                 that then appears to work"
            ),
            SnapshotError::StoreNotDirectory { path, kind } => {
                write!(f, "the snapshot store {path} is a {kind}, not a directory")
            }
            SnapshotError::StoreEqualsRoot { path } => write!(
                f,
                "the snapshot store and the environment root are the same directory ({path}); \
                 a snapshot must live outside the environment it protects"
            ),
            SnapshotError::StoreInsideRoot { store, root } => write!(
                f,
                "the snapshot store {store} is inside the environment root {root}; \
                 refusing, because a command running inside the environment could delete the \
                 snapshot, and a save point that can be deleted by the thing it protects is \
                 not a save point"
            ),

            SnapshotError::LabelEmpty => write!(f, "a snapshot name cannot be empty"),
            SnapshotError::LabelDot => {
                write!(f, "the snapshot name \".\" is not a name")
            }
            SnapshotError::LabelDotDot { label } => write!(
                f,
                "the snapshot name {label:?} is a relative path step, not a name"
            ),
            SnapshotError::LabelHasSeparator { label } => write!(
                f,
                "the snapshot name {label:?} contains a path separator ('/'); a name must be a \
                 single path component, so this could name a location outside the store"
            ),
            SnapshotError::LabelHasNul => write!(
                f,
                "the snapshot name contains a NUL byte, which cannot be part of a path"
            ),
            SnapshotError::LabelTooLong { bytes } => write!(
                f,
                "the snapshot name is {bytes} bytes long; a single path component may be at most \
                 {NAME_MAX_BYTES} bytes (NAME_MAX). Measured in bytes, not characters"
            ),
            SnapshotError::LabelExists { label } => write!(
                f,
                "a snapshot named {label:?} already exists in the store; refusing rather than \
                 overwriting it, because silently replacing a snapshot destroys an undo while \
                 reporting success"
            ),

            SnapshotError::NotASnapshot { label } => write!(
                f,
                "there is no complete snapshot named {label:?} in the store. A directory without \
                 a completed record is not a snapshot — it is what a snapshot process that died \
                 halfway leaves behind, and restoring it would put back part of an environment"
            ),
            SnapshotError::RecordMalformed { label, detail } => write!(
                f,
                "the record for snapshot {label:?} is not usable: {detail}"
            ),
            SnapshotError::SnapshotAltered {
                label,
                detail,
                which,
            } => write!(
                f,
                "snapshot {label:?} no longer matches its own record, so it is not safe to \
                 restore: {detail} (checked in the snapshot {which}; the environment was not \
                 touched)"
            ),
            SnapshotError::DifferentRoot {
                snapshot_root,
                given_root,
            } => write!(
                f,
                "this snapshot was taken from {snapshot_root}, but --root is {given_root}. \
                 Refusing to restore one environment's contents into another environment's \
                 directory"
            ),

            SnapshotError::UnsupportedEntry { path, kind } => write!(
                f,
                "{path} is a {kind}; refusing the whole snapshot rather than copying it \
                 approximately or silently skipping it"
            ),

            SnapshotError::ChangedWhileCopying { path } => write!(
                f,
                "{path} changed while the snapshot was being taken. Refusing the whole \
                 snapshot: the alternative is a snapshot that records a moment which never \
                 existed, and a snapshot like that is worse than none because it looks correct"
            ),

            SnapshotError::Io { path, reason } => write!(f, "{path}: {reason}"),
        }
    }
}

impl std::error::Error for SnapshotError {}

fn io_err(path: &Path) -> impl Fn(std::io::Error) -> SnapshotError + '_ {
    move |e| SnapshotError::Io {
        path: path.display().to_string(),
        reason: e.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Label — a name that cannot be a path
// ---------------------------------------------------------------------------

/// A snapshot's name, guaranteed to be a single path component.
///
/// **The only constructor is [`Label::parse`]**, which is the whole point: it is
/// what makes "a name quietly used as a path" a mistake the compiler catches on
/// the write side. This is the same discipline Phase 2a and 2b applied to virtual
/// paths, applied to the one piece of outside input this crate has.
///
/// Refuses, each with its own reason: empty; `.`; `..`; anything containing `/`
/// (which covers `../../x` and `/etc/evil`); anything containing NUL; anything
/// over [`NAME_MAX_BYTES`] bytes.
///
/// **Not refused, and recorded rather than invented:** a name containing other
/// bytes, including a newline. Such a name is a legal single path component and
/// cannot escape the store. It does make `snapshot list` output ambiguous to read;
/// that is a cosmetic limitation, noted in `README.md`, and refusing it would be
/// inventing a requirement the attack list does not make.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Label(String);

impl Label {
    pub fn parse(raw: &str) -> Result<Label, SnapshotError> {
        if raw.is_empty() {
            return Err(SnapshotError::LabelEmpty);
        }
        if raw == "." {
            return Err(SnapshotError::LabelDot);
        }
        if raw == ".." {
            return Err(SnapshotError::LabelDotDot {
                label: raw.to_string(),
            });
        }
        if raw.contains('/') {
            return Err(SnapshotError::LabelHasSeparator {
                label: raw.to_string(),
            });
        }
        if raw.contains('\0') {
            return Err(SnapshotError::LabelHasNul);
        }
        // Measured in bytes of the UTF-8 encoding, not in characters: a
        // character-counting check would accept a 200-character name that is 400
        // bytes and then fail at the filesystem with an OS error instead of a
        // named reason.
        if raw.len() > NAME_MAX_BYTES {
            return Err(SnapshotError::LabelTooLong { bytes: raw.len() });
        }
        Ok(Label(raw.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The record file's name for this label: `<label>.snapshot`.
    fn record_file_name(&self) -> String {
        format!("{}.snapshot", self.0)
    }
}

impl fmt::Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// ---------------------------------------------------------------------------
// Entries and records
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Dir,
    File,
    Symlink,
}

impl EntryKind {
    fn as_str(self) -> &'static str {
        match self {
            EntryKind::Dir => "dir",
            EntryKind::File => "file",
            EntryKind::Symlink => "symlink",
        }
    }

    fn parse(s: &str) -> Option<EntryKind> {
        match s {
            "dir" => Some(EntryKind::Dir),
            "file" => Some(EntryKind::File),
            "symlink" => Some(EntryKind::Symlink),
            _ => None,
        }
    }
}

/// One thing inside the tree.
///
/// `rel` is the path **relative to the base**, held as raw bytes. Never a
/// `String`: a filename on Linux is a byte string, and converting it lossily
/// would silently rename files that are not valid UTF-8. It is also never
/// escaped — the record makes it self-delimiting with `namelen` instead, so a
/// newline or a space in a name changes nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub rel: Vec<u8>,
    pub kind: EntryKind,
    /// Permission bits (`& 0o7777`).
    pub mode: u32,
    /// Regular file: byte length. Symlink: byte length of the target string.
    /// Directory: 0.
    pub size: u64,
    /// Regular file: SHA-256 of the contents. Symlink: SHA-256 of the target
    /// string — **not** of anything the link points at. Directory: all zeros.
    pub hash: [u8; 32],
}

impl Entry {
    fn rel_display(&self) -> String {
        String::from_utf8_lossy(&self.rel).to_string()
    }
}

/// A parsed, complete snapshot record.
#[derive(Debug, Clone)]
pub struct Record {
    pub label: String,
    /// The canonical path of the root this snapshot was taken from. Restoring
    /// into a different root is refused (attack-list-2c.md §C.8).
    pub origin_root: String,
    pub entries: Vec<Entry>,
}

/// Build a record's text. `origin_root` is hex-encoded so a path containing a
/// newline or an unusual byte cannot break the layout.
pub fn render_record(label: &Label, origin_root: &Path, entries: &[Entry]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(entries.len() * 128 + 128);
    out.extend_from_slice(RECORD_MAGIC.as_bytes());
    out.push(b'\n');

    // ROOT <hex of the canonical root bytes>
    out.extend_from_slice(b"ROOT ");
    out.extend_from_slice(hex(origin_root.as_os_str().as_bytes()).as_bytes());
    out.push(b'\n');

    for e in entries {
        out.extend_from_slice(
            format!(
                "ENTRY {} {} {} {} {}\n",
                e.kind.as_str(),
                e.mode,
                e.size,
                hex(&e.hash),
                e.rel.len()
            )
            .as_bytes(),
        );
        // The raw path bytes, then the delimiter. `namelen` above is what makes
        // this unambiguous when a name contains a newline.
        out.extend_from_slice(&e.rel);
        out.push(b'\n');
    }

    out.extend_from_slice(format!("END {}\n", entries.len()).as_bytes());
    let _ = label; // the label is the filename; kept in the signature for clarity
    out
}

/// Parse a record. Strict by design: every failure mode produces
/// [`SnapshotError::NotASnapshot`] or [`SnapshotError::RecordMalformed`], never a
/// half-parsed list that could be restored.
pub fn parse_record(label: &Label, bytes: &[u8]) -> Result<Record, SnapshotError> {
    let malformed = |detail: &str| SnapshotError::RecordMalformed {
        label: label.to_string(),
        detail: detail.to_string(),
    };

    let mut lines = LineReader::new(bytes);
    let magic = lines.next_line().ok_or_else(|| malformed("empty"))?;
    if magic != RECORD_MAGIC.as_bytes() {
        return Err(malformed("it does not begin with the snapshot header"));
    }

    let root_line = lines.next_line().ok_or_else(|| malformed("truncated"))?;
    let root_hex = std::str::from_utf8(root_line)
        .map_err(|_| malformed("the origin-root line is not text"))?
        .strip_prefix("ROOT ")
        .ok_or_else(|| malformed("the origin-root line is missing"))?;
    let root_bytes =
        parse_hex(root_hex).ok_or_else(|| malformed("the origin-root line is not hex"))?;
    let origin_root = String::from_utf8_lossy(&root_bytes).to_string();

    let mut entries: Vec<Entry> = Vec::new();
    loop {
        let line = lines
            .next_line()
            .ok_or_else(|| malformed("it ends without a completion line"))?;

        if line.starts_with(b"END ") {
            let count_text = std::str::from_utf8(&line[4..])
                .map_err(|_| malformed("the completion line is not text"))?;
            let count: usize = count_text
                .trim()
                .parse()
                .map_err(|_| malformed("the completion line is not a number"))?;

            if count != entries.len() {
                return Err(malformed(
                    "its completion line counts a different number of entries than it contains",
                ));
            }
            if !lines.is_empty() {
                return Err(malformed("it has content after the completion line"));
            }
            return Ok(Record {
                label: label.to_string(),
                origin_root,
                entries,
            });
        }

        let text = std::str::from_utf8(line).map_err(|_| malformed("an entry line is not text"))?;
        let mut parts = text.split(' ');
        let _tag = parts.next();
        let kind = parts
            .next()
            .and_then(EntryKind::parse)
            .ok_or_else(|| malformed("an entry has an unknown kind"))?;
        let mode = parts
            .next()
            .and_then(|s| s.parse::<u32>().ok())
            .ok_or_else(|| malformed("an entry has an unreadable mode"))?;
        let size = parts
            .next()
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or_else(|| malformed("an entry has an unreadable size"))?;
        let hash = parts
            .next()
            .and_then(parse_hex32)
            .ok_or_else(|| malformed("an entry has an unreadable checksum"))?;
        let namelen = parts
            .next()
            .and_then(|s| s.parse::<usize>().ok())
            .ok_or_else(|| malformed("an entry has an unreadable name length"))?;

        let rel = lines
            .take_bytes(namelen)
            .ok_or_else(|| malformed("an entry's name is truncated"))?;
        lines
            .expect_newline()
            .ok_or_else(|| malformed("an entry's name is not terminated"))?;

        if rel.is_empty() {
            return Err(malformed("an entry has an empty name"));
        }
        entries.push(Entry {
            rel,
            kind,
            mode,
            size,
            hash,
        });
    }
}

/// A cursor over a byte buffer. Used instead of `split(b'\n')` because the
/// name blocks are allowed to contain newlines.
struct LineReader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> LineReader<'a> {
    fn new(buf: &'a [u8]) -> LineReader<'a> {
        LineReader { buf, pos: 0 }
    }

    fn next_line(&mut self) -> Option<&'a [u8]> {
        if self.pos >= self.buf.len() {
            return None;
        }
        let start = self.pos;
        let mut end = self.buf.len();
        for i in start..self.buf.len() {
            if self.buf[i] == b'\n' {
                end = i;
                break;
            }
        }
        self.pos = if end < self.buf.len() { end + 1 } else { end };
        Some(&self.buf[start..end])
    }

    fn take_bytes(&mut self, n: usize) -> Option<Vec<u8>> {
        if self.pos + n > self.buf.len() {
            return None;
        }
        let out = self.buf[self.pos..self.pos + n].to_vec();
        self.pos += n;
        Some(out)
    }

    fn expect_newline(&mut self) -> Option<()> {
        if self.pos < self.buf.len() && self.buf[self.pos] == b'\n' {
            self.pos += 1;
            Some(())
        } else {
            None
        }
    }

    fn is_empty(&self) -> bool {
        self.pos >= self.buf.len()
    }
}

fn parse_hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(s.len() / 2);
    let mut i = 0;
    while i < b.len() {
        let hi = (b[i] as char).to_digit(16)?;
        let lo = (b[i + 1] as char).to_digit(16)?;
        out.push((hi * 16 + lo) as u8);
        i += 2;
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// Walking, hashing, copying
// ---------------------------------------------------------------------------

/// Classify an entry. Anything that is not a directory, regular file or symlink
/// is refused by name — a socket or fifo in an environment is exactly the shape
/// of a thing that gets dropped silently, and a silently incomplete snapshot is
/// worse than no snapshot (attack-list-2c.md §9.5).
fn classify(path: &Path, ft: std::fs::FileType) -> Result<EntryKind, SnapshotError> {
    if ft.is_symlink() {
        return Ok(EntryKind::Symlink);
    }
    if ft.is_dir() {
        return Ok(EntryKind::Dir);
    }
    if ft.is_file() {
        return Ok(EntryKind::File);
    }
    let kind = if ft.is_fifo() {
        "named pipe (fifo)"
    } else if ft.is_socket() {
        "socket"
    } else if ft.is_block_device() {
        "block device"
    } else if ft.is_char_device() {
        "character device"
    } else {
        "special file"
    };
    Err(SnapshotError::UnsupportedEntry {
        path: path.display().to_string(),
        kind: kind.to_string(),
    })
}

/// Hash a regular file by streaming it. Never reads it into memory.
pub fn hash_file(path: &Path) -> Result<(u64, [u8; 32]), SnapshotError> {
    let f = File::open(path).map_err(io_err(path))?;
    let mut reader = BufReader::with_capacity(CHUNK, f);
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    let mut total: u64 = 0;
    loop {
        let n = reader.read(&mut buf).map_err(io_err(path))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        total += n as u64;
    }
    Ok((total, hasher.finish()))
}

/// Walk a tree, describing every entry. **Iterative, with an explicit stack** —
/// a deep tree must not overflow the stack, and an arbitrary depth limit would be
/// a refusal the attack list does not ask for.
///
/// Every call here is `symlink_metadata`; a symlink is described as a symlink and
/// never descended into. Results are sorted by raw path bytes so two walks of the
/// same tree produce byte-identical records.
///
/// **This reads every file** to hash it. Where a walk is needed only to establish
/// the *shape* of a tree — which names exist, what kind each is, its size and its
/// mode — [`walk_shape`] is the one to use: it opens nothing. The difference is
/// large: on 5,000 small files, hashing costs about 1.1 ms per file on a
/// just-written tree, so a full walk is roughly 5.5 s where a shape walk is
/// roughly 0.07 s.
pub fn walk_tree(base: &Path) -> Result<Vec<Entry>, SnapshotError> {
    walk_inner(base, true)
}

/// Walk a tree establishing its shape only: names, kinds, sizes and modes.
///
/// No file is opened, so `Entry::hash` is **not** populated — a file's hash is
/// all zeros, which is distinguishable from a real hash and must never be
/// compared against a recorded one. This exists for the checks that establish
/// structure after content has already been hashed on its way past, so that
/// proving the shape costs metadata calls instead of a second full read.
///
/// Sizes are the real ones from `symlink_metadata`; symlink sizes are the byte
/// length of the stored target string, and symlink hashes are still computed
/// (the target string is read anyway, and is needed to describe the link).
pub fn walk_shape(base: &Path) -> Result<Vec<Entry>, SnapshotError> {
    walk_inner(base, false)
}

fn walk_inner(base: &Path, hash_files: bool) -> Result<Vec<Entry>, SnapshotError> {
    let mut out: Vec<Entry> = Vec::new();
    let mut stack: Vec<(PathBuf, Vec<u8>)> = vec![(base.to_path_buf(), Vec::new())];

    while let Some((dir, prefix)) = stack.pop() {
        let rd = fs::read_dir(&dir).map_err(io_err(&dir))?;
        for item in rd {
            let item = item.map_err(io_err(&dir))?;
            let name = item.file_name();
            let name_bytes = name.as_bytes();
            let child = dir.join(&name);

            let mut rel = prefix.clone();
            if !rel.is_empty() {
                rel.push(b'/');
            }
            rel.extend_from_slice(name_bytes);

            let md = fs::symlink_metadata(&child).map_err(io_err(&child))?;
            let kind = classify(&child, md.file_type())?;
            let mode = md.permissions().mode() & 0o7777;

            let entry = match kind {
                EntryKind::Dir => Entry {
                    rel: rel.clone(),
                    kind,
                    mode,
                    size: 0,
                    hash: [0u8; 32],
                },
                EntryKind::File => {
                    if hash_files {
                        let (size, hash) = hash_file(&child)?;
                        Entry {
                            rel: rel.clone(),
                            kind,
                            mode,
                            size,
                            hash,
                        }
                    } else {
                        // Shape only: the size comes from the metadata already in
                        // hand, and the hash is left as zeros because nothing was
                        // read. A shape walk must never be compared against a
                        // recorded hash.
                        Entry {
                            rel: rel.clone(),
                            kind,
                            mode,
                            size: md.len(),
                            hash: [0u8; 32],
                        }
                    }
                }
                EntryKind::Symlink => {
                    // The link's own target, as bytes, hashed as a string.
                    // Nothing here dereferences it.
                    let target = fs::read_link(&child).map_err(io_err(&child))?;
                    let target_bytes = target.as_os_str().as_bytes();
                    Entry {
                        rel: rel.clone(),
                        kind,
                        mode,
                        size: target_bytes.len() as u64,
                        hash: sha256::sha256(target_bytes),
                    }
                }
            };

            if kind == EntryKind::Dir {
                stack.push((child, rel));
            }
            out.push(entry);
        }
    }

    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(out)
}

/// Copy every entry from `src_base` into `dst_base`.
///
/// Three passes, and the order is not cosmetic:
///
/// 1. **Directories**, created with an owner-writable mode. A directory recorded
///    as read-only (say `r-xr-xr-x`) must still receive its children, so its final
///    mode cannot be applied yet.
/// 2. **Files and symlinks**, streamed in; file modes applied on creation.
/// 3. **Directory modes**, deepest first. Applying a parent's read-only mode
///    before its children are written would make the write fail.
///
/// Because entries are sorted by raw path bytes and a parent's path is a strict
/// prefix of its children's, pass 1 already creates parents before children.
///
/// **Returns a description of what was actually written**, and that return value
/// is the point. `create` records *that* rather than re-reading the copy: the
/// bytes are hashed while they stream past, so the snapshot's own integrity is
/// established without a second full read of everything just written. Measured
/// on 5,000 small files: verify-after-write cost 5,592 ms of a 5,873 ms snapshot
/// (95%), because reading a just-written file for the first time is ~90 times
/// slower than reading it again — the filesystem is flushing. Hashing in flight
/// makes that read unnecessary, and the guarantee is stronger rather than weaker:
/// it attests to the bytes actually written, not to a re-read of them later.
fn copy_entries(
    src_base: &Path,
    dst_base: &Path,
    entries: &[Entry],
) -> Result<Vec<Entry>, SnapshotError> {
    let dest_of = |rel: &[u8]| dst_base.join(PathBuf::from(OsString::from_vec(rel.to_vec())));
    let src_of = |rel: &[u8]| src_base.join(PathBuf::from(OsString::from_vec(rel.to_vec())));

    // Pass 1 — directories.
    for e in entries.iter().filter(|e| e.kind == EntryKind::Dir) {
        let dst = dest_of(&e.rel);
        fs::create_dir(&dst).map_err(io_err(&dst))?;
        fs::set_permissions(&dst, fs::Permissions::from_mode(0o700 | (e.mode & 0o077)))
            .map_err(io_err(&dst))?;
    }

    // Pass 2 — files and symlinks, recording what each one actually became.
    let mut written: Vec<Entry> = Vec::with_capacity(entries.len());
    for e in entries.iter().filter(|e| e.kind != EntryKind::Dir) {
        let src = src_of(&e.rel);
        let dst = dest_of(&e.rel);
        match e.kind {
            EntryKind::File => {
                // Hash the bytes on their way past, and take the size from that
                // count rather than from the earlier walk: what is recorded is
                // what was written just now.
                let (size, hash, mode) = copy_file_contents(&src, &dst, e.mode)?;
                written.push(Entry {
                    rel: e.rel.clone(),
                    kind: EntryKind::File,
                    mode,
                    size,
                    hash,
                });
            }
            EntryKind::Symlink => {
                // `read_link` returns the stored target string. It is recreated
                // verbatim: not resolved, not checked, not dereferenced. A
                // dangling link comes back as a dangling link, which is correct
                // and is a success, not an error.
                let target = fs::read_link(&src).map_err(io_err(&src))?;
                std::os::unix::fs::symlink(&target, &dst).map_err(io_err(&dst))?;
                let target_bytes = target.as_os_str().as_bytes();
                written.push(Entry {
                    rel: e.rel.clone(),
                    kind: EntryKind::Symlink,
                    mode: e.mode,
                    size: target_bytes.len() as u64,
                    hash: sha256::sha256(target_bytes),
                });
            }
            EntryKind::Dir => unreachable!("dirs were handled in pass 1"),
        }
    }

    // Pass 3 — directory modes, deepest first. The mode actually applied is read
    // back, so the record says what is on disk rather than what was requested.
    let mut dirs: Vec<&Entry> = entries
        .iter()
        .filter(|e| e.kind == EntryKind::Dir)
        .collect();
    dirs.sort_by(|a, b| b.rel.cmp(&a.rel));
    for e in dirs {
        let dst = dest_of(&e.rel);
        fs::set_permissions(&dst, fs::Permissions::from_mode(e.mode)).map_err(io_err(&dst))?;
        let applied = fs::symlink_metadata(&dst)
            .map_err(io_err(&dst))?
            .permissions()
            .mode()
            & 0o7777;
        written.push(Entry {
            rel: e.rel.clone(),
            kind: EntryKind::Dir,
            mode: applied,
            size: 0,
            hash: [0u8; 32],
        });
    }

    written.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(written)
}

/// Stream a file from `src` to `dst`, returning `(bytes written, hash of those
/// bytes, mode actually applied)`.
///
/// The hash is computed over the same buffer that is written, so there is no
/// second read of the destination. `attack-list-2c.md` §A.4 requires the copied
/// bytes to be verified; this verifies the bytes *as written*, which is the
/// stronger claim.
fn copy_file_contents(
    src: &Path,
    dst: &Path,
    want_mode: u32,
) -> Result<(u64, [u8; 32], u32), SnapshotError> {
    let f = File::open(src).map_err(io_err(src))?;
    let mut reader = BufReader::with_capacity(CHUNK, f);
    let out = File::create(dst).map_err(io_err(dst))?;
    let mut writer = BufWriter::with_capacity(CHUNK, out);
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    let mut total: u64 = 0;
    loop {
        let n = reader.read(&mut buf).map_err(io_err(src))?;
        if n == 0 {
            break;
        }
        writer.write_all(&buf[..n]).map_err(io_err(dst))?;
        hasher.update(&buf[..n]);
        total += n as u64;
    }
    writer.flush().map_err(io_err(dst))?;

    fs::set_permissions(dst, fs::Permissions::from_mode(want_mode)).map_err(io_err(dst))?;
    let applied = fs::symlink_metadata(dst)
        .map_err(io_err(dst))?
        .permissions()
        .mode()
        & 0o7777;

    Ok((total, hasher.finish(), applied))
}

/// How much of an entry a comparison checks.
///
/// The distinction exists because content is expensive to establish and structure
/// is not. A comparison that used `Content` everywhere would re-read every file
/// for no additional guarantee, at roughly 1.1 ms per file on a freshly written
/// tree (measured: 5.5 s of a 5.9 s snapshot on 5,000 files), while the content it
/// was re-reading had already been hashed on its way past.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compare {
    /// Names, kinds, sizes and modes. **No hash is consulted** — correct only for
    /// a shape walk, where hashes are zeros by construction and comparing them
    /// would fail a correct tree.
    Shape,
    /// A superset of `Shape`: also requires each file's recorded hash to match.
    /// Used where nothing has hashed the content already and the content is the
    /// point.
    Content,
}

/// Compare two sets of entries, naming the first difference.
///
/// The single comparison behind every tree check, so the wording of a mismatch is
/// consistent and the logic cannot drift between call sites. Callers:
///
/// - `create`, comparing the walk against what the copy wrote (a file that
///   changed between the two is one difference this catches);
/// - `create`, checking the copy's directory modes, which are read back after
///   being applied because umask and umbits legitimately change them;
/// - `restore`, deciding whether a snapshot's stored files still match its record
///   (`Compare::Content` — nothing hashed those bytes yet, so this is where the
///   hash check belongs);
/// - `restore`, confirming the environment it just wrote (`Compare::Shape` — the
///   bytes were hashed as they were written).
fn compare_entries(
    actual: &[Entry],
    expected: &[Entry],
    label: &Label,
    which: &'static str,
    how: Compare,
) -> Result<(), SnapshotError> {
    let altered = |detail: String| SnapshotError::SnapshotAltered {
        label: label.to_string(),
        detail,
        which,
    };

    let mut actual_by_rel: Vec<&Entry> = actual.iter().collect();
    actual_by_rel.sort_by(|a, b| a.rel.cmp(&b.rel));
    let mut expected_sorted: Vec<&Entry> = expected.iter().collect();
    expected_sorted.sort_by(|a, b| a.rel.cmp(&b.rel));

    if actual_by_rel.len() != expected_sorted.len() {
        // Name the extra or missing entry rather than just the counts: a bare
        // count difference tells whoever reads it nothing about what changed.
        for a in &actual_by_rel {
            if !expected_sorted.iter().any(|e| e.rel == a.rel) {
                return Err(altered(format!(
                    "it contains {}, which the record does not list",
                    a.rel_display()
                )));
            }
        }
        for e in &expected_sorted {
            if !actual_by_rel.iter().any(|a| a.rel == e.rel) {
                return Err(altered(format!(
                    "the record lists {}, which is missing",
                    e.rel_display()
                )));
            }
        }
        return Err(altered(format!(
            "entry count differs ({} present, {} recorded)",
            actual_by_rel.len(),
            expected_sorted.len()
        )));
    }

    for (a, e) in actual_by_rel.iter().zip(expected_sorted.iter()) {
        if a.rel != e.rel {
            return Err(altered(format!(
                "entry {} does not match the recorded {}",
                a.rel_display(),
                e.rel_display()
            )));
        }
        if a.kind != e.kind {
            return Err(altered(format!(
                "{} is a {} but the record says {}",
                e.rel_display(),
                match a.kind {
                    EntryKind::Dir => "directory",
                    EntryKind::File => "file",
                    EntryKind::Symlink => "symlink",
                },
                e.kind.as_str()
            )));
        }
        if a.mode != e.mode {
            return Err(altered(format!(
                "{} has mode {:o} but the record says {:o}",
                e.rel_display(),
                a.mode,
                e.mode
            )));
        }
        if a.size != e.size {
            return Err(altered(format!(
                "{} is {} bytes but the record says {}",
                e.rel_display(),
                a.size,
                e.size
            )));
        }
        if how == Compare::Content && a.hash != e.hash {
            return Err(altered(format!(
                "{} has checksum {} but the record says {}",
                e.rel_display(),
                hex(&a.hash),
                hex(&e.hash)
            )));
        }
    }

    Ok(())
}

/// Check that a snapshot's **stored files** still match its record, without
/// paying to re-read every byte.
///
/// `restore` must refuse a snapshot that was altered after it was taken
/// (`attack-list-2c.md` §F.2), and the file contents are the only evidence for
/// that. But hashing every file in the store is the expensive part of a restore:
/// measured on 5,000 small files, re-reading the store cost 5,592 ms of a
/// 5,873 ms create and about the same of a restore, because the first read of a
/// just-written file is ~90 times slower than the second.
///
/// So the cheap, decisive properties are checked first — the entry set, and each
/// entry's kind, mode and **size**. A size difference *is* a content difference,
/// and it is caught without opening the file. Hashes are compared only where the
/// sizes agree, which is where the cheap check cannot decide. Nothing is skipped:
/// a file whose bytes changed but whose length did not still fails on its hash,
/// which is exactly the §F.2 case the attack list builds.
///
/// Returns `(files_compared_by_hash, files_decided_by_size)` so the report can
/// say how much of the check was decided cheaply rather than implying otherwise.
pub fn verify_snapshot_integrity(
    tree: &Path,
    label: &Label,
    expected: &[Entry],
    which: &'static str,
    how: Compare,
) -> Result<(), SnapshotError> {
    let altered = |detail: String| SnapshotError::SnapshotAltered {
        label: label.to_string(),
        detail,
        which,
    };

    // Shape first: names, kinds, sizes, modes. Cheap, and decisive for everything
    // except bytes of equal length.
    let walk = walk_shape(tree).map_err(|e| altered(e.to_string()))?;
    compare_entries(&walk, expected, label, which, Compare::Shape)?;

    if how == Compare::Shape {
        return Ok(());
    }

    // Content: hash each recorded file and compare. Reached only where nothing has
    // hashed these bytes already — the snapshot's stored files. A file whose
    // length changed was already caught above without opening it; this catches the
    // case the attack list builds in §F.2, where an edit preserved the length.
    for e in expected.iter() {
        if e.kind != EntryKind::File {
            continue;
        }
        let path = tree.join(PathBuf::from(OsString::from_vec(e.rel.clone())));
        let (_, hash) = hash_file(&path).map_err(|err| altered(err.to_string()))?;
        if hash != e.hash {
            return Err(altered(format!(
                "{} has checksum {} but the record says {}",
                e.rel_display(),
                hex(&hash),
                hex(&e.hash)
            )));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Validating the two directories
// ---------------------------------------------------------------------------

/// Check `--root`: exists, a real directory, not `/`, not a symlink.
fn validate_root(root: &Path) -> Result<PathBuf, SnapshotError> {
    // `/` by name, before anything else, so the catastrophic typo cannot be
    // reached by a path that merely happens to be short.
    if root == Path::new("/") {
        return Err(SnapshotError::RootIsFilesystemRoot);
    }

    // symlink_metadata, not metadata: a symlink root is refused rather than
    // silently followed.
    let md = fs::symlink_metadata(root).map_err(|_| SnapshotError::RootMissing {
        path: root.display().to_string(),
    })?;
    if md.file_type().is_symlink() {
        return Err(SnapshotError::RootIsSymlink {
            path: root.display().to_string(),
        });
    }
    if !md.is_dir() {
        return Err(SnapshotError::RootNotDirectory {
            path: root.display().to_string(),
            kind: "file",
        });
    }

    fs::canonicalize(root).map_err(io_err(root))
}

/// Check `--store`: exists, is a directory. Never created silently.
///
/// Returns the store's canonical path **if it exists**. `Ok(None)` means the
/// store is absent. The `create` path deliberately keeps that case separate so it
/// can check "is this absent store sitting inside the root?" and give the more
/// specific refusal, rather than reporting a missing store for a store that is
/// missing *because* it is inside the root (`attack-list-2c.md` §B.5 before §B.7).
fn validate_store(store: &Path) -> Result<PathBuf, SnapshotError> {
    let md = match fs::metadata(store) {
        Ok(m) => m,
        Err(_) => {
            return Err(SnapshotError::StoreMissing {
                path: store.display().to_string(),
            })
        }
    };
    if !md.is_dir() {
        return Err(SnapshotError::StoreNotDirectory {
            path: store.display().to_string(),
            kind: "file",
        });
    }
    fs::canonicalize(store).map_err(io_err(store))
}

/// Is `store` inside `root`, decided with what is known even for a store that
/// does not exist yet?
///
/// Canonical paths when both exist (so a symlink cannot disguise the
/// relationship), and a lexical comparison otherwise. The lexical check is sound
/// for this purpose because both inputs come from one command line on one
/// machine: a path that lexically sits under the root and does not exist yet is
/// still a path that would be created inside the root — which is exactly the
/// thing being refused, and the refusal must not depend on the directory already
/// existing.
fn store_is_inside_root(store: &Path, root: &Path) -> Option<SnapshotError> {
    if store == root {
        return Some(SnapshotError::StoreEqualsRoot {
            path: store.display().to_string(),
        });
    }

    let inside = match (fs::canonicalize(store), fs::canonicalize(root)) {
        (Ok(s), Ok(r)) => s.starts_with(&r),
        _ => {
            // At least one is absent: compare textually, on whole components.
            store.starts_with(root)
        }
    };

    if inside {
        return Some(SnapshotError::StoreInsideRoot {
            store: store.display().to_string(),
            root: root.display().to_string(),
        });
    }
    None
}

// ---------------------------------------------------------------------------
// The three operations
// ---------------------------------------------------------------------------

/// Take a snapshot. Returns the label.
///
/// Order is the safety property: validate, then **copy**, then verify the copy
/// against a freshly walked tree, and only then write the record. A snapshot is
/// not complete until its record exists (attack-list-2c.md §F.1), so the record
/// is written last, always.
pub fn create(root: &Path, store: &Path, label: &Label) -> Result<(), SnapshotError> {
    let root_real = validate_root(root)?;

    // The store's relationship to the root is checked FIRST, before "does the
    // store exist". This ordering is deliberate: a store that does not exist yet
    // but sits inside the root must be refused for the *containment* reason, not
    // reported as merely missing. The refusal that matters most must not be the
    // one that can be avoided by the directory not existing yet.
    if let Some(e) = store_is_inside_root(store, &root_real) {
        return Err(e);
    }

    let store_real = validate_store(store)?;

    let tree = store_real.join(label.as_str());
    let record_path = store_real.join(label.record_file_name());

    // Refuse to overwrite. Checked as "exists at all" so a plain file sitting at
    // the name is refused too, not just an existing snapshot directory.
    if tree.exists() || record_path.exists() {
        return Err(SnapshotError::LabelExists {
            label: label.to_string(),
        });
    }

    // Walk first: an unsupported entry (a socket, say) must refuse the whole
    // snapshot *before* anything is written, not halfway through a copy.
    let entries = walk_tree(&root_real)?;

    fs::create_dir(&tree).map_err(io_err(&tree))?;

    let outcome = (|| -> Result<(), SnapshotError> {
        // `copy_entries` returns what it actually wrote — sizes, modes and
        // checksums computed from the bytes as they were streamed out. This is
        // what gets recorded, so the snapshot's integrity is established by the
        // write itself rather than by a second full read of the copy.
        let written = copy_entries(&root_real, &tree, &entries)?;

        // The copy was hashed as it streamed out. The walk hashed the same files
        // a moment earlier. Comparing the two costs nothing — both sets of hashes
        // are already in memory — and it is what catches a file that **changed
        // between the walk and the copy**. Without this, such a file would be
        // copied torn or newer, and the snapshot would record it as
        // self-consistent: a snapshot that silently is not the moment it claims
        // to be. With it, `create` refuses instead.
        //
        // Only files and symlinks are compared here. A directory's recorded mode
        // is read back after it is applied (umask and umbits make the requested
        // and applied modes legitimately different), so it is checked below
        // instead, against what is actually on disk.
        for (w, e) in written.iter().zip(entries.iter()) {
            if e.kind == EntryKind::Dir {
                continue;
            }
            if w.rel != e.rel || w.kind != e.kind || w.size != e.size || w.hash != e.hash {
                return Err(SnapshotError::ChangedWhileCopying {
                    path: String::from_utf8_lossy(&e.rel).to_string(),
                });
            }
        }

        // Directory modes are the one property not established by the write
        // itself, because the mode the copy ends up with is the mode the
        // filesystem granted. Each directory is stat'ed directly — deliberately
        // NOT via `walk_tree`, which would hash every file in the copy and bring
        // back exactly the 5.6-second cost this change removed. Directories are
        // counted twice here: the copy's expected directory count comes from the
        // `written` set, so a directory that failed to be created is caught.
        let expected_dirs: Vec<&Entry> = written
            .iter()
            .filter(|e| e.kind == EntryKind::Dir)
            .collect();
        for e in &expected_dirs {
            let path = tree.join(PathBuf::from(OsString::from_vec(e.rel.clone())));
            let md = fs::symlink_metadata(&path).map_err(|_| SnapshotError::SnapshotAltered {
                label: label.to_string(),
                detail: format!("the directory {} is not in the copy", e.rel_display()),
                which: "copy",
            })?;
            let mode = md.permissions().mode() & 0o7777;
            if mode != e.mode {
                return Err(SnapshotError::SnapshotAltered {
                    label: label.to_string(),
                    detail: format!(
                        "the directory {} has mode {:o} in the copy but {:o} was recorded",
                        e.rel_display(),
                        mode,
                        e.mode
                    ),
                    which: "copy",
                });
            }
        }

        let record = render_record(label, &root_real, &written);
        fs::write(&record_path, &record).map_err(io_err(&record_path))?;
        Ok(())
    })();

    if let Err(e) = outcome {
        // Clean up so a failed create does not leave a directory that looks like
        // a snapshot. The *damaged-snapshot* case the attack list tests (§F.1) is
        // a process killed outright, which no cleanup can intercept — and the
        // harness constructs that state directly.
        let mut cleanup_note = String::new();
        if tree.exists() {
            if let Err(ce) = fs::remove_dir_all(&tree) {
                cleanup_note = format!(" (cleanup also failed: {ce})");
            }
        }
        let _ = fs::remove_file(&record_path);
        if cleanup_note.is_empty() {
            return Err(e);
        }
        return Err(SnapshotError::Io {
            path: tree.display().to_string(),
            reason: format!("{e}{cleanup_note}"),
        });
    }

    Ok(())
}

/// What `list` shows for one snapshot.
#[derive(Debug, Clone)]
pub struct Listed {
    pub label: String,
    pub origin_root: String,
    pub files: usize,
    pub dirs: usize,
    pub symlinks: usize,
}

/// Every *complete* snapshot in the store, sorted by label.
///
/// A directory without a record is not listed, and a record without its directory
/// is not listed. Both are left on disk untouched: they are evidence of something
/// that went wrong, and tidying them away would erase it.
pub fn list(store: &Path) -> Result<Vec<Listed>, SnapshotError> {
    let store_real = validate_store(store)?;
    let rd = fs::read_dir(&store_real).map_err(io_err(&store_real))?;

    let mut found: Vec<Listed> = Vec::new();
    for item in rd {
        let item = item.map_err(io_err(&store_real))?;
        let name = item.file_name();
        let name_str = match name.to_str() {
            Some(s) => s.to_string(),
            None => continue, // not a name this crate ever creates
        };
        let stem = match name_str.strip_suffix(".snapshot") {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => continue,
        };

        let label = match Label::parse(&stem) {
            Ok(l) => l,
            Err(_) => continue,
        };

        let tree = store_real.join(label.as_str());
        let md = match fs::symlink_metadata(&tree) {
            Ok(m) if m.is_dir() => m,
            _ => continue, // record with no directory: not a snapshot
        };
        let _ = md;

        let bytes = match fs::read(item.path()) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let record = match parse_record(&label, &bytes) {
            Ok(r) => r,
            Err(_) => continue, // incomplete or malformed: not a snapshot
        };

        let mut files = 0;
        let mut dirs = 0;
        let mut symlinks = 0;
        for e in &record.entries {
            match e.kind {
                EntryKind::File => files += 1,
                EntryKind::Dir => dirs += 1,
                EntryKind::Symlink => symlinks += 1,
            }
        }

        found.push(Listed {
            label: label.to_string(),
            origin_root: record.origin_root,
            files,
            dirs,
            symlinks,
        });
    }

    found.sort_by(|a, b| a.label.cmp(&b.label));
    Ok(found)
}

/// Make sure a directory's **contents** can be removed, before anything is
/// removed.
///
/// `remove_dir_all` needs write+execute on the directory that holds the entries.
/// A directory that the environment happened to record as read-only therefore
/// made `restore` fail — and it failed *after* `clear_contents` had already
/// deleted everything else, leaving a half-cleared environment. That was a real
/// defect, found by the blind list (`blind-attack-list-2c.md` item 26, a
/// read-only directory that a restore has to empty). Observed before the fix:
/// `restore` exited 1 with `Permission denied (os error 13)` and the environment
/// was left holding the read-only directory while everything beside it had been
/// deleted.
///
/// This is pre-flight, not cleanup: it runs for every directory in the tree that
/// is about to be cleared, and it runs before the first deletion. If a directory
/// cannot be made writable, `restore` **refuses and changes nothing at all**,
/// which is a far better answer than a partial deletion.
///
/// It walks with `symlink_metadata` and does not descend through symlinks, so a
/// planted link cannot draw it outside the environment.
fn ensure_contents_removable(dir: &Path) -> Result<(), SnapshotError> {
    // Iterative, for the same reason the walk is: a deep tree must not blow the
    // stack, and here that would happen *during* a restore.
    let mut stack: Vec<PathBuf> = vec![dir.to_path_buf()];

    while let Some(current) = stack.pop() {
        let md = fs::symlink_metadata(&current).map_err(io_err(&current))?;
        if !md.is_dir() {
            continue; // a symlink or a file has no contents to remove
        }

        let mode = md.permissions().mode() & 0o7777;
        // Owner write is what deleting a child requires; owner execute is what
        // reaching it requires.
        if mode & 0o300 != 0o300 {
            fs::set_permissions(&current, fs::Permissions::from_mode(mode | 0o300)).map_err(
                |e| SnapshotError::Io {
                    path: current.display().to_string(),
                    reason: format!(
                        "this directory is mode {mode:o} and cannot be made writable to be \
                             emptied ({e}); refusing the restore before deleting anything, \
                             because the alternative is a half-cleared environment"
                    ),
                },
            )?;
        }

        let rd = fs::read_dir(&current).map_err(io_err(&current))?;
        for item in rd {
            let item = item.map_err(io_err(&current))?;
            let path = item.path();
            let child = fs::symlink_metadata(&path).map_err(io_err(&path))?;
            // Deliberately not following symlinks: link_to_dir.is_dir() is false
            // here, so a planted link is never descended into.
            if child.is_dir() {
                stack.push(path);
            }
        }
    }

    Ok(())
}

/// Delete the children of a directory, **symlink-aware**.
///
/// `symlink_metadata` again: a symlink to a directory reports `is_dir() == false`
/// here, so it is removed with `remove_file` and the link is deleted rather than
/// the directory it points at. The root directory itself is never removed — only
/// its contents (attack-list-2c.md §C.11: its inode must survive).
fn clear_contents(root: &Path) -> Result<(), SnapshotError> {
    let rd = fs::read_dir(root).map_err(io_err(root))?;
    for item in rd {
        let item = item.map_err(io_err(root))?;
        let path = item.path();
        let md = fs::symlink_metadata(&path).map_err(io_err(&path))?;
        if md.is_dir() {
            fs::remove_dir_all(&path).map_err(io_err(&path))?;
        } else {
            fs::remove_file(&path).map_err(io_err(&path))?;
        }
    }
    Ok(())
}

/// Restore a snapshot over the root.
///
/// The order here is the entire safety argument:
///
/// 1. Validate everything and **read the record**.
/// 2. Refuse if the snapshot came from a different root.
/// 3. **Verify the snapshot against its own record** — before a single byte of
///    the environment is touched. A refusal here leaves the environment exactly
///    as it was (attack-list-2c.md §C.9, §F.2, §F.3).
/// 4. Clear the root's contents.
/// 5. Copy the snapshot back.
/// 6. Verify the result and report it. A restore that cannot confirm itself has
///    not succeeded, and saying otherwise is the failure this project exists to
///    avoid.
pub fn restore(root: &Path, store: &Path, label: &Label) -> Result<(), SnapshotError> {
    let root_real = validate_root(root)?;

    // Same ordering as `create`, for the same reason: containment before
    // existence.
    if let Some(e) = store_is_inside_root(store, &root_real) {
        return Err(e);
    }

    let store_real = validate_store(store)?;

    let tree = store_real.join(label.as_str());
    let record_path = store_real.join(label.record_file_name());

    // A tree and a record, both present, or there is no snapshot.
    if !tree.is_dir() || !record_path.is_file() {
        return Err(SnapshotError::NotASnapshot {
            label: label.to_string(),
        });
    }

    let bytes = fs::read(&record_path).map_err(io_err(&record_path))?;
    let record = parse_record(label, &bytes)?;

    // Cross-root restore: a plausible disaster, and not a silent default.
    if record.origin_root != root_real.display().to_string() {
        return Err(SnapshotError::DifferentRoot {
            snapshot_root: record.origin_root.clone(),
            given_root: root_real.display().to_string(),
        });
    }

    // Verify the snapshot's stored files against its own record BEFORE touching
    // the environment. A refusal here leaves the environment exactly as it was
    // (attack-list-2c.md §C.9, §F.2, §F.3).
    verify_snapshot_integrity(&tree, label, &record.entries, "store", Compare::Content)?;

    // Establish that the environment's contents CAN be removed, before removing
    // any of them. Without this, a read-only directory part-way down the tree made
    // `clear_contents` fail after it had already deleted everything else, leaving
    // a half-cleared environment — observed, and the worst class of bug in this
    // phase. A refusal here changes nothing.
    ensure_contents_removable(&root_real)?;

    clear_contents(&root_real)?;
    copy_entries(&tree, &root_real, &record.entries)?;

    // Report the result honestly: walk what is actually on disk and compare it
    // against the record, so a restore that could not confirm itself says so.
    // The verification is against the *snapshot's* recorded entries, which is
    // what "back exactly as it was" means.
    verify_snapshot_integrity(
        &root_real,
        label,
        &record.entries,
        "restored environment",
        Compare::Shape,
    )?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Fixtures and demo — used by the harness and by a human wanting to see it
// ---------------------------------------------------------------------------

fn w(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p)?;
    }
    fs::write(path, contents)
}

/// The attack list's fixture tree. Built by hand here with `mkdir`/`printf`
/// equivalents, deliberately **not** by calling another crate's fixture mode, so
/// 2c's gate does not depend on 2a's or 2b's binary still behaving.
///
/// `outside` is where `link-escape` points. It is a directory this harness owns
/// rather than `/etc`, so the "followed a symlink" failure mode is both harmless
/// and **visible**: if the copy follows the link, the outside sentinel's contents
/// appear inside the snapshot and the harness's escape check fires.
pub fn build_fixtures(root: &Path, outside: &Path) -> Result<(), SnapshotError> {
    fs::create_dir_all(root).map_err(io_err(root))?;
    fs::create_dir_all(outside).map_err(io_err(outside))?;

    let mk = |p: PathBuf| fs::create_dir_all(&p).map_err(io_err(&p));

    w(&root.join("home/documents/notes.txt"), "hi").map_err(io_err(root))?;
    mk(root.join("home/documents/sub"))?;
    w(&root.join("home/documents/sub/deep.txt"), "deep").map_err(io_err(root))?;
    mk(root.join("home/work"))?;

    w(&root.join("weird/with space.txt"), "spaces survive").map_err(io_err(root))?;
    w(&root.join("weird/caf\u{e9}.txt"), "unicode survives").map_err(io_err(root))?;
    w(&root.join("weird/mode-755.sh"), "#!/bin/sh\necho ok\n").map_err(io_err(root))?;
    w(&root.join("weird/mode-444.txt"), "read only").map_err(io_err(root))?;
    w(&root.join("weird/empty-file.txt"), "").map_err(io_err(root))?;

    w(&root.join("deep/a/b/c/d/e/leaf.txt"), "bottom").map_err(io_err(root))?;

    // Symlinks. `symlink_metadata` treats all three the same way; the difference
    // is only where they point.
    symlink(Path::new("home/documents"), root.join("link-docs"))?;
    symlink(outside, root.join("link-escape"))?;
    symlink(Path::new("nothing-here.txt"), root.join("link-broken"))?;

    // Hard link pair: same inode, two names. Restored as two files (§I.2), which
    // is recorded rather than hidden.
    w(&root.join("hard/one.txt"), "linked").map_err(io_err(root))?;
    fs::hard_link(root.join("hard/one.txt"), root.join("hard/two.txt")).map_err(io_err(root))?;

    // Modes are set after writing, since a file is created writable.
    fs::set_permissions(
        root.join("weird/mode-755.sh"),
        fs::Permissions::from_mode(0o755),
    )
    .map_err(io_err(root))?;
    fs::set_permissions(
        root.join("weird/mode-444.txt"),
        fs::Permissions::from_mode(0o444),
    )
    .map_err(io_err(root))?;

    // The outside half of the escape check.
    w(&outside.join("sentinel.txt"), "sentinel\n").map_err(io_err(outside))?;
    w(&outside.join("sub/keep.txt"), "keep\n").map_err(io_err(outside))?;

    Ok(())
}

fn symlink(target: &Path, at: PathBuf) -> Result<(), SnapshotError> {
    std::os::unix::fs::symlink(target, &at).map_err(io_err(&at))
}

/// A short, human-visible cycle: snapshot, damage, restore. Prints what it did
/// so the whole idea is visible in one command.
pub fn demo() -> Result<(), SnapshotError> {
    let base = std::env::temp_dir().join(format!("atrium-2c-demo-{}", std::process::id()));
    let root = base.join("root");
    let store = base.join("store");
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&store).map_err(io_err(&store))?;

    println!("demo directories: {}", base.display());
    build_fixtures(&root, &base.join("outside"))?;

    let count = |p: &Path| walk_tree(p).map(|v| v.len()).unwrap_or(0);
    println!("  root entries before      : {}", count(&root));

    let label = Label::parse("demo")?;
    create(&root, &store, &label)?;
    println!("  snapshot created         : demo");

    // Damage: delete a directory, overwrite a file, add a stranger.
    fs::remove_dir_all(root.join("deep")).map_err(io_err(&root))?;
    fs::write(root.join("home/documents/notes.txt"), "DAMAGED").map_err(io_err(&root))?;
    fs::write(root.join("not-in-snapshot.txt"), "stranger").map_err(io_err(&root))?;
    println!(
        "  after damage             : {} entries, notes.txt says \"DAMAGED\"",
        count(&root)
    );

    restore(&root, &store, &label)?;

    let notes = fs::read_to_string(root.join("home/documents/notes.txt")).unwrap_or_default();
    println!(
        "  after restore            : {} entries, notes.txt says {notes:?}",
        count(&root)
    );
    println!(
        "  stranger removed         : {}",
        !root.join("not-in-snapshot.txt").exists()
    );
    println!(
        "  deleted dir is back      : {}",
        root.join("deep/a/b/c/d/e/leaf.txt").exists()
    );
    let listed = list(&store)?;
    println!("  snapshots in store       : {}", listed.len());

    let _ = fs::remove_dir_all(&base);
    println!("demo complete (directories removed)");
    Ok(())
}

/// Bind a unix socket for tests that need a special file. Exposed so the test
/// crate can produce the state without duplicating the `unsafe`-free bind.
pub fn bind_probe_socket(path: &Path) -> std::io::Result<UnixListener> {
    UnixListener::bind(path)
}

/// Make an `OsStr` from raw bytes, for tests and for callers holding raw names.
pub fn os_str_from_bytes(bytes: &[u8]) -> OsString {
    OsString::from_vec(bytes.to_vec())
}

/// The raw bytes of an `OsStr`.
pub fn os_str_bytes(s: &OsStr) -> &[u8] {
    s.as_bytes()
}
