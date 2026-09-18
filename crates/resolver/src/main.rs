//! atrium-resolver — command-line harness (brief §8.5).
//!
//! Three modes:
//!   atrium-resolver resolve --root <dir> <virtual-path>
//!   atrium-resolver demo
//!   atrium-resolver fixtures --root <dir>

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use atrium_resolver::resolve;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("resolve") => mode_resolve(&args[1..]),
        Some("demo") => mode_demo(),
        Some("fixtures") => mode_fixtures(&args[1..]),
        _ => {
            eprintln!("usage:");
            eprintln!("  atrium-resolver resolve --root <dir> <virtual-path>");
            eprintln!("  atrium-resolver demo");
            eprintln!("  atrium-resolver fixtures --root <dir>");
            ExitCode::from(2)
        }
    }
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == flag {
            return it.next().cloned();
        }
    }
    None
}

fn mode_resolve(args: &[String]) -> ExitCode {
    let root = match flag_value(args, "--root") {
        Some(r) => PathBuf::from(r),
        None => {
            eprintln!("resolve: missing --root <dir>");
            return ExitCode::from(2);
        }
    };
    // Boring positional parse: the arg that is neither --root nor its value.
    let mut positional: Option<String> = None;
    let mut skip = false;
    for a in args {
        if skip {
            skip = false;
            continue;
        }
        if a == "--root" {
            skip = true;
            continue;
        }
        positional = Some(a.clone());
    }
    let vpath = match positional {
        Some(p) => p,
        None => {
            eprintln!("resolve: missing <virtual-path>");
            return ExitCode::from(2);
        }
    };
    match resolve(&root, &vpath) {
        Ok(real) => {
            println!("ACCEPT {}", real.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            println!("REJECT {}", e);
            ExitCode::from(1)
        }
    }
}

fn mode_fixtures(args: &[String]) -> ExitCode {
    let root = match flag_value(args, "--root") {
        Some(r) => PathBuf::from(r),
        None => {
            eprintln!("fixtures: missing --root <dir>");
            return ExitCode::from(2);
        }
    };
    match build_fixtures(&root) {
        Ok(()) => {
            println!("fixtures created under {}", root.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("fixtures: {}", e);
            ExitCode::FAILURE
        }
    }
}

/// Create every directory, file and symlink referenced in attack-list.md
/// sections D and I, inside `root`. Creates nothing outside `root`.
/// Used by `fixtures` mode and by `demo` mode.
fn build_fixtures(root: &Path) -> std::io::Result<()> {
    use std::fs;
    use std::os::unix::fs::symlink;

    fs::create_dir_all(root)?;
    let r = |p: &str| root.join(p);

    // Section I positive fixtures.
    fs::create_dir_all(r("home/documents"))?;
    fs::write(r("home/documents/notes.txt"), b"notes\n")?;
    fs::create_dir_all(r("home/subdir"))?;
    fs::create_dir_all(r("home/a/b"))?;
    fs::create_dir_all(r("caf\u{e9}"))?; // composed é
    fs::write(r("caf\u{e9}/notes.txt"), b"cafe\n")?;
    fs::create_dir_all(r("cafe\u{301}"))?; // decomposed é
    fs::create_dir_all(r("home/my documents"))?;
    fs::write(r("home/my documents/file.txt"), b"spaces\n")?;
    fs::write(r("home/documents/file.name.with.dots.txt"), b"dots\n")?;
    fs::write(r("home/documents/-leading-dash.txt"), b"dash\n")?;
    fs::create_dir_all(r("home/.hidden"))?;
    fs::write(r("home/.hidden/inner.txt"), b"hidden\n")?;
    fs::create_dir_all(r("\u{432}\u{43d}\u{443}\u{442}\u{440}\u{438}"))?; // внутри
    fs::write(
        r("\u{432}\u{43d}\u{443}\u{442}\u{440}\u{438}/\u{444}\u{430}\u{439}\u{43b}.txt"),
        b"ru\n",
    )?;
    fs::create_dir_all(r("\u{65e5}\u{672c}\u{8a9e}"))?; // 日本語
    fs::write(
        r("\u{65e5}\u{672c}\u{8a9e}/\u{30d5}\u{30a1}\u{30a4}\u{30eb}.txt"),
        b"jp\n",
    )?;

    // link-to-inside: symlink pointing at another place inside the root.
    // Absolute symlink targets are host-absolute, so the target must be the
    // real location of home/documents under this root.
    if !r("link-to-inside").exists() {
        symlink(root.join("home/documents"), r("link-to-inside"))?;
    }
    fs::write(r("home/documents/file.txt"), b"file\n")?;

    // Section D attack symlinks.
    let mk = |name: &str, target: &str| -> std::io::Result<()> {
        let p = r(name);
        if std::fs::symlink_metadata(&p).is_err() {
            symlink(Path::new(target), p)?;
        }
        Ok(())
    };
    mk("link-to-etc", "/etc")?;
    mk("link-to-home", "/home/muffin")?;
    mk("link-to-root", "/")?;
    mk("link-to-parent", "..")?;
    // chain-a -> chain-b -> /etc
    mk("chain-b", "/etc")?;
    if std::fs::symlink_metadata(r("chain-a")).is_err() {
        symlink(r("chain-b"), r("chain-a"))?;
    }
    // chain-deep: 5 hops, last one lands outside.
    mk("chain-deep-5", "/etc")?;
    for i in (1..5).rev() {
        if std::fs::symlink_metadata(r(&format!("chain-deep-{}", i))).is_err() {
            symlink(
                r(&format!("chain-deep-{}", i + 1)),
                r(&format!("chain-deep-{}", i)),
            )?;
        }
    }
    if std::fs::symlink_metadata(r("chain-deep")).is_err() {
        symlink(r("chain-deep-1"), r("chain-deep"))?;
    }
    // loop-a <-> loop-b
    mk("loop-b", "loop-a")?;
    mk("loop-a", "loop-b")?;
    // dir-link -> outside directory, with a file we reference through it.
    mk("dir-link", "/tmp")?;
    // good-dir/inner-link: link sits deep, not at the root.
    fs::create_dir_all(r("good-dir"))?;
    if std::fs::symlink_metadata(r("good-dir/inner-link")).is_err() {
        symlink(Path::new("/etc"), r("good-dir/inner-link"))?;
    }

    // Phase 1b fixtures (attack-list-1b.md §Fixtures): dangling symlinks.
    // Targets are host paths, as above; the targets themselves are never
    // created here or anywhere.
    if std::fs::symlink_metadata(r("link-to-nothing")).is_err() {
        symlink(root.join("home/documents/absent.txt"), r("link-to-nothing"))?;
    }
    if std::fs::symlink_metadata(r("link-to-nothing-out")).is_err() {
        symlink(Path::new("/etc/absent.txt"), r("link-to-nothing-out"))?;
    }
    // attack-list-1b.md §M.3: a RELATIVE symlink target that climbs above the
    // root. The target is interpreted against the link's own directory, so the
    // traversal never appears in the requested path — a resolver that clamps
    // only what the caller typed misses it. From `<root>` this resolves to
    // `/outside` on the host; the target is never created.
    if std::fs::symlink_metadata(r("link-rel-out")).is_err() {
        symlink(Path::new("../../outside"), r("link-rel-out"))?;
    }

    Ok(())
}

/// One attack-list line: (virtual path, expected to be accepted?).
struct Case {
    path: &'static str,
    expect_accept: bool,
    section: &'static str,
}

fn demo_cases() -> Vec<Case> {
    let mut v = Vec::new();
    let r = |path: &'static str, section: &'static str| Case {
        path,
        expect_accept: false,
        section,
    };
    let a = |path: &'static str, section: &'static str| Case {
        path,
        expect_accept: true,
        section,
    };

    // A. Plain traversal (all reject).
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
        v.push(r(p, "A"));
    }
    // B. Absolute host paths — as virtual paths these mean
    // <root>/etc/passwd etc. Under Phase 1b they are ordinary absent names
    // inside the root and must ACCEPT (attack-list-1b.md §K; verdicts
    // inverted from Phase 1's, which rejected them as absent).
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
        v.push(a(p, "B"));
    }
    // C. Relative paths (reject).
    for p in [
        "home/documents",
        "./home/documents",
        "../home",
        "..",
        ".",
        "documents/../../..",
    ] {
        v.push(r(p, "C"));
    }
    // D. Symlinks (reject).
    for p in [
        "/link-to-etc",
        "/link-to-home",
        "/link-to-root",
        "/link-to-parent",
        "/chain-a",
        "/chain-deep",
        "/loop-a",
        "/dir-link/file.txt",
        "/good-dir/inner-link",
        "/link-to-etc/../home/documents",
    ] {
        v.push(r(p, "D"));
    }
    // E. Separator / normalisation tricks.
    //
    // The five lines formerly listed here as reject-cases were removed on
    // 12 Sep 2026: `//`, `///home///documents`, `/home/documents//`,
    // `/home/./documents/.` and `/home/documents/..` are lexically identical
    // to section-I ACCEPT lines after component normalisation (repeated `/`
    // collapse, `.` elision, POSIX `..`). They are now real pass/fail cases
    // in the section-I block below, not informational lines. See
    // attack-list.md section E for the correction.
    for p in [
        "/home//../..", // == /home/../.. → climbs above root: real reject
    ] {
        v.push(r(p, "E"));
    }
    // The bare backslash spellings have no leading `/`: still relative
    // paths, still rejected.
    for p in ["\\", "\\..\\.."] {
        v.push(r(p, "E"));
    }
    // The absolute backslash names are ordinary filename characters
    // (ruling K.1c); absent inside the root → accept under Phase 1b
    // (verdicts inverted; they rejected as absent).
    for p in ["/home\\..\\..", "/home/documents\\..\\.."] {
        v.push(a(p, "E"));
    }
    // F. Unicode / lookalikes. The NUL lines still reject (the resolver's
    // own reason). The rest are absent names inside the root and ACCEPT
    // under Phase 1b (verdicts inverted; they rejected as absent).
    for p in ["/home\u{0}documents", "/home/doc\u{0}uments", "/\u{0}"] {
        v.push(r(p, "F"));
    }
    for p in [
        "/home\u{2044}documents",
        "/home\u{2215}documents",
        "/home\u{FF0F}documents",
        "/home/\u{202E}documents",
        "/home/\u{FEFF}documents",
        "/caf\u{e9}/documents",   // composed: /café/documents absent → accept
        "/cafe\u{301}/documents", // decomposed: /café/documents absent → accept
    ] {
        v.push(a(p, "F"));
    }
    // G. Length and shape (reject). Long names generated at runtime below.
    // A bare "/" was removed from this section on 12 Sep 2026: it was listed
    // as a reject-case here while section I required it to ACCEPT. Section I
    // is correct; "/" is a section-I case now.
    for p in [
        "/   ",
        "/home/documents.",
        "/home/documents...",
        "/home/documents ",
        "/home/ documents",
    ] {
        v.push(a(p, "G")); // absent names inside the root: accept under 1b
    }
    v.push(r(" ", "G")); // still a relative path: reject
                         // H. Nasty combinations (reject).
    for p in [
        "/link-to-etc/../../..",
        "/home/../link-to-root/home/muffin",
        "/home/\u{0}/../..",
        "//../..",
        "/home/documents/../../../",
        "/./link-to-parent/..",
    ] {
        v.push(r(p, "H"));
    }
    // I. Must be ACCEPTED.
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
        // Moved from section E on 12 Sep 2026 — these were wrongly listed as
        // reject-cases; they normalise to section-I paths and must ACCEPT.
        "//",
        "///home///documents",
        "/home/documents//",
        "/home/./documents/.",
        "/home/documents/..",
    ] {
        v.push(a(p, "I"));
    }
    v
}

fn mode_demo() -> ExitCode {
    let tmp = std::env::temp_dir().join(format!("atrium-demo-{}", std::process::id()));
    if tmp.exists() {
        let _ = std::fs::remove_dir_all(&tmp);
    }
    if let Err(e) = std::fs::create_dir_all(&tmp) {
        eprintln!("demo: cannot create temp dir {}: {}", tmp.display(), e);
        return ExitCode::FAILURE;
    }
    if let Err(e) = build_fixtures(&tmp) {
        eprintln!("demo: fixtures failed: {}", e);
        let _ = std::fs::remove_dir_all(&tmp);
        return ExitCode::FAILURE;
    }
    println!("demo root: {}", tmp.display());

    let mut failures = 0usize;
    for c in demo_cases() {
        let verdict = resolve(&tmp, c.path);
        let (got, detail) = match &verdict {
            Ok(p) => ("ACCEPT", format!("{}", p.display())),
            Err(e) => ("REJECT", format!("{}", e)),
        };
        let ok = (verdict.is_ok()) == c.expect_accept;
        // Extra check: an ACCEPT must still be inside the root.
        let containment_ok = match &verdict {
            Ok(p) => p.starts_with(&tmp),
            Err(_) => true,
        };
        let mark = if ok && containment_ok { "ok  " } else { "FAIL" };
        if !(ok && containment_ok) {
            failures += 1;
        }
        println!(
            "[{}] [{} {:>7}] {:<40} {}",
            mark,
            c.section,
            got,
            c.path.escape_debug(),
            detail
        );
    }

    // Runtime-generated length cases (G / attack-list-1b §J): a 300-char
    // single component rejects on the resolver's own 255-byte check; a
    // 200-deep nesting of short names now ACCEPTs (PATH_MAX is not
    // enforced — the Phase 1 rejection was absence, not length).
    let long_comp = format!("/{}", "a".repeat(300));
    let verdict = resolve(&tmp, &long_comp);
    match &verdict {
        Ok(p) => {
            failures += 1;
            println!(
                "[FAIL] [G  ACCEPT] {:<40} wrongly accepted {}",
                long_comp.escape_debug(),
                p.display()
            );
        }
        Err(e) => println!("[ok  ] [G  REJECT] {:<40} {}", long_comp.escape_debug(), e),
    }
    let deep = format!("/{}", vec!["a"; 200].join("/"));
    let verdict = resolve(&tmp, &deep);
    match &verdict {
        Ok(p) => {
            if p.starts_with(&tmp) {
                println!(
                    "[ok  ] [G  ACCEPT] {:<40} {}",
                    deep.escape_debug(),
                    p.display()
                );
            } else {
                failures += 1;
                println!(
                    "[FAIL] [G  ACCEPT] {:<40} escapes the root: {}",
                    deep.escape_debug(),
                    p.display()
                );
            }
        }
        Err(e) => {
            failures += 1;
            println!(
                "[FAIL] [G  REJECT] {:<40} wrongly rejected: {}",
                deep.escape_debug(),
                e
            );
        }
    }

    // Phase 1b fixtures: dangling symlinks. Inside-target accepts to the
    // target's location; outside-target rejects (ruling K.1a).
    for (p, expect_accept) in [
        ("/link-to-nothing", true),
        ("/link-to-nothing-out", false),
        ("/newfile", true),
        ("/home/documents/../newfile", true),
        ("/link-to-inside/newfile", true),
        ("/home\\..\\..\\newfile", true), // backslash is a filename char
    ] {
        let verdict = resolve(&tmp, p);
        let (got, detail) = match &verdict {
            Ok(r) => ("ACCEPT", format!("{}", r.display())),
            Err(e) => ("REJECT", format!("{}", e)),
        };
        let ok = verdict.is_ok() == expect_accept
            && verdict
                .as_ref()
                .map(|r| r.starts_with(&tmp))
                .unwrap_or(true);
        if !ok {
            failures += 1;
        }
        println!(
            "[{}] [1b {:>7}] {:<40} {}",
            if ok { "ok  " } else { "FAIL" },
            got,
            p.escape_debug(),
            detail
        );
    }

    let _ = std::fs::remove_dir_all(&tmp);
    if failures > 0 {
        println!("demo: {} case(s) FAILED", failures);
        ExitCode::FAILURE
    } else {
        println!("demo: all cases behaved as required");
        ExitCode::SUCCESS
    }
}
