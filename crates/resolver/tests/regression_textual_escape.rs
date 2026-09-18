//! Regression tests for the textual-mode sandbox escape (18 Sep 2026).
//!
//! Every path here is one the property test `resolver_props.rs` found, or the
//! same shape with the escaping symlink in the middle of the path rather than
//! at the end. On an unfixed resolver all of them are ACCEPTED and every one
//! of them lands outside the environment root.
//!
//! The shape: a component that does not exist, then `..`, then the name of a
//! symlink whose target is outside the root. The non-existent component does
//! not need to be unusual — `/nope/../café` is the same bug as
//! `/日本語/..//café`.
//!
//! These tests assert the required verdict (`Err`). They are deliberately
//! explicit rather than generated: the generated version is
//! `resolver_props.rs`, and a regression suite should fail loudly and
//! legibly on the day someone reintroduces this.
//!
//! Nothing here edits an existing test file.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use atrium_resolver::resolve;

/// A root containing symlinks that point outside it, with the outside
/// directory and its file really existing — so an escape is an escape onto
/// real content, not onto a path that happens to be missing.
struct Fixture {
    dir: PathBuf,
    root: PathBuf,
    outside: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Fixture {
        let dir =
            std::env::temp_dir().join(format!("atrium-regress-{}-{}", name, std::process::id()));
        if dir.exists() {
            fs::remove_dir_all(&dir).unwrap();
        }
        let root = dir.join("root");
        let outside = dir.join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();

        // The content an escape would reach.
        fs::write(outside.join("secret.txt"), b"TOP-SECRET-OUTSIDE-CONTENT").unwrap();

        // An escaping symlink whose target really exists (absolute target).
        symlink(outside.join("secret.txt"), root.join("caf\u{e9}")).unwrap();
        // The same, reached by a relative target: one level up, then across.
        symlink(Path::new("../outside/secret.txt"), root.join("jI876")).unwrap();
        // A directory outside the root, for the middle-of-path case.
        symlink(&outside, root.join("dirlink")).unwrap();
        // A directory that does exist inside the root, as the control: the
        // same path shape pointing at something legal must still be accepted.
        fs::create_dir_all(root.join("real")).unwrap();
        fs::write(root.join("real/notes.txt"), b"ok\n").unwrap();

        Fixture { dir, root, outside }
    }

    /// Assert the path is refused, and that the refusal is not a lie: if it
    /// had been accepted, the returned location would have escaped.
    fn assert_refused(&self, virtual_path: &str) {
        match resolve(&self.root, virtual_path) {
            Err(e) => {
                // A refusal that names absence would be the wrong refusal for
                // an escape case, so make that visible in the failure output.
                let reason = e.to_string();
                assert!(
                    !reason.contains("does not exist inside the environment root"),
                    "{} was refused, but by citing ABSENCE rather than the escape: {}",
                    virtual_path,
                    reason
                );
            }
            Ok(real) => {
                let canonical = fs::canonicalize(&real).unwrap_or_else(|_| real.clone());
                panic!(
                    "ESCAPE: `{}` was ACCEPTED as {} — canonicalised {} — the canonical root is {}",
                    virtual_path,
                    real.display(),
                    canonical.display(),
                    fs::canonicalize(&self.root).unwrap().display()
                );
            }
        }
    }

    /// Assert the path is accepted and the result really is inside the root.
    fn assert_accepted_inside(&self, virtual_path: &str) {
        match resolve(&self.root, virtual_path) {
            Ok(real) => {
                let canon_root = fs::canonicalize(&self.root).unwrap();
                let canonical = fs::canonicalize(&real).unwrap_or_else(|_| real.clone());
                assert!(
                    canonical.starts_with(&canon_root),
                    "`{}` was accepted as {} but canonicalises to {}, outside the root {}",
                    virtual_path,
                    real.display(),
                    canonical.display(),
                    canon_root.display()
                );
            }
            Err(e) => panic!("`{}` should be accepted, was refused: {}", virtual_path, e),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

// ---------------------------------------------------------------------------
// The shrunk cases, one test each. These are the exact strings the property
// test reported, in the order it reported them.
// ---------------------------------------------------------------------------

#[test]
fn shrunk_case_nope_then_cafe_is_refused() {
    let f = Fixture::new("nope-cafe");
    f.assert_refused("/nope/../caf\u{e9}");
}

#[test]
fn shrunk_case_underscore_then_j_i_876_is_refused() {
    let f = Fixture::new("underscore-jI876");
    f.assert_refused("/_/..//jI876");
}

#[test]
fn shrunk_case_underscore_triple_slash_then_cafe_is_refused() {
    let f = Fixture::new("underscore-slashes-cafe");
    f.assert_refused("/_///../caf\u{e9}");
}

#[test]
fn shrunk_case_double_slash_underscore_cafe_etc_is_refused() {
    let f = Fixture::new("slashes-cafe-etc");
    f.assert_refused("//_//../caf\u{e9}/etc");
}

#[test]
fn shrunk_case_rocket_dot_cafe_is_refused() {
    let f = Fixture::new("rocket-cafe");
    f.assert_refused("/\u{1f680}/..//caf\u{e9}/./caf\u{e9}");
}

/// The case the property test shrank to in its saved regressions file, kept
/// verbatim so a future reader can match the two.
#[test]
fn shrunk_case_saved_regression_is_refused() {
    let f = Fixture::new("saved-regression");
    f.assert_refused("/\u{65e5}\u{672c}\u{8a9e}/..//caf\u{e9}");
}

// ---------------------------------------------------------------------------
// The same shape, unicode-free. The bug is not about non-ASCII names; it was
// simply found by a generator that produced them.
// ---------------------------------------------------------------------------

#[test]
fn ascii_absent_then_dotdot_then_escaping_link_is_refused() {
    let f = Fixture::new("ascii");
    f.assert_refused("/nope/../jI876");
    f.assert_refused("/x/../caf\u{e9}");
    f.assert_refused("/x/y/../../caf\u{e9}");
}

// ---------------------------------------------------------------------------
// The escaping symlink in the MIDDLE of the path, not at the end. The end
// position was where the property test happened to land; nothing about the
// mechanism is specific to it.
// ---------------------------------------------------------------------------

#[test]
fn absent_then_dotdot_then_escaping_dir_link_in_the_middle_is_refused() {
    let f = Fixture::new("middle-dirlink");
    // dirlink is a directory outside the root; the target file is a child.
    f.assert_refused("/nope/../dirlink/secret.txt");
    f.assert_refused("/_/..//dirlink/secret.txt");
    f.assert_refused("/\u{65e5}\u{672c}\u{8a9e}/../dirlink/secret.txt");
}

#[test]
fn absent_then_dotdot_then_escaping_file_link_in_the_middle_is_refused() {
    let f = Fixture::new("middle-filelink");
    // A file symlink with a trailing component: still must not be accepted.
    f.assert_refused("/nope/../caf\u{e9}/child");
    f.assert_refused("/nope/../jI876/child");
}

#[test]
fn absent_run_then_dotdots_then_escaping_link_is_refused() {
    let f = Fixture::new("absent-run");
    f.assert_refused("/a/b/c/../../../caf\u{e9}");
    f.assert_refused("/a/b/c/../../../../caf\u{e9}");
    f.assert_refused("/nope/deeper/still/../../../caf\u{e9}");
}

// ---------------------------------------------------------------------------
// Controls. The fix must not turn every absent path into a refusal, and the
// same shapes pointing at something legal must still be accepted.
// ---------------------------------------------------------------------------

#[test]
fn the_same_shapes_pointing_inside_are_still_accepted() {
    let f = Fixture::new("controls-inside");
    // Absent then `..`: back to the root, then a name that does not exist.
    f.assert_accepted_inside("/nope/../newfile");
    f.assert_accepted_inside("/nope/..");
    f.assert_accepted_inside("/_/..//newfile");
    f.assert_accepted_inside("/\u{65e5}\u{672c}\u{8a9e}/..//newfile");
    // Absent then `..` then a real directory inside the root.
    f.assert_accepted_inside("/nope/../real/notes.txt");
    // Plain absent paths, no traversal at all.
    f.assert_accepted_inside("/newfile");
    f.assert_accepted_inside("/newdir/newfile");
}

#[test]
fn the_escaping_link_alone_is_refused_and_stays_refused() {
    let f = Fixture::new("link-alone");
    f.assert_refused("/caf\u{e9}");
    f.assert_refused("/jI876");
    f.assert_refused("/caf\u{e9}/..");
    f.assert_refused("/dirlink/secret.txt");
}

/// The outside file must be untouched by any of this: resolution only reads.
#[test]
fn resolving_never_touches_the_outside_file() {
    let f = Fixture::new("no-write");
    let before = fs::read(f.outside.join("secret.txt")).unwrap();
    let _ = resolve(&f.root, "/nope/../caf\u{e9}");
    let _ = resolve(&f.root, "/nope/../dirlink/secret.txt");
    let after = fs::read(f.outside.join("secret.txt")).unwrap();
    assert_eq!(before, after, "resolution modified the outside file");
}
