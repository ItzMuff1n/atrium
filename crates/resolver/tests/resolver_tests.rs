//! Integration tests for the Phase 1 resolver, covering every line of
//! attack-list.md sections A–I (that does not depend on non-existent-path
//! resolution, which is Phase 1b).

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use atrium_resolver::{resolve, ResolveError};

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Fixture {
        let root =
            std::env::temp_dir().join(format!("atrium-test-{}-{}", name, std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        fs::create_dir_all(&root).unwrap();
        build_fixtures(&root);
        Fixture { root }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn build_fixtures(root: &Path) {
    let r = |p: &str| root.join(p);
    fs::create_dir_all(r("home/documents")).unwrap();
    fs::write(r("home/documents/notes.txt"), b"notes\n").unwrap();
    fs::create_dir_all(r("home/subdir")).unwrap();
    fs::create_dir_all(r("home/a/b")).unwrap();
    fs::create_dir_all(r("caf\u{e9}")).unwrap();
    fs::write(r("caf\u{e9}/notes.txt"), b"cafe\n").unwrap();
    fs::create_dir_all(r("cafe\u{301}")).unwrap();
    fs::create_dir_all(r("home/my documents")).unwrap();
    fs::write(r("home/my documents/file.txt"), b"x").unwrap();
    fs::write(r("home/documents/file.name.with.dots.txt"), b"x").unwrap();
    fs::write(r("home/documents/-leading-dash.txt"), b"x").unwrap();
    fs::create_dir_all(r("home/.hidden")).unwrap();
    fs::write(r("home/.hidden/inner.txt"), b"x").unwrap();
    fs::create_dir_all(r("\u{432}\u{43d}\u{443}\u{442}\u{440}\u{438}")).unwrap();
    fs::write(
        r("\u{432}\u{43d}\u{443}\u{442}\u{440}\u{438}/\u{444}\u{430}\u{439}\u{43b}.txt"),
        b"x",
    )
    .unwrap();
    fs::create_dir_all(r("\u{65e5}\u{672c}\u{8a9e}")).unwrap();
    fs::write(
        r("\u{65e5}\u{672c}\u{8a9e}/\u{30d5}\u{30a1}\u{30a4}\u{30eb}.txt"),
        b"x",
    )
    .unwrap();
    // link-to-inside target must be an absolute HOST path that exists
    // (absolute symlink targets are interpreted on the host, not in the
    // sandbox).
    symlink(root.join("home/documents"), r("link-to-inside")).unwrap();
    fs::write(r("home/documents/file.txt"), b"x").unwrap();

    symlink(Path::new("/etc"), r("link-to-etc")).unwrap();
    symlink(Path::new("/home/muffin"), r("link-to-home")).unwrap();
    symlink(Path::new("/"), r("link-to-root")).unwrap();
    symlink(Path::new(".."), r("link-to-parent")).unwrap();
    symlink(Path::new("/etc"), r("chain-b")).unwrap();
    symlink(r("chain-b"), r("chain-a")).unwrap();
    symlink(Path::new("/etc"), r("chain-deep-5")).unwrap();
    for i in (1..5).rev() {
        symlink(
            r(&format!("chain-deep-{}", i + 1)),
            r(&format!("chain-deep-{}", i)),
        )
        .unwrap();
    }
    symlink(r("chain-deep-1"), r("chain-deep")).unwrap();
    symlink(Path::new("loop-a"), r("loop-b")).unwrap();
    symlink(Path::new("loop-b"), r("loop-a")).unwrap();
    symlink(Path::new("/tmp"), r("dir-link")).unwrap();
    fs::create_dir_all(r("good-dir")).unwrap();
    symlink(Path::new("/etc"), r("good-dir/inner-link")).unwrap();
    // Phase 1b fixtures (attack-list-1b.md §K.1a): dangling symlinks.
    // Targets are host paths, as for the other links; the targets are
    // deliberately never created.
    symlink(root.join("home/documents/absent.txt"), r("link-to-nothing")).unwrap();
    symlink(Path::new("/etc/absent.txt"), r("link-to-nothing-out")).unwrap();
}

fn rejects(f: &Fixture, p: &str) -> ResolveError {
    match resolve(&f.root, p) {
        Ok(real) => panic!("expected REJECT for {:?}, got ACCEPT {}", p, real.display()),
        Err(e) => e,
    }
}

fn accepts(f: &Fixture, p: &str) -> PathBuf {
    match resolve(&f.root, p) {
        Ok(real) => {
            assert!(
                real.starts_with(&f.root),
                "ACCEPT of {:?} points outside the root: {}",
                p,
                real.display()
            );
            real
        }
        Err(e) => panic!("expected ACCEPT for {:?}, got REJECT {}", p, e),
    }
}

// ---- unique-reason helper: no two distinct attack lines may share a reason
// string unless the reason is legitimately the same rule. We at least check
// that each rejection Display names something specific (non-generic).

fn assert_specific_reason(err: &ResolveError, path: &str) {
    let s = err.to_string();
    assert!(s.starts_with("Rejected:"), "reason not prefixed: {}", s);
    assert!(
        !s.eq_ignore_ascii_case("invalid path") && s.len() > 25,
        "reason too generic for {:?}: {}",
        path,
        s
    );
}

#[test]
fn section_a_plain_traversal() {
    let f = Fixture::new("a");
    for p in [
        "/..",
        "/../..",
        "/../../..",
        "/../../../../../../../../../../..",
        "/home/../..",
        "/home/documents/../../..",
        "/home/documents/../../../etc/passwd",
        "/./../.",
        "/a/./../../..",
        "/a/b/c/../../../..",
        "/a/b/../b/../b/../../..",
    ] {
        let e = rejects(&f, p);
        assert_specific_reason(&e, p);
    }
}

#[test]
fn section_b_absolute_host_paths() {
    let f = Fixture::new("b");
    // As virtual paths these mean <root>/etc/passwd etc. Under Phase 1b
    // they are ordinary absent names inside the root and must ACCEPT —
    // contained by construction (DECISIONS.md, "The environment namespace
    // follows the Linux tree"; attack-list-1b.md §K). This test's verdicts
    // were inverted by Phase 1b; they asserted absence, which is no longer
    // a rejection reason.
    for p in [
        "/etc/passwd",
        "/etc/shadow",
        "/home/muffin",
        "/home/muffin/.ssh/id_rsa",
        "/root",
        "/proc/self/environ",
        "/proc/self/cwd",
        "/dev/null",
        "/sys/class",
        "/var/log/syslog",
    ] {
        accepts(&f, p);
    }
}

#[test]
fn section_c_relative_paths() {
    let f = Fixture::new("c");
    for p in [
        "home/documents",
        "./home/documents",
        "../home",
        "..",
        ".",
        "documents/../../..",
    ] {
        let e = rejects(&f, p);
        assert!(
            matches!(e, ResolveError::RelativePath { .. }),
            "{:?} should be RelativePath, got {}",
            p,
            e
        );
        assert_specific_reason(&e, p);
    }
}

#[test]
fn section_d_symlinks() {
    let f = Fixture::new("d");
    for p in [
        "/link-to-etc",
        "/link-to-home",
        "/link-to-root",
        "/chain-a",
        "/chain-deep",
        "/dir-link/file.txt",
        "/good-dir/inner-link",
        "/link-to-etc/../home/documents",
    ] {
        let e = rejects(&f, p);
        assert_specific_reason(&e, p);
    }
    // link-to-parent -> ".." (relative symlink target); it resolves to the
    // root's parent on the host → escape → reject.
    let e = rejects(&f, "/link-to-parent");
    assert_specific_reason(&e, "/link-to-parent");
    // Loop: must reject cleanly, not hang, not crash. std canonicalize
    // returns ELOOP; we map it to our own reason.
    let e = rejects(&f, "/loop-a");
    assert!(
        matches!(e, ResolveError::SymlinkLoop { .. }),
        "expected SymlinkLoop, got {}",
        e
    );
}

#[test]
fn section_e_separator_normalisation() {
    let f = Fixture::new("e");
    // Real rejections: doubled separators around a climbing `..` still
    // climb. Bare backslash spellings have no leading `/`, so they are
    // still relative paths and reject as such. The absolute backslash
    // names are ordinary absent names inside the root (ruling K.1c — a
    // backslash is a filename character, never a separator) and ACCEPT
    // under Phase 1b. These two verdicts were inverted by Phase 1b.
    for p in ["\\", "\\..\\.."] {
        let e = rejects(&f, p);
        assert!(
            matches!(e, ResolveError::RelativePath { .. }),
            "{:?} expected RelativePath, got {}",
            p,
            e
        );
    }
    for p in ["/home\\..\\..", "/home/documents\\..\\.."] {
        accepts(&f, p);
    }
    let e = rejects(&f, "/home//../..");
    assert_specific_reason(&e, "/home//../..");
    // Moved into section I on 12 Sep 2026: these were wrongly listed as
    // reject-cases here. They are lexically identical to section-I accepts
    // after component normalisation, so they must ACCEPT and must stay
    // inside the root. See attack-list.md section E.
    let canon_root = fs::canonicalize(&f.root).unwrap();
    assert_eq!(accepts(&f, "//"), canon_root);
    assert_eq!(
        accepts(&f, "///home///documents"),
        canon_root.join("home/documents")
    );
    assert_eq!(
        accepts(&f, "/home/documents//"),
        canon_root.join("home/documents")
    );
    assert_eq!(
        accepts(&f, "/home/./documents/."),
        canon_root.join("home/documents")
    );
    assert_eq!(accepts(&f, "/home/documents/.."), canon_root.join("home"));
}

#[test]
fn section_f_unicode_and_lookalikes() {
    let f = Fixture::new("f");
    // Lookalike separators are ordinary characters on Linux: these names
    // are absent inside the root. Under Phase 1b absent-inside accepts,
    // and the names are contained by construction. These verdicts were
    // inverted by Phase 1b (they asserted NotFound).
    for p in [
        "/home\u{2044}documents",
        "/home\u{2215}documents",
        "/home\u{FF0F}documents",
        "/home/\u{202E}documents",
        "/home/\u{FEFF}documents",
    ] {
        accepts(&f, p);
    }
    // NUL must be rejected by the resolver with its own named reason,
    // not a leaked OS error.
    for p in ["/home\u{0}documents", "/home/doc\u{0}uments", "/\u{0}"] {
        let e = rejects(&f, p);
        assert!(
            matches!(e, ResolveError::NullByte { .. }),
            "{:?} expected NullByte, got {}",
            p,
            e
        );
        let s = e.to_string();
        assert!(s.contains("NUL"), "NUL reason must name NUL, got {}", s);
    }
    // OS layer also rejects NUL (defense in depth, both layers tested).
    let os_err = fs::metadata("/home\0documents").unwrap_err();
    assert_eq!(os_err.kind(), std::io::ErrorKind::InvalidInput);
    // The two café spellings are different byte sequences and must not be
    // silently conflated: composed resolves to the composed dir; asking for
    // a missing child inside each is absent-inside → under Phase 1b these
    // ACCEPT (verdicts inverted; they asserted NotFound).
    accepts(&f, "/caf\u{e9}/documents");
    accepts(&f, "/cafe\u{301}/documents");
    // And the composed form itself is a distinct existing directory:
    assert!(accepts(&f, "/caf\u{e9}").ends_with("caf\u{e9}"));
    assert!(accepts(&f, "/cafe\u{301}").ends_with("cafe\u{301}"));
}

#[test]
fn section_g_length_and_shape() {
    let f = Fixture::new("g");
    // Item 10 (DECISIONS.md, "Phase 1b — three resolver rulings"): the
    // resolver checks component length itself, in bytes, per component.
    // A 300-char single component is 300 bytes → REJECT with the
    // resolver's OWN reason (not the OS's ENAMETOOLONG), on an ABSENT
    // name so the only possible source of the rejection is the check.
    // (Old assertion: ENAMETOOLONG-class clean rejection, asserted only
    // that the reject was specific. Inverted in detail: same verdict, new
    // required reason.)
    let long_comp = format!("/{}", "a".repeat(300));
    let e = rejects(&f, &long_comp);
    assert!(
        matches!(e, ResolveError::NameTooLong { bytes: 300, .. }),
        "300-byte component must reject with NameTooLong, got {}",
        e
    );
    assert!(
        !e.to_string().contains("os error"),
        "reason must be the resolver's own, got {}",
        e
    );
    // 200 nested short components → long path of short names. Under item
    // 10 this ACCEPTs (PATH_MAX deliberately not enforced). Its Phase 1
    // rejection was absence, not a length verdict. Verdict inverted.
    let deep = format!("/{}", vec!["a"; 200].join("/"));
    accepts(&f, &deep);
    // Empty string → EmptyPath.
    let e = rejects(&f, "");
    assert!(matches!(e, ResolveError::EmptyPath));
    // Single space → still a RELATIVE path, rejected as such.
    let e = rejects(&f, " ");
    assert!(matches!(e, ResolveError::RelativePath { .. }));
    // Root-plus-spaces / trailing dots — legal Linux names, absent in the
    // fixture → under Phase 1b they ACCEPT (verdicts inverted; they
    // asserted NotFound).
    for p in [
        "/   ",
        "/home/documents.",
        "/home/documents...",
        "/home/documents ",
        "/home/ documents",
    ] {
        accepts(&f, p);
    }
}

#[test]
#[ignore]
fn section_j_component_length() {
    // The three length tests required by name (brief §9.8), plus the
    // 255-byte fixture-on-disk accept. All rejections are on ABSENT names
    // and must carry the resolver's own reason — never a leaked OS error.
    let f = Fixture::new("len");
    let canon_root = fs::canonicalize(&f.root).unwrap();

    // 255-byte ASCII name, ABSENT → ACCEPT (resolver's own check, exactly
    // at the limit).
    let name255 = "a".repeat(255);
    let p = format!("/{}", name255);
    let real = accepts(&f, &p);
    assert_eq!(real, canon_root.join(&name255));

    // 255-byte ASCII name as a REAL fixture on disk (the filesystem
    // agrees it is legal) → ACCEPT, ending at the fixture.
    fs::write(f.root.join(&name255), b"x").unwrap();
    let real = accepts(&f, &p);
    assert_eq!(real, canon_root.join(&name255));

    // 256-byte ASCII name, ABSENT → REJECT with the resolver's own reason.
    let p = format!("/{}", "b".repeat(256));
    let e = rejects(&f, &p);
    assert!(
        matches!(e, ResolveError::NameTooLong { bytes: 256, .. }),
        "256-byte component must reject with NameTooLong, got {}",
        e
    );
    assert!(
        !e.to_string().contains("os error"),
        "reason must be the resolver's own, got {}",
        e
    );

    // 200 Hebrew characters — 400 bytes, only 200 characters. REJECT.
    // This is the one a character-counting implementation gets wrong.
    let hebrew: String = "\u{5d0}".repeat(200);
    assert_eq!(hebrew.len(), 400);
    let p = format!("/{}", hebrew);
    let e = rejects(&f, &p);
    assert!(
        matches!(e, ResolveError::NameTooLong { bytes: 400, .. }),
        "400-byte component must reject with NameTooLong, got {}",
        e
    );
    assert!(
        !e.to_string().contains("os error"),
        "reason must be the resolver's own, got {}",
        e
    );

    // Length is per component, mid-path too: an over-long component deep
    // in an absent path still rejects on the resolver's own check.
    let p = format!("/newdir/{}/tail", "c".repeat(256));
    let e = rejects(&f, &p);
    assert!(matches!(e, ResolveError::NameTooLong { .. }), "{}", e);

    // 200 nested short components → ACCEPT (long path, short names).
    let deep = format!("/{}", vec!["a"; 200].join("/"));
    accepts(&f, &deep);
}

#[test]
fn section_h_nasty_combinations() {
    let f = Fixture::new("h");
    for p in [
        "/link-to-etc/../../..",
        "/home/../link-to-root/home/muffin",
        "//../..",
        "/home/documents/../../../",
        "/./link-to-parent/..",
    ] {
        let e = rejects(&f, p);
        assert_specific_reason(&e, p);
    }
    let e = rejects(&f, "/home/\u{0}/../..");
    assert!(matches!(e, ResolveError::NullByte { .. }));
}

#[test]
fn section_i_must_be_accepted() {
    let f = Fixture::new("i");
    for p in [
        "/",
        "/home",
        "/home/documents",
        "/home/documents/notes.txt",
        "/home/documents/",
        "/home/./documents",
        "/home/subdir/../documents",
        "/home/a/b/../../documents",
        "/caf\u{e9}/notes.txt",
        "/home/my documents/file.txt",
        "/home/documents/file.name.with.dots.txt",
        "/home/documents/-leading-dash.txt",
        "/home/.hidden",
        "/home/.hidden/inner.txt",
        "/\u{432}\u{43d}\u{443}\u{442}\u{440}\u{438}/\u{444}\u{430}\u{439}\u{43b}.txt",
        "/\u{65e5}\u{672c}\u{8a9e}/\u{30d5}\u{30a1}\u{30a4}\u{30eb}.txt",
        "/link-to-inside",
        "/link-to-inside/file.txt",
        // Moved from section E on 12 Sep 2026 — wrongly listed as reject-cases
        // there; they normalise to section-I paths and must ACCEPT.
        "//",
        "///home///documents",
        "/home/documents//",
        "/home/./documents/.",
        "/home/documents/..",
    ] {
        accepts(&f, p);
    }
}

#[test]
fn resolve_never_mutates() {
    // Resolving a path must not create anything. Resolve a missing path
    // inside an existing directory and confirm nothing appeared.
    let f = Fixture::new("nomut");
    let before: Vec<_> = fs::read_dir(f.root.join("home/documents"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    let _ = resolve(&f.root, "/home/documents/does-not-exist.txt");
    let after: Vec<_> = fs::read_dir(f.root.join("home/documents"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(before, after);
}

#[test]
fn acceptance_stays_inside_root() {
    // A symlink inside pointing inside must resolve to a path still under
    // the canonical root.
    let f = Fixture::new("inside");
    let real = accepts(&f, "/link-to-inside/file.txt");
    let canon_root = fs::canonicalize(&f.root).unwrap();
    assert!(real.starts_with(&canon_root));
}

/// Assert a rejection's reason names an ESCAPE (a `..` step or a symlink),
/// not merely absence — the discriminator that separates a working Phase
/// 1b from one that never implemented it (attack-list-1b.md, "Why the
/// reason matters").
fn assert_reason_names_escape(err: &ResolveError, path: &str) {
    assert!(
        !matches!(err, ResolveError::NotFound { .. }),
        "{:?} rejected as NotFound; the reason must name the escape, got {}",
        path,
        err
    );
    assert!(
        !err.to_string().contains("os error 36"),
        "{:?} rejected with a leaked OS error, got {}",
        path,
        err
    );
}

/// Every line of attack-list-1b.md, sections A–H (reject, reason names the
/// escape), section I and K (accept, verified contained), and K.1a/c.
#[test]
fn phase1b_attack_list() {
    let f = Fixture::new("p1b");

    // A. Traversal in the part that exists, absent tail.
    for p in [
        "/../newfile",
        "/../../newfile",
        "/../../..",
        "/home/../../newfile",
        "/home/documents/../../../etc/newfile",
        "/a/./../../newfile",
    ] {
        let e = rejects(&f, p);
        assert_specific_reason(&e, p);
        assert_reason_names_escape(&e, p);
    }
    // B. `..` inside the part that does not exist.
    for p in [
        "/newdir/../../newfile",
        "/newdir/../..",
        "/newfile/../..",
        "/home/newdir/../../../newfile",
        "/a/b/newdir/../../../../newfile",
        "/home/documents/newdir/../../../../etc/newfile",
    ] {
        let e = rejects(&f, p);
        assert_specific_reason(&e, p);
        assert_reason_names_escape(&e, p);
    }
    // C. Absent tail on an existing symlink that points outside.
    for p in [
        "/link-to-etc/newfile",
        "/link-to-home/newfile",
        "/link-to-root/newfile",
        "/link-to-parent/newfile",
        "/chain-a/newfile",
        "/link-to-etc/sub/newfile",
        "/good-dir/inner-link/newfile",
        "/link-to-etc/../home/newfile",
    ] {
        let e = rejects(&f, p);
        assert_specific_reason(&e, p);
        assert_reason_names_escape(&e, p);
    }
    // D. Escapes that appear only once the absent tail is appended.
    for p in [
        "/link-to-inside/../../../newfile",
        "/link-to-inside/newdir/../../../../newfile",
        "/good-dir/../../../newfile",
        "/link-to-root/../etc/newfile",
    ] {
        let e = rejects(&f, p);
        assert_specific_reason(&e, p);
        assert_reason_names_escape(&e, p);
    }
    // E. Out, back, then absent.
    for p in [
        "/link-to-etc/../../home/documents/newfile",
        "/home/../link-to-root/home/newfile",
        "//../newfile",
    ] {
        let e = rejects(&f, p);
        assert_specific_reason(&e, p);
        assert_reason_names_escape(&e, p);
    }
    // F. Absent parent, escaping remainder.
    for p in [
        "/newdir/../../etc/newfile",
        "/home/newdir/../../../root/newfile",
    ] {
        let e = rejects(&f, p);
        assert_specific_reason(&e, p);
        assert_reason_names_escape(&e, p);
    }
    // G. Separator and normalisation tricks, absent tail.
    for p in [
        "/home//..//..//newfile",
        "/home/documents//../..//../newfile",
        "/home/documents/newdir//../../../..//newfile",
    ] {
        let e = rejects(&f, p);
        assert_specific_reason(&e, p);
        assert_reason_names_escape(&e, p);
    }
    // H. Nasty combinations.
    for p in [
        "/link-to-etc/../../newfile",
        "/home/../link-to-root/newfile",
        "/link-to-parent/newdir/newfile",
        "/..//newfile",
        "/home/documents/../../../newfile",
        "/link-to-etc/./../newfile",
    ] {
        let e = rejects(&f, p);
        assert_specific_reason(&e, p);
        assert_reason_names_escape(&e, p);
    }

    // I. Must be ACCEPTED (absent, inside the root).
    for p in [
        "/newfile",
        "/newdir/newfile",
        "/newdir/",
        "/new file.txt",
        "/newdir/.hidden",
        "/newdir/file.name.with.dots.txt",
        "/newdir/-leading-dash.txt",
        "/home/newfile",
        "/home/documents/newfile",
        "/home/documents/newdir/newfile",
        "/home/documents/./newfile",
        "/home//newfile",
        "//newfile",
        "/home/documents/../newfile",
        "/home/newdir/../newfile",
        "/home/newdir/../../newfile",
        "/newdir/../newfile",
        "/home/documents/newdir/../../newfile",
        "/link-to-inside/newfile",
        "/link-to-inside/newdir/newfile",
        "/\u{43d}\u{43e}\u{432}\u{44b}\u{439}/\u{444}\u{430}\u{439}\u{43b}.txt",
        "/\u{65b0}\u{3057}\u{3044}/\u{30d5}\u{30a1}\u{30a4}\u{30eb}.txt",
        "/newdir/\u{43d}\u{43e}\u{432}\u{44b}\u{439}/\u{444}\u{430}\u{439}\u{43b}.txt",
    ] {
        accepts(&f, p);
    }

    // K. Names that read as escapes and are not — all ACCEPT.
    for p in [
        "/etc/passwd",
        "/etc/newdir/newfile",
        "/root/newfile",
        "/proc/self/newfile",
        "/sys/class/newfile",
        "/dev/newfile",
    ] {
        accepts(&f, p);
    }

    // K.1a: a dangling symlink inside the root ACCEPTs to the target
    // location; its counterpart pointing outside REJECTs.
    let canon_root = fs::canonicalize(&f.root).unwrap();
    let real = accepts(&f, "/link-to-nothing");
    assert_eq!(real, canon_root.join("home/documents/absent.txt"));
    let e = rejects(&f, "/link-to-nothing-out");
    assert!(
        matches!(e, ResolveError::SymlinkEscapes { .. }),
        "expected SymlinkEscapes for /link-to-nothing-out, got {}",
        e
    );
    // An absent tail appended to a dangling-out link rejects the same way.
    let e = rejects(&f, "/link-to-nothing-out/newfile");
    assert_reason_names_escape(&e, "/link-to-nothing-out/newfile");
    // And an absent tail on a dangling-inside link accepts.
    let real = accepts(&f, "/link-to-nothing/newfile");
    assert_eq!(real, canon_root.join("home/documents/absent.txt/newfile"));

    // K.1c: backslash is an ordinary filename character; absent → accept.
    accepts(&f, "/home\\..\\..\\newfile");

    // §9.8 named traps:
    // an absent path whose parent chain leaves the root, rejected naming
    // the `..` step.
    let e = rejects(&f, "/../newfile");
    assert!(
        format!("{}", e).contains(".."),
        "the reason must name the `..` step, got {}",
        e
    );
    // an absent tail appended to a symlink pointing outside.
    let e = rejects(&f, "/link-to-etc/newfile");
    assert!(
        matches!(e, ResolveError::SymlinkEscapes { .. }),
        "expected SymlinkEscapes, got {}",
        e
    );

    // Ruling K.1b (unchanged Phase 1 behaviour, verified here): a file
    // followed by `..` or a name rejects with the OS's ENOTDIR reason.
    let e = rejects(&f, "/home/documents/notes.txt/..");
    assert!(
        matches!(e, ResolveError::OsError { .. }),
        "expected OsError, got {}",
        e
    );
    assert!(e.to_string().contains("Not a directory"), "{}", e);
    let e = rejects(&f, "/home/documents/notes.txt/newfile");
    assert!(e.to_string().contains("Not a directory"), "{}", e);
    // The recorded deviation that coexists with it: a trailing slash on a
    // file ACCEPTs.
    accepts(&f, "/home/documents/notes.txt/");
}
