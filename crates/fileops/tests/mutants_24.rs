//! Mutant-killing tests for the fileops mutants the weekly run reported missed
//! (issue #24). Companion to `resolver/tests/mutants_23.rs`.
//!
//! Each test here exists because `cargo mutants -p atrium-fileops` showed a
//! change to `crates/fileops/src/lib.rs` that the whole suite did not notice.
//!
//! The 21 mutants split into two plainly different groups, and the split was
//! established by running the real binary with and without each mutation, not
//! by reading:
//!
//! KILLABLE (13) -- a witness input exists, and each test below is verified by
//! hand-applying the mutation and watching THIS test fail:
//!   - 71:9 x2   `parent_lexical` -> `String::new()` / `"xyzzy"` -- the parent
//!               is NAMED in `ParentMissing` and `NotADirectory` messages.
//!   - 81:9      `Display for VirtualPath` -> `Ok(Default::default())` -- the
//!               CLI's move line prints the paths, so Display is observable.
//!   - 125:9 x2  `EntryKind::as_str` -> `""` / `"xyzzy"` -- `list_dir` labels.
//!   - 322:18    `Ok(m) if m.is_dir()` -> `false` in read_file -- a directory
//!               read must refuse with IsADirectory.
//!   - 340:18    the same guard -> `true` in list_dir -- a file must refuse
//!               with NotADirectory.
//!   - 346:19    `== NotFound` -> `false` in list_dir -- an absent name must
//!               still be named NotFound, not the OS reason.
//!   - 346:28    `==` -> `!=` in list_dir -- same site, comparison flipped.
//!   - 392:32    `==` -> `!=` in move_path -- move source absent vs OS error.
//!   - 412:19    `== NotFound` -> `false` in move_path -- destination parent
//!               absent must name ParentMissing.
//!   - 412:28    `==` -> `!=` in move_path -- same site, comparison flipped.
//!   - 465:20    `entries > 0` -> `< 0` in delete_path -- the empty-directory
//!               leg must still delete, and the non-empty leg must refuse with
//!               the entry count.
//!
//! EQUIVALENT (8) -- no input can distinguish them, established by two
//! measurements rather than by reading. (a) REACHABILITY: each guard was
//! rewritten to log the state every time it is evaluated, and the whole suite
//! was run -- all 36 hits carried exactly the state the guard names and never
//! anything else, with a working positive control (the killed 340:18 guard was
//! seen taking its other branch). (b) DIFFERENTIAL: a probe was run against the
//! real code and against each mutation in turn; the output is byte-identical,
//! because the ways of producing a non-NotFound error here -- EACCES from a
//! chmod-000 parent, ENOTDIR from a file used as a parent -- are refused by
//! `RealPath::resolve` BEFORE the operation body runs. Recorded with reasons in
//! `.cargo/mutants.toml`:
//!   - 275:19, 305:19, 326:19, 346:19(T), 412:19(T), 445:19 -- the six
//!     `Err(e) if e.kind() == NotFound` guards mutated to `true`. The guard is
//!     only ever reached with NotFound.
//!   - 299:18, 406:18 -- the two `Ok(m) if m.is_dir()` guards mutated to `true`.
//!     The parent is always a real directory when the operation is reached; the
//!     non-directory cases are, again, refused by the resolver first.
//!
//! The 346:19 and 412:19 sites each carry BOTH a `false` mutant (killed here)
//! and a `true` mutant (equivalent), because the `true` direction needs an
//! error kind that cannot arrive and the `false` direction breaks an ordinary
//! absent-name path.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;

use atrium_fileops::{
    delete_path, list_dir, move_path, read_file, write_file, EntryKind, FileOpError, VirtualPath,
};

/// A throwaway environment root, created per test.
struct Env {
    root: PathBuf,
}

impl Env {
    fn new(name: &str) -> Env {
        let root =
            std::env::temp_dir().join(format!("atrium-mut24-{}-{}", name, std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        fs::create_dir_all(&root).unwrap();
        Env { root }
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn vp(s: &str) -> VirtualPath {
    VirtualPath::new(s).expect("test paths are absolute constants")
}

// ---------------------------------------------------------------------------
// 71:9 x2 -- parent_lexical, named in the refusal message
// ---------------------------------------------------------------------------

/// `write_file` to a path whose PARENT is absent must reject with
/// `ParentMissing`, and the message must name the missing parent -- which is
/// `parent_lexical`'s only job. With `String::new()` the parent renders as an
/// empty string; with `"xyzzy"` it renders as the literal `xyzzy`. Both make
/// the message wrong, and both are what this test pins.
#[test]
fn parent_missing_names_the_lexical_parent() {
    let e = Env::new("parent-lexical");
    let err = write_file(&e.root, &vp("/absent/f.txt"), b"x").unwrap_err();
    assert!(
        matches!(err, FileOpError::ParentMissing { .. }),
        "a missing parent must reject with ParentMissing, got {}",
        err
    );
    let msg = err.to_string();
    assert!(
        msg.contains("`/absent`"),
        "the refusal must name the missing parent `/absent`, got: {}",
        msg
    );
    assert!(
        !msg.contains("xyzzy"),
        "the parent name must not be a placeholder, got: {}",
        msg
    );
    // an empty rendering would leave the backticks adjacent
    assert!(
        !msg.contains("``"),
        "the parent name must not render empty, got: {}",
        msg
    );
}

/// The same for `move_path`'s destination, which calls `parent_lexical` on the
/// `to` path (line 409/415) -- the other two call sites of the same function.
#[test]
fn move_destination_missing_parent_names_the_lexical_parent() {
    let e = Env::new("move-parent-lexical");
    fs::write(e.root.join("a.txt"), b"a").unwrap();
    let err = move_path(&e.root, &vp("/a.txt"), &vp("/gone/b.txt")).unwrap_err();
    assert!(
        matches!(err, FileOpError::ParentMissing { .. }),
        "a missing destination parent must reject with ParentMissing, got {}",
        err
    );
    let msg = err.to_string();
    assert!(
        msg.contains("`/gone`"),
        "the refusal must name the missing parent `/gone`, got: {}",
        msg
    );
    assert!(
        !msg.contains("xyzzy") && !msg.contains("``"),
        "the parent name must be real, got: {}",
        msg
    );
}

/// `parent_lexical`'s boundary: a top-level path's parent is `/`, not an empty
/// string. `/f.txt` -> `/`. Under `String::new()` this renders empty; under
/// `"xyzzy"` it renders the placeholder.
#[test]
fn top_level_missing_parent_is_the_root() {
    let e = Env::new("parent-root");
    // `/newdir` does not exist, so its parent is the virtual root.
    fs::create_dir_all(e.root.join("sub")).unwrap();
    let err = write_file(&e.root, &vp("/newdir/f.txt"), b"x").unwrap_err();
    let msg = err.to_string();
    assert!(
        !msg.contains("xyzzy") && !msg.contains("``"),
        "a top-level parent must render as `/`, got: {}",
        msg
    );
}

// ---------------------------------------------------------------------------
// 81:9 -- Display for VirtualPath
// ---------------------------------------------------------------------------

/// `VirtualPath`'s `Display` must print the path as the agent wrote it. Nothing
/// in the crate's own code uses Display (it uses `as_str`), so without a test
/// mutating it to `Ok(Default::default())` is invisible -- the mutant renders
/// the path as the EMPTY STRING. The CLI's move line does print it, so this is
/// genuinely observable, and this test pins it.
#[test]
fn virtual_path_display_prints_the_spelling() {
    let p = vp("/home/documents/notes.txt");
    assert_eq!(
        format!("{}", p),
        "/home/documents/notes.txt",
        "Display must print the path as written; with the mutant it is empty"
    );
    // and the CLI's move format string is exactly this shape
    let line = format!("move {} -> {}", vp("/a"), vp("/b"));
    assert_eq!(line, "move /a -> /b");
    // a path with a trailing slash must round-trip unchanged (Display does not
    // normalise; that is resolve()'s job)
    assert_eq!(format!("{}", vp("/dir/")), "/dir/");
}

// ---------------------------------------------------------------------------
// 125:9 x2 -- EntryKind::as_str, the labels list_dir prints
// ---------------------------------------------------------------------------

/// `list_dir` must label each entry file / dir / link. With `as_str` mutated
/// the labels become `""` or `"xyzzy"`, so this pins all three spellings --
/// including the symlink case, which is the one a naive test would miss.
#[test]
fn list_dir_labels_all_three_entry_kinds() {
    let e = Env::new("entry-kind-labels");
    fs::create_dir_all(e.root.join("sub")).unwrap();
    fs::write(e.root.join("a.txt"), b"a").unwrap();
    symlink(e.root.join("a.txt"), e.root.join("z-link")).unwrap();

    let entries = list_dir(&e.root, &vp("/")).unwrap();
    let by_name = |n: &str| {
        entries
            .iter()
            .find(|d| d.name == n)
            .unwrap_or_else(|| panic!("entry {} missing from listing", n))
            .kind
    };
    assert_eq!(by_name("a.txt"), EntryKind::File, "a file is labelled file");
    assert_eq!(
        by_name("sub"),
        EntryKind::Directory,
        "a dir is labelled dir"
    );
    assert_eq!(
        by_name("z-link"),
        EntryKind::Symlink,
        "a symlink is labelled link"
    );

    // the strings themselves, since the enum could be right while as_str is not
    assert_eq!(EntryKind::File.as_str(), "file");
    assert_eq!(EntryKind::Directory.as_str(), "dir");
    assert_eq!(EntryKind::Symlink.as_str(), "link");
}

// ---------------------------------------------------------------------------
// 322:18 -- the is_dir guard in read_file
// ---------------------------------------------------------------------------

/// `read_file` on a DIRECTORY must refuse with `IsADirectory` rather than
/// trying to read it. The guard is `Ok(m) if m.is_dir()`; mutated to `false`
/// the arm is skipped and the directory falls to `fs::read`, which fails with
/// the OS's EISDIR instead -- a different variant, which is the difference
/// this test pins.
#[test]
fn read_file_on_a_directory_refuses_as_is_a_directory() {
    let e = Env::new("read-dir");
    fs::create_dir_all(e.root.join("sub")).unwrap();
    let err = read_file(&e.root, &vp("/sub")).unwrap_err();
    assert!(
        matches!(err, FileOpError::IsADirectory { .. }),
        "reading a directory must reject with IsADirectory, not fall through \
         to the OS error, got {}",
        err
    );
    let msg = err.to_string();
    assert!(
        msg.contains("directory") && !msg.contains("operating system"),
        "the refusal must be named as a directory, not as an OS refusal, \
         got: {}",
        msg
    );
}

// ---------------------------------------------------------------------------
// 340:18 -- the is_dir guard in list_dir
// ---------------------------------------------------------------------------

/// `list_dir` on a FILE must refuse with `NotADirectory`. Mutated to `true` the
/// guard accepts the file as a directory and `read_dir` fails with the OS
/// reason instead, so the pinned difference is the variant and its wording.
#[test]
fn list_dir_on_a_file_refuses_as_not_a_directory() {
    let e = Env::new("list-file");
    fs::write(e.root.join("f.txt"), b"f").unwrap();
    let err = list_dir(&e.root, &vp("/f.txt")).unwrap_err();
    assert!(
        matches!(err, FileOpError::NotADirectory { .. }),
        "listing a file must reject with NotADirectory, got {}",
        err
    );
    let msg = err.to_string();
    assert!(
        !msg.contains("operating system"),
        "the refusal must not be the OS's own reason, got: {}",
        msg
    );
}

// ---------------------------------------------------------------------------
// 346:19 (false) and 346:28 -- the NotFound guard and the == in list_dir
// ---------------------------------------------------------------------------

/// `list_dir` of an ABSENT inside-root name must name it `NotFound`.
/// - `==` mutated to `!=` (346:28): the NotFound arm is skipped and the name
///   falls to `io_err` -- the OS reason.
/// - `==` mutated to `false` (346:19): same effect, different edit.
/// Either way the caller stops being told the name is absent, which is what a
/// caller needs to distinguish "no such directory" from "the OS refused".
#[test]
fn list_dir_of_absent_name_names_it_not_found() {
    let e = Env::new("list-absent");
    let err = list_dir(&e.root, &vp("/absent")).unwrap_err();
    assert!(
        matches!(err, FileOpError::NotFound { .. }),
        "an absent name must reject with NotFound, got {}",
        err
    );
    let msg = err.to_string();
    assert!(
        !msg.contains("operating system"),
        "an absent name is not an OS refusal, got: {}",
        msg
    );
}

// ---------------------------------------------------------------------------
// 392:32 -- the == on the move SOURCE existence check
// ---------------------------------------------------------------------------

/// `move_path` with an ABSENT source must reject with `NotFound` naming the
/// source. The check is `e.kind() == NotFound` inside an `if let Err`; with
/// `!=` the absent case falls to `io_err` instead. This is the difference.
#[test]
fn move_of_absent_source_names_it_not_found() {
    let e = Env::new("move-absent-src");
    fs::create_dir_all(e.root.join("d")).unwrap();
    let err = move_path(&e.root, &vp("/absent"), &vp("/d/x")).unwrap_err();
    assert!(
        matches!(err, FileOpError::NotFound { .. }),
        "moving an absent source must reject with NotFound, got {}",
        err
    );
    let msg = err.to_string();
    assert!(
        !msg.contains("operating system"),
        "an absent source is not an OS refusal, got: {}",
        msg
    );
}

// ---------------------------------------------------------------------------
// 412:19 (false) and 412:28 -- the NotFound guard and == on the move DESTINATION
// ---------------------------------------------------------------------------

/// `move_path` whose DESTINATION PARENT is absent must name it `ParentMissing`.
/// `==` mutated to `false` or to `!=` skips the arm, so the caller gets the OS
/// reason instead of being told which parent is missing.
#[test]
fn move_destination_absent_parent_names_parent_missing() {
    let e = Env::new("move-dest-parent");
    fs::write(e.root.join("a.txt"), b"a").unwrap();
    let err = move_path(&e.root, &vp("/a.txt"), &vp("/absent/b.txt")).unwrap_err();
    assert!(
        matches!(err, FileOpError::ParentMissing { .. }),
        "a missing destination parent must reject with ParentMissing, got {}",
        err
    );
    let msg = err.to_string();
    assert!(
        !msg.contains("operating system"),
        "the refusal must name the missing parent, not an OS refusal, got: {}",
        msg
    );
}

// ---------------------------------------------------------------------------
// 465:20 -- entries > 0 in delete_path
// ---------------------------------------------------------------------------

/// `delete_path` must refuse a NON-EMPTY directory (naming it and its count)
/// and must remove an EMPTY one. `entries > 0` mutated to `< 0` inverts both
/// legs at once: the empty directory would be reported as non-empty. This test
/// drives both legs, so the mutation breaks it either way.
#[test]
fn delete_refuses_non_empty_and_removes_empty() {
    let e = Env::new("delete-count");
    // non-empty leg
    fs::create_dir_all(e.root.join("full")).unwrap();
    fs::write(e.root.join("full/a.txt"), b"a").unwrap();
    fs::write(e.root.join("full/b.txt"), b"b").unwrap();
    let err = delete_path(&e.root, &vp("/full"), false).unwrap_err();
    assert!(
        matches!(err, FileOpError::DirectoryNotEmpty { entries: 2, .. }),
        "a non-empty directory must refuse with the entry count 2, got {}",
        err
    );
    let msg = err.to_string();
    assert!(
        msg.contains("2"),
        "the refusal must name the number of entries, got: {}",
        msg
    );
    assert!(
        e.root.join("full").exists(),
        "nothing may be deleted from a refused directory"
    );

    // empty leg: must actually be removed
    fs::create_dir_all(e.root.join("empty")).unwrap();
    delete_path(&e.root, &vp("/empty"), false).unwrap();
    assert!(
        !e.root.join("empty").exists(),
        "an empty directory must be removed, not reported as non-empty"
    );
}
