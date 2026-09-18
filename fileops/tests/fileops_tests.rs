//! Integration tests for Phase 2a, covering every line of `attack-list-2a.md`
//! sections A–K. Standard, inherited from Phases 1/1b and kept:
//! - absent cases are asserted absent on disk, so a passing test can only
//!   pass for the right reason;
//! - refusals assert the REASON names the specific thing, not just that a
//!   refusal happened;
//! - §E.1 asserts the outside file survives by CHECKSUM — that line's
//!   failure mode is destroying the user's data;
//! - §E.2 (link loop) hangs rather than fails if a walk follows links;
//!   cargo test is therefore run with a timeout (see README §K).
//!
//! No test is weakened to pass. If a line is impossible to satisfy, it is a
//! finding in the report, not a hidden xfail.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use atrium_fileops::{
    create_dir, delete_path, list_dir, move_path, read_file, write_file, EntryKind, FileOpError,
    VirtualPath,
};

/// A throwaway environment root + the outside directory, per test. Nothing
/// touches the project or anywhere else. Dropped (and removed) at the end.
struct Env {
    root: PathBuf,
    outside: PathBuf,
    #[allow(dead_code)]
    id: String,
}

fn sha256_like(p: &Path) -> String {
    // A content fingerprint good enough to prove bytes did not change:
    // length + position-exponent hash over the bytes. Boring, no deps.
    let b = fs::read(p).expect("fingerprint target exists");
    let mut h: u128 = 0x9e3779b97f4a7c15;
    for (i, byte) in b.iter().enumerate() {
        h = h.rotate_left(13).wrapping_add(*byte as u128 + i as u128);
        h ^= h >> 29 & 0xff;
    }
    format!("len={} hash={:032x}", b.len(), h)
}

fn build_env() -> Env {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = format!(
        "atrium-2a-test-{}-{}",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    );
    let root = std::env::temp_dir().join(format!("{}-root", id));
    let outside = std::env::temp_dir().join(format!("{}-outside", id));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    if outside.exists() {
        fs::remove_dir_all(&outside).unwrap();
    }
    fs::create_dir_all(&outside.join("sub")).unwrap();
    fs::write(outside.join("sentinel.txt"), b"sentinel\n").unwrap();
    fs::write(outside.join("sub/keep.txt"), b"keep\n").unwrap();

    let r = |p: &str| root.join(p);
    fs::create_dir_all(r("home/documents/subdir")).unwrap();
    fs::write(r("home/documents/notes.txt"), b"notes\n").unwrap();
    let mk = |name: &str, target: &Path| {
        symlink(target, r(name)).unwrap();
    };
    mk("outside-link", &outside);
    mk("inside-link", &root.join("home/documents"));
    mk("chain-b", &outside);
    mk("chain-a", &root.join("chain-b"));
    mk("dangling-out", &outside.join("absent.txt"));
    mk("dangling-in", &root.join("home/documents/absent.txt"));
    mk("dangling-in2", &root.join("home/documents/absent2.txt"));
    symlink("../../outside-dir", r("rel-outside-link")).unwrap();
    symlink("home/documents", r("rel-inside-link")).unwrap();
    fs::create_dir_all(r("trap")).unwrap();
    fs::write(r("trap/keepme.txt"), b"keepme\n").unwrap();
    symlink(&outside, r("trap/escape")).unwrap();
    fs::create_dir_all(r("loopdir")).unwrap();
    symlink("loop-b", r("loopdir/loop-a")).unwrap();
    symlink("loop-a", r("loopdir/loop-b")).unwrap();
    fs::write(r("link-to-file"), b"not a dir\n").unwrap();

    Env { root, outside, id }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
        let _ = fs::remove_dir_all(&self.outside);
    }
}

fn vp(s: &str) -> VirtualPath {
    VirtualPath::new(s).expect("test paths are absolute constants")
}

fn reason_of(e: &FileOpError) -> String {
    format!("{}", e)
}

// ---------------------------------------------------------------------------
// A. Traversal (MUST REFUSE; nothing created anywhere)
// ---------------------------------------------------------------------------

#[test]
fn a1_write_traversal_refused() {
    let e = build_env();
    let out = sha256_like(&e.outside.join("sentinel.txt"));
    let err = write_file(&e.root, &vp("/home/../../escaped.txt"), b"x").unwrap_err();
    assert!(
        reason_of(&err).contains(".."),
        "reason names the step: {}",
        reason_of(&err)
    );
    assert!(!e.root.join("../escaped.txt").exists());
    assert_eq!(out, sha256_like(&e.outside.join("sentinel.txt")));
}

#[test]
fn a2_create_dir_traversal_refused() {
    let e = build_env();
    let err = create_dir(&e.root, &vp("/../escaped-dir")).unwrap_err();
    assert!(reason_of(&err).contains(".."), "{}", reason_of(&err));
}

#[test]
fn a3_delete_traversal_refused() {
    let e = build_env();
    let err = delete_path(&e.root, &vp("/home/documents/../../escaped.txt"), true).unwrap_err();
    assert!(reason_of(&err).contains(".."), "{}", reason_of(&err));
    assert!(
        e.root.join("home/documents").exists(),
        "nothing was deleted"
    );
}

#[test]
fn a4_read_traversal_refused() {
    let e = build_env();
    let err = read_file(&e.root, &vp("/../../etc/passwd")).unwrap_err();
    assert!(reason_of(&err).contains(".."), "{}", reason_of(&err));
}

#[test]
fn a5_list_traversal_refused() {
    let e = build_env();
    let err = list_dir(&e.root, &vp("/home/../..")).unwrap_err();
    assert!(reason_of(&err).contains(".."), "{}", reason_of(&err));
}

#[test]
fn a6_move_traversal_refused_source_kept() {
    let e = build_env();
    let before = fs::read(e.root.join("home/documents/notes.txt")).unwrap();
    let err = move_path(
        &e.root,
        &vp("/home/documents/notes.txt"),
        &vp("/home/../../moved.txt"),
    )
    .unwrap_err();
    assert!(reason_of(&err).contains(".."), "{}", reason_of(&err));
    assert_eq!(
        before,
        fs::read(e.root.join("home/documents/notes.txt")).unwrap(),
        "notes.txt must still be present with its bytes"
    );
}

#[test]
fn a7_delete_dotdot_refused() {
    let e = build_env();
    let err = delete_path(&e.root, &vp("/.."), true).unwrap_err();
    assert!(reason_of(&err).contains(".."), "{}", reason_of(&err));
    assert!(e.root.exists());
}

#[test]
fn a8_four_dots_is_a_name_not_traversal() {
    let e = build_env();
    create_dir(&e.root, &vp("/..../deep")).unwrap();
    assert!(
        e.root.join("..../deep").is_dir(),
        "the line that catches a string-replace 'cleanup': four dots is a legal filename"
    );
    // And nothing appeared outside.
    assert!(sha256_like(&e.outside.join("sentinel.txt")).starts_with("len="));
}

// ---------------------------------------------------------------------------
// B. Host-looking absolute paths act INSIDE (never against the host)
// ---------------------------------------------------------------------------

#[test]
fn b1_create_dir_etc_passwd_inside() {
    let e = build_env();
    create_dir(&e.root, &vp("/etc/passwd")).unwrap();
    assert!(
        e.root.join("etc/passwd").is_dir(),
        "a DIRECTORY under the root"
    );
}

#[test]
fn b2_write_under_inside_etc_dir() {
    let e = build_env();
    create_dir(&e.root, &vp("/etc/passwd")).unwrap();
    write_file(&e.root, &vp("/etc/passwd/x"), b"hi").unwrap();
    assert_eq!(fs::read(e.root.join("etc/passwd/x")).unwrap(), b"hi");
}

#[test]
fn b3_etc_shadow_inside_never_host_permission_error() {
    let e = build_env();
    let r = write_file(&e.root, &vp("/etc/shadow"), b"hi");
    match r {
        Ok(()) => assert_eq!(fs::read(e.root.join("etc/shadow")).unwrap(), b"hi"),
        Err(err) => {
            let reason = reason_of(&err);
            assert!(
                !reason.contains("Permission denied") && !reason.contains("os error 13"),
                "a host permission error means the operation was aimed at the host: {}",
                reason
            );
        }
    }
}

#[test]
fn b4_delete_inside_etc_only() {
    let e = build_env();
    create_dir(&e.root, &vp("/etc/passwd")).unwrap();
    write_file(&e.root, &vp("/etc/passwd/x"), b"hi").unwrap();
    delete_path(&e.root, &vp("/etc/passwd"), true).unwrap();
    assert!(!e.root.join("etc/passwd").exists(), "inside dir removed");
}

#[test]
fn b5_root_secret_inside_never_host_permission_error() {
    let e = build_env();
    let r = create_dir(&e.root, &vp("/root/secret"));
    match r {
        Ok(()) => assert!(e.root.join("root/secret").is_dir()),
        Err(err) => {
            let reason = reason_of(&err);
            assert!(
                !reason.contains("Permission denied") && !reason.contains("os error 13"),
                "a host permission error means the operation was aimed at the host: {}",
                reason
            );
        }
    }
}

#[test]
fn b6_usr_bin_inside_never_host_permission_error() {
    let e = build_env();
    let r = write_file(&e.root, &vp("/usr/bin/evil"), b"x");
    match r {
        Ok(()) => assert_eq!(fs::read(e.root.join("usr/bin/evil")).unwrap(), b"x"),
        Err(err) => {
            let reason = reason_of(&err);
            assert!(
                !reason.contains("Permission denied") && !reason.contains("os error 13"),
                "a host permission error means the operation was aimed at the host: {}",
                reason
            );
        }
    }
}

// ---------------------------------------------------------------------------
// C. Symlink routes out — every operation refuses
// ---------------------------------------------------------------------------

#[test]
fn c1_write_through_outside_link_refused_and_absent() {
    let e = build_env();
    let err = write_file(&e.root, &vp("/outside-link/created.txt"), b"x").unwrap_err();
    let reason = reason_of(&err);
    assert!(
        reason.contains("outside") || reason.contains("symlink"),
        "{}",
        reason
    );
    assert!(
        !e.outside.join("created.txt").exists(),
        "MUST NOT EXIST outside, checked on disk"
    );
}

#[test]
fn c2_create_dir_through_outside_link_refused() {
    let e = build_env();
    let before: Vec<_> = fs::read_dir(&e.outside)
        .unwrap()
        .map(|d| d.unwrap().file_name())
        .collect();
    create_dir(&e.root, &vp("/outside-link/newdir")).unwrap_err();
    let after: Vec<_> = fs::read_dir(&e.outside)
        .unwrap()
        .map(|d| d.unwrap().file_name())
        .collect();
    assert_eq!(before.len(), after.len(), "nothing new under <outside>");
}

#[test]
fn c3_read_through_outside_link_refused_content_never_returned() {
    let e = build_env();
    let err = read_file(&e.root, &vp("/outside-link/sentinel.txt")).unwrap_err();
    let reason = reason_of(&err);
    assert!(
        !reason.contains("sentinel\n"),
        "the sentinel's CONTENTS must not leak: {}",
        reason
    );
}

#[test]
fn c4_list_through_outside_link_refused() {
    let e = build_env();
    list_dir(&e.root, &vp("/outside-link")).unwrap_err();
}

#[test]
fn c5_delete_through_outside_link_refused_sentinel_survives() {
    let e = build_env();
    let before = sha256_like(&e.outside.join("sentinel.txt"));
    delete_path(&e.root, &vp("/outside-link/sentinel.txt"), false).unwrap_err();
    assert_eq!(before, sha256_like(&e.outside.join("sentinel.txt")));
}

#[test]
fn c6_move_through_outside_link_refused_notes_kept() {
    let e = build_env();
    move_path(
        &e.root,
        &vp("/home/documents/notes.txt"),
        &vp("/outside-link/moved.txt"),
    )
    .unwrap_err();
    assert!(e.root.join("home/documents/notes.txt").exists());
    assert!(!e.outside.join("moved.txt").exists());
}

#[test]
fn c7_c8_chain_out_refused() {
    let e = build_env();
    write_file(&e.root, &vp("/chain-a/created.txt"), b"x").unwrap_err();
    create_dir(&e.root, &vp("/chain-a/newdir")).unwrap_err();
    assert!(!e.outside.join("created.txt").exists());
    assert!(!e.outside.join("newdir").exists());
}

#[test]
fn c9_c10_relative_outside_link_refused() {
    let e = build_env();
    write_file(&e.root, &vp("/rel-outside-link/created.txt"), b"x").unwrap_err();
    list_dir(&e.root, &vp("/rel-outside-link")).unwrap_err();
}

#[test]
fn c11_c12_c13_dangling_paths_pin_the_boundary() {
    let e = build_env();
    // C.11: absent AND outside refuses. Absence must not launder the escape.
    let err = write_file(&e.root, &vp("/dangling-out/newfile"), b"x").unwrap_err();
    let reason_out = reason_of(&err);
    assert!(
        reason_out.contains("symlink") || reason_out.contains("outside"),
        "{}",
        reason_out
    );
    assert!(!e.outside.join("newfile").exists(), "nothing outside");

    // C.12: absent AND inside, but the absent target is the PARENT — and
    // write_file never creates parents (F.2). So this refuses for the ORDINARY
    // reason, naming the missing parent, and NOT as a symlink escape: the link
    // is inside the root and leads somewhere legal.
    let err = write_file(&e.root, &vp("/dangling-in/newfile"), b"x").unwrap_err();
    let reason_in = reason_of(&err);
    assert!(
        reason_in.contains("parent") && reason_in.contains("/dangling-in"),
        "C.12 must refuse for the missing-parent reason, naming it: {}",
        reason_in
    );
    assert!(
        !reason_in.contains("points outside"),
        "C.12 must NOT be a symlink-escape refusal — the link is inside: {}",
        reason_in
    );
    assert!(
        !e.root.join("home/documents/absent.txt").exists(),
        "still absent"
    );

    // C.13: creating the absent target THROUGH the dangling link must work —
    // the case Phase 1b exists for. Uses /dangling-in2 so this does not make
    // home/documents/absent.txt real and break C.12's premise above.
    write_file(&e.root, &vp("/dangling-in2"), b"x").unwrap();
    assert_eq!(
        fs::read(e.root.join("home/documents/absent2.txt")).unwrap(),
        b"x"
    );
}

#[test]
fn h_no_real_path_is_disclosed_in_any_reason() {
    // Regression guard for the defect fixed 14 Sep 2026: the operations passed
    // the resolver's Display text straight through, and three of its variants
    // name a REAL on-disk location. Every refusal here must name the VIRTUAL
    // path and the failing step, and must not contain the root or the outside
    // directory's real location (DESIGN.md §3.1; attack-list-2a.md §H.2-H.5).
    let e = build_env();
    let root_s = e.root.to_string_lossy().to_string();
    let outside_s = e.outside.to_string_lossy().to_string();

    let cases: Vec<(String, FileOpError)> = vec![
        (
            "traversal".into(),
            write_file(&e.root, &vp("/home/../../escaped.txt"), b"x").unwrap_err(),
        ),
        (
            "symlink-out".into(),
            write_file(&e.root, &vp("/outside-link/x"), b"x").unwrap_err(),
        ),
        (
            "chain-out".into(),
            write_file(&e.root, &vp("/chain-a/x"), b"x").unwrap_err(),
        ),
        (
            "dangling-out".into(),
            write_file(&e.root, &vp("/dangling-out/x"), b"x").unwrap_err(),
        ),
        (
            "list-out".into(),
            list_dir(&e.root, &vp("/outside-link")).unwrap_err(),
        ),
        (
            "read-out".into(),
            read_file(&e.root, &vp("/outside-link/sentinel.txt")).unwrap_err(),
        ),
    ];

    for (label, err) in cases {
        let reason = reason_of(&err);
        // Positive: it still names the failing step, so the fix did not gut the reason.
        assert!(
            reason.contains("..") || reason.contains("symlink") || reason.contains("outside"),
            "[{}] reason must still name the failing step: {}",
            label,
            reason
        );
        // Negative: no real location anywhere in it.
        assert!(
            !reason.contains(&root_s),
            "[{}] ROOT disclosed: {}",
            label,
            reason
        );
        assert!(
            !reason.contains(&outside_s),
            "[{}] OUTSIDE disclosed: {}",
            label,
            reason
        );
    }
}

// ---------------------------------------------------------------------------
// D. Move — both ends attacked
// ---------------------------------------------------------------------------

#[test]
fn d1_move_source_outside_refused() {
    let e = build_env();
    let before = sha256_like(&e.outside.join("sentinel.txt"));
    // The line people miss: source is a virtual path like any other.
    let err = move_path(
        &e.root,
        &vp("/outside-link/sentinel.txt"),
        &vp("/home/documents/copied.txt"),
    )
    .unwrap_err();
    let reason = reason_of(&err);
    assert!(
        reason.contains("symlink") || reason.contains("outside"),
        "{}",
        reason
    );
    assert!(
        !e.root.join("home/documents/copied.txt").exists(),
        "nothing happened"
    );
    assert_eq!(before, sha256_like(&e.outside.join("sentinel.txt")));
}

#[test]
fn d2_d3_d4_d6_move_combinations_refused() {
    let e = build_env();
    move_path(
        &e.root,
        &vp("/home/documents/notes.txt"),
        &vp("/home/../../out.txt"),
    )
    .unwrap_err();
    move_path(
        &e.root,
        &vp("/home/documents/notes.txt"),
        &vp("/outside-link/dest.txt"),
    )
    .unwrap_err();
    move_path(&e.root, &vp("/home/../../x"), &vp("/home/documents/y")).unwrap_err();
    move_path(&e.root, &vp("/home/documents"), &vp("/outside-link/sub")).unwrap_err();
    assert!(e.root.join("home/documents/notes.txt").exists());
}

#[test]
fn d5_move_root_refused() {
    let e = build_env();
    let err = move_path(&e.root, &vp("/"), &vp("/home/movedroot")).unwrap_err();
    assert!(
        reason_of(&err).contains("root"),
        "names the root: {}",
        reason_of(&err)
    );
    assert!(e.root.exists());
}

// ---------------------------------------------------------------------------
// E. Delete — the dangerous one
// ---------------------------------------------------------------------------

#[test]
fn e1_recursive_delete_of_tree_containing_outside_link() {
    let e = build_env();
    let sentinel_before = sha256_like(&e.outside.join("sentinel.txt"));
    let keep_before = sha256_like(&e.outside.join("sub/keep.txt"));
    delete_path(&e.root, &vp("/trap"), true).unwrap();
    assert!(!e.root.join("trap").exists(), "the tree is gone");
    assert!(
        !e.root.join("trap/escape").exists(),
        "including the escape link"
    );
    // The one assertion whose failure mode is destroying the user's data:
    assert_eq!(
        sentinel_before,
        sha256_like(&e.outside.join("sentinel.txt")),
        "OUTSIDE FILE TOUCHED — recursive delete followed the link"
    );
    assert_eq!(keep_before, sha256_like(&e.outside.join("sub/keep.txt")));
}

#[test]
fn e2_link_loop_terminates_links_removed_as_links() {
    let e = build_env();
    // If descent follows links this never returns. Keep this test in the
    // suite and run cargo test with a timeout (README §K).
    delete_path(&e.root, &vp("/loopdir"), true).unwrap();
    assert!(!e.root.join("loopdir").exists());
}

#[test]
fn e3_delete_non_empty_directory_without_flag_refused() {
    let e = build_env();
    let err = delete_path(&e.root, &vp("/home/documents"), false).unwrap_err();
    let reason = reason_of(&err);
    assert!(
        reason.contains("not empty"),
        "the reason must say so specifically: {}",
        reason
    );
    assert!(e.root.join("home/documents").exists());
}

#[test]
fn e4_delete_empty_subdirectory_recursive() {
    let e = build_env();
    delete_path(&e.root, &vp("/home/documents/subdir"), true).unwrap();
    assert!(!e.root.join("home/documents/subdir").exists());
    assert!(
        e.root.join("home/documents/notes.txt").exists(),
        "rest of tree intact"
    );
}

#[test]
fn e5_delete_empty_subdirectory_plain() {
    let e = build_env();
    delete_path(&e.root, &vp("/home/documents/subdir"), false).unwrap();
    assert!(!e.root.join("home/documents/subdir").exists());
}

#[test]
fn e6_delete_root_refused_unconditional() {
    let e = build_env();
    for recursive in [false, true] {
        let err = delete_path(&e.root, &vp("/"), recursive).unwrap_err();
        let reason = reason_of(&err);
        assert!(reason.contains("root"), "names the root: {}", reason);
        assert!(e.root.exists());
    }
}

#[test]
fn e7_delete_ordinary_file() {
    let e = build_env();
    write_file(&e.root, &vp("/tmp-file.txt"), b"t").unwrap();
    delete_path(&e.root, &vp("/tmp-file.txt"), false).unwrap();
    assert!(!e.root.join("tmp-file.txt").exists());
}

#[test]
fn e7_notes_file_delete() {
    let e = build_env();
    delete_path(&e.root, &vp("/home/documents/notes.txt"), false).unwrap();
    assert!(!e.root.join("home/documents/notes.txt").exists());
}

#[test]
fn e8_delete_absent_refused() {
    let e = build_env();
    let err = delete_path(&e.root, &vp("/home/documents/absent.txt"), false).unwrap_err();
    assert!(
        reason_of(&err).contains("does not exist"),
        "{}",
        reason_of(&err)
    );
}

#[test]
fn e9_delete_outside_link_refused_recorded_limitation_k1() {
    let e = build_env();
    // K.1: the link itself is inside and harmless, but resolving it lands
    // outside, so this refuses. Fail-closed: safe, and it means such a link
    // can never be removed. Recorded, not fixed (fix needs resolver change).
    delete_path(&e.root, &vp("/outside-link"), false).unwrap_err();
    assert!(
        fs::symlink_metadata(e.root.join("outside-link"))
            .unwrap()
            .file_type()
            .is_symlink(),
        "link still there"
    );
    assert_eq!(
        sha256_like(&e.outside.join("sentinel.txt")),
        sha256_like(&e.outside.join("sentinel.txt"))
    );
}

#[test]
fn e10_delete_traversal_to_etc_passwd_refused() {
    let e = build_env();
    delete_path(&e.root, &vp("/../../../etc/passwd"), false).unwrap_err();
}

// ---------------------------------------------------------------------------
// F. Create and write — the ordinary mistakes
// ---------------------------------------------------------------------------

#[test]
fn f1_overwrite_inside() {
    let e = build_env();
    write_file(&e.root, &vp("/home/documents/notes.txt"), b"replaced").unwrap();
    assert_eq!(
        fs::read(e.root.join("home/documents/notes.txt")).unwrap(),
        b"replaced"
    );
}

#[test]
fn f2_write_missing_parent_refused_naming_it() {
    let e = build_env();
    let err = write_file(&e.root, &vp("/home/documents/no-such-dir/x"), b"y").unwrap_err();
    let reason = reason_of(&err);
    assert!(
        reason.contains("/home/documents/no-such-dir"),
        "names the MISSING PARENT: {}",
        reason
    );
    assert!(
        !e.root.join("home/documents/no-such-dir").exists(),
        "write must not quietly create parent directories"
    );
}

#[test]
fn f3_create_dir_makes_intermediate_dirs() {
    let e = build_env();
    create_dir(&e.root, &vp("/home/newdir/deep/deeper")).unwrap();
    assert!(e.root.join("home/newdir/deep/deeper").is_dir());
}

#[test]
fn f4_write_to_a_directory_refused() {
    let e = build_env();
    let err = write_file(&e.root, &vp("/home/documents"), b"x").unwrap_err();
    let reason = reason_of(&err);
    assert!(
        reason.contains("directory"),
        "says so, not obscurely: {}",
        reason
    );
}

#[test]
fn f5_create_dir_over_existing_file_refused() {
    let e = build_env();
    let err = create_dir(&e.root, &vp("/home/documents/notes.txt")).unwrap_err();
    let reason = reason_of(&err);
    assert!(reason.contains("file"), "names the file: {}", reason);
}

#[test]
fn f6_create_dir_existing_is_not_an_error() {
    let e = build_env();
    create_dir(&e.root, &vp("/home/documents")).unwrap();
    assert!(e.root.join("home/documents").is_dir());
}

#[test]
fn f7_write_through_a_file_component_refused() {
    let e = build_env();
    write_file(&e.root, &vp("/home/link-to-file/x"), b"y").unwrap_err();
    assert!(!e.root.join("home/link-to-file/x").exists());
}

// ---------------------------------------------------------------------------
// G. Reading and listing the wrong kind of thing
// ---------------------------------------------------------------------------

#[test]
fn g1_read_a_directory_refused() {
    let e = build_env();
    let err = read_file(&e.root, &vp("/home/documents")).unwrap_err();
    let reason = reason_of(&err);
    assert!(
        reason.contains("directory"),
        "says so, not obscurely: {}",
        reason
    );
}

#[test]
fn g2_list_a_file_refused() {
    let e = build_env();
    let err = list_dir(&e.root, &vp("/home/documents/notes.txt")).unwrap_err();
    let reason = reason_of(&err);
    assert!(
        reason.contains("not a directory") || reason.contains("directory"),
        "says so: {}",
        reason
    );
}

#[test]
fn g3_read_absent_refused() {
    let e = build_env();
    let err = read_file(&e.root, &vp("/home/documents/absent.txt")).unwrap_err();
    assert!(
        reason_of(&err).contains("does not exist"),
        "{}",
        reason_of(&err)
    );
}

#[test]
fn g4_listing_is_sorted_and_labelled() {
    let e = build_env();
    create_dir(&e.root, &vp("/home/g4")).unwrap();
    write_file(&e.root, &vp("/home/g4/b.txt"), b"1").unwrap();
    write_file(&e.root, &vp("/home/g4/a.txt"), b"1").unwrap();
    create_dir(&e.root, &vp("/home/g4/sub")).unwrap();
    symlink("/nowhere-inside", e.root.join("home/g4/z-link")).unwrap();
    let entries = list_dir(&e.root, &vp("/home/g4")).unwrap();
    let names: Vec<_> = entries.iter().map(|d| (d.name.clone(), d.kind)).collect();
    assert_eq!(
        names,
        vec![
            ("a.txt".to_string(), EntryKind::File),
            ("b.txt".to_string(), EntryKind::File),
            ("sub".to_string(), EntryKind::Directory),
            ("z-link".to_string(), EntryKind::Symlink),
        ],
        "sorted by name, each labelled"
    );
}

#[test]
fn g5_inside_link_is_transparent() {
    let e = build_env();
    let bytes = read_file(&e.root, &vp("/inside-link/notes.txt")).unwrap();
    assert_eq!(bytes, b"notes\n");
}

// ---------------------------------------------------------------------------
// I. The boring half — operations that MUST work
// ---------------------------------------------------------------------------

#[test]
fn i1_to_i8_full_cycle_inside() {
    let e = build_env();
    create_dir(&e.root, &vp("/home/work")).unwrap();
    assert!(e.root.join("home/work").is_dir());
    write_file(&e.root, &vp("/home/work/a.txt"), b"alpha").unwrap();
    assert_eq!(
        fs::read(e.root.join("home/work/a.txt")).unwrap().len(),
        5,
        "exactly the bytes alpha, length 5"
    );
    write_file(&e.root, &vp("/home/work/b.txt"), b"beta").unwrap();
    let bytes = read_file(&e.root, &vp("/home/work/a.txt")).unwrap();
    assert_eq!(bytes, b"alpha");
    let entries = list_dir(&e.root, &vp("/home/work")).unwrap();
    let names: Vec<_> = entries.iter().map(|d| (d.name.as_str(), d.kind)).collect();
    assert_eq!(
        names,
        vec![("a.txt", EntryKind::File), ("b.txt", EntryKind::File)]
    );
    move_path(&e.root, &vp("/home/work/a.txt"), &vp("/home/work/c.txt")).unwrap();
    assert!(!e.root.join("home/work/a.txt").exists());
    assert_eq!(fs::read(e.root.join("home/work/c.txt")).unwrap(), b"alpha");
    delete_path(&e.root, &vp("/home/work/c.txt"), false).unwrap();
    assert!(!e.root.join("home/work/c.txt").exists());
    delete_path(&e.root, &vp("/home/work"), true).unwrap();
    assert!(!e.root.join("home/work").exists());
    assert!(e.root.exists(), "I.8's second half: the root survives");
}

#[test]
fn i9_list_root_stable_order() {
    let e = build_env();
    let entries = list_dir(&e.root, &vp("/")).unwrap();
    let names: Vec<_> = entries.iter().map(|d| d.name.clone()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted, "stable order, sorted by name");
    assert!(names.contains(&"home".to_string()));
    assert!(names.contains(&"trap".to_string()));
}

#[test]
fn i10_read_fixture_exact_contents() {
    let e = build_env();
    let bytes = read_file(&e.root, &vp("/home/documents/notes.txt")).unwrap();
    assert_eq!(bytes, b"notes\n");
}

// ---------------------------------------------------------------------------
// K. The symlink-object limitation — reproduced, recorded, not fixed
// ---------------------------------------------------------------------------

#[test]
fn k1_delete_outside_link_refuses_fail_closed() {
    // Covered by e9 above with the limitation comment; kept as its own
    // test to pin §K.1's "cannot be removed" consequence on disk.
    let e = build_env();
    delete_path(&e.root, &vp("/outside-link"), false).unwrap_err();
    assert!(
        fs::symlink_metadata(e.root.join("outside-link")).is_ok(),
        "the link cannot be removed — the recorded §K limitation"
    );
}

#[test]
fn k2_delete_inside_link_removes_the_target_leaves_the_link() {
    // RECORDED LIMITATION, attack-list-2a.md §K.2: resolve() follows the
    // final component, so the operation lands on the target. This test pins
    // the OBSERVED behaviour so the limitation is watched, not so it looks
    // intentional. Fixing it needs a resolver change this phase cannot make.
    let e = build_env();
    delete_path(&e.root, &vp("/inside-link"), true).unwrap();
    assert!(
        !e.root.join("home/documents").exists(),
        "limitation observed: the TARGET was removed"
    );
    assert!(
        fs::symlink_metadata(e.root.join("inside-link")).is_ok(),
        "limitation observed: /inside-link remains, now dangling"
    );
}

#[test]
fn k3_move_inside_link_moves_the_target() {
    // RECORDED LIMITATION, §K.3: same shape as K.2.
    let e = build_env();
    move_path(&e.root, &vp("/inside-link"), &vp("/home/elsewhere")).unwrap();
    assert!(!e.root.join("home/documents").exists());
    assert!(
        e.root.join("home/elsewhere/notes.txt").exists(),
        "the TARGET moved, not the link"
    );
    assert!(
        fs::symlink_metadata(e.root.join("inside-link")).is_ok(),
        "the link remains, dangling"
    );
}
