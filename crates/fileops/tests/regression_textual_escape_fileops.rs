//! Regression tests: the file operations must refuse the textual-mode escape,
//! and must leave the outside file byte-identical when they do.
//!
//! The resolver bug (18 Sep 2026) accepted a path like `/nope/../café` where
//! `café` is a symlink pointing out of the root. `fileops` relies on the
//! resolver as its single doorway — `RealPath`'s constructor is private, so
//! every operation resolves before touching the disk — which means that bug
//! was reachable through `read_file`, `write_file` and `delete_path`, not just
//! through `resolve()`.
//!
//! These tests drive the operations, not the resolver, because that is the
//! layer a user's files would actually be lost through. Each one checks two
//! things: the operation is refused, and the outside file is unchanged
//! afterwards. A refusal is not enough on its own — an operation that writes
//! and then refuses has still destroyed the file.
//!
//! Nothing here edits an existing test file.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use atrium_fileops::{delete_path, read_file, write_file, FileOpError, VirtualPath};

const OUTSIDE_CONTENT: &[u8] = b"TOP-SECRET-OUTSIDE-CONTENT-MUST-NOT-CHANGE";

struct Fixture {
    dir: PathBuf,
    root: PathBuf,
    outside_file: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Fixture {
        let dir = std::env::temp_dir().join(format!(
            "atrium-fileops-esc-{}-{}",
            name,
            std::process::id()
        ));
        if dir.exists() {
            fs::remove_dir_all(&dir).unwrap();
        }
        let root = dir.join("root");
        let outside = dir.join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();

        let outside_file = outside.join("secret.txt");
        fs::write(&outside_file, OUTSIDE_CONTENT).unwrap();

        // The same two escaping symlinks the resolver regression uses, plus a
        // directory link so a child path can be aimed outside the root.
        symlink(&outside_file, root.join("caf\u{e9}")).unwrap();
        symlink(Path::new("../outside/secret.txt"), root.join("jI876")).unwrap();
        symlink(&outside, root.join("dirlink")).unwrap();

        // A legal file inside the root, so the controls can prove the
        // operations still work at all.
        fs::create_dir_all(root.join("real")).unwrap();
        fs::write(root.join("real/notes.txt"), b"ok\n").unwrap();

        Fixture {
            dir,
            root,
            outside_file,
        }
    }

    /// The exact bytes of the outside file right now.
    fn outside_bytes(&self) -> Vec<u8> {
        fs::read(&self.outside_file).unwrap()
    }

    /// Assert the result is a refusal AND the outside file still holds
    /// `before` — the bytes captured BEFORE the operation ran. Reporting the
    /// file state first matters: an operation that overwrites and then
    /// returns an error has still destroyed the file, and a test that only
    /// looks at the Result would call that a pass.
    fn assert_refused_and_untouched<T: std::fmt::Debug>(
        &self,
        operation: &str,
        virtual_path: &str,
        before: &[u8],
        result: Result<T, FileOpError>,
    ) {
        let after = self.outside_bytes();
        let damaged = after != before;

        match result {
            Err(e) => {
                assert!(
                    !damaged,
                    "{} REFUSED `{}` but the outside file changed anyway: {} -> {}",
                    operation,
                    virtual_path,
                    String::from_utf8_lossy(before),
                    String::from_utf8_lossy(&after)
                );
                let reason = e.to_string();
                assert!(
                    !reason.contains("does not exist inside the environment"),
                    "{} refused `{}` by citing ABSENCE rather than the escape: {}",
                    operation,
                    virtual_path,
                    reason
                );
            }
            Ok(v) => panic!(
                "{} ACCEPTED `{}` and returned {:?}. Outside file: {} ({} bytes, {})",
                operation,
                virtual_path,
                v,
                self.outside_file.display(),
                after.len(),
                if damaged {
                    format!(
                        "DAMAGED: was {:?}, now {:?}",
                        String::from_utf8_lossy(before),
                        String::from_utf8_lossy(&after)
                    )
                } else {
                    "unchanged".to_string()
                }
            ),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// The paths that must not reach outside content. The same shape at the end
/// of the path and in the middle of it.
const ESCAPE_PATHS: &[&str] = &[
    "/nope/../caf\u{e9}",
    "/_/..//jI876",
    "/_///../caf\u{e9}",
    "/\u{1f680}/..//caf\u{e9}",
    "/\u{65e5}\u{672c}\u{8a9e}/..//caf\u{e9}",
    "/nope/../dirlink/secret.txt",
    "/nope/../jI876/child.txt",
];

// ---------------------------------------------------------------------------
// read
// ---------------------------------------------------------------------------

#[test]
fn read_through_the_escape_is_refused_and_reads_nothing() {
    let f = Fixture::new("read");
    for path in ESCAPE_PATHS {
        let before = f.outside_bytes();
        let vp = VirtualPath::new(path).unwrap();
        let result = read_file(&f.root, &vp);
        f.assert_refused_and_untouched("read_file", path, &before, result);
    }
}

/// The sharpest version of the read: prove the secret bytes were never
/// returned, whatever the error was.
#[test]
fn read_never_returns_the_outside_content() {
    let f = Fixture::new("read-content");
    for path in ESCAPE_PATHS {
        let vp = VirtualPath::new(path).unwrap();
        if let Ok(bytes) = read_file(&f.root, &vp) {
            assert_ne!(
                bytes, OUTSIDE_CONTENT,
                "read_file `{}` returned the outside file's contents",
                path
            );
            panic!(
                "read_file `{}` succeeded and returned {} bytes",
                path,
                bytes.len()
            );
        }
    }
}

// ---------------------------------------------------------------------------
// write
// ---------------------------------------------------------------------------

#[test]
fn write_through_the_escape_is_refused_and_the_outside_file_is_byte_identical() {
    let f = Fixture::new("write");
    let before = f.outside_bytes();
    for path in ESCAPE_PATHS {
        // Capture immediately before THIS attempt: a successful write would
        // change the bytes, and the next iteration must see the new truth.
        let at_attempt = f.outside_bytes();
        let vp = VirtualPath::new(path).unwrap();
        let result = write_file(&f.root, &vp, b"OVERWRITTEN-BY-THE-SANDBOX");
        f.assert_refused_and_untouched("write_file", path, &at_attempt, result);
    }
    let after = f.outside_bytes();
    assert_eq!(
        before,
        after,
        "the outside file changed; it began with {:?}",
        String::from_utf8_lossy(&before)
    );
    // Name it explicitly: the content must still be exactly the original.
    assert_eq!(after, OUTSIDE_CONTENT);
}

// ---------------------------------------------------------------------------
// delete
// ---------------------------------------------------------------------------

#[test]
fn delete_through_the_escape_is_refused_and_the_outside_file_still_exists() {
    let f = Fixture::new("delete");
    let before = f.outside_bytes();
    for path in ESCAPE_PATHS {
        let at_attempt = f.outside_bytes();
        let vp = VirtualPath::new(path).unwrap();
        let result = delete_path(&f.root, &vp, false);
        f.assert_refused_and_untouched("delete_path", path, &at_attempt, result);
        assert!(
            f.outside_file.exists(),
            "delete_path `{}` removed the outside file",
            path
        );
    }
    assert_eq!(f.outside_bytes(), before);
}

#[test]
fn recursive_delete_through_the_escape_is_refused_and_the_outside_dir_survives() {
    let f = Fixture::new("delete-recursive");
    let outside_dir = f.outside_file.parent().unwrap().to_path_buf();
    for path in ESCAPE_PATHS {
        let at_attempt = if f.outside_file.exists() {
            f.outside_bytes()
        } else {
            Vec::new()
        };
        let vp = VirtualPath::new(path).unwrap();
        let result = delete_path(&f.root, &vp, true);
        if f.outside_file.exists() {
            f.assert_refused_and_untouched("delete_path(recursive)", path, &at_attempt, result);
        } else {
            panic!(
                "recursive delete `{}` removed the outside file {}",
                path,
                f.outside_file.display()
            );
        }
        assert!(
            outside_dir.exists(),
            "recursive delete `{}` removed the outside directory",
            path
        );
        assert!(
            f.outside_file.exists(),
            "recursive delete `{}` removed the outside file",
            path
        );
    }
    assert_eq!(f.outside_bytes(), OUTSIDE_CONTENT);
}

// ---------------------------------------------------------------------------
// Controls: the fix must not break the operations on legal paths.
// ---------------------------------------------------------------------------

#[test]
fn legal_operations_still_work() {
    let f = Fixture::new("controls");
    let readable = VirtualPath::new("/real/notes.txt").unwrap();
    assert_eq!(read_file(&f.root, &readable).unwrap(), b"ok\n");

    let new_file = VirtualPath::new("/real/created.txt").unwrap();
    write_file(&f.root, &new_file, b"hello").unwrap();
    assert_eq!(read_file(&f.root, &new_file).unwrap(), b"hello");

    delete_path(&f.root, &new_file, false).unwrap();
    assert!(!f.root.join("real/created.txt").exists());

    // And an absent-inside path is still refused by the operation (not by the
    // resolver) with the operation's own reason.
    let absent = VirtualPath::new("/real/absent.txt").unwrap();
    match read_file(&f.root, &absent) {
        Err(FileOpError::NotFound { .. }) => {}
        other => panic!(
            "expected NotFound for an absent inside path, got {:?}",
            other
        ),
    }
}

#[test]
fn the_outside_file_is_untouched_by_every_legal_operation_too() {
    let f = Fixture::new("controls-outside");
    let before = f.outside_bytes();
    let vp = VirtualPath::new("/real/scratch.txt").unwrap();
    write_file(&f.root, &vp, b"scratch").unwrap();
    let _ = read_file(&f.root, &vp);
    let _ = delete_path(&f.root, &vp, false);
    assert_eq!(f.outside_bytes(), before);
}
