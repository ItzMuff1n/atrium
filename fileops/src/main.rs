//! atrium-fileops — CLI harness (brief §8: a `run` mode and a `demo` mode,
//! like resolver's).
//!
//!   atrium-fileops run --root <dir> create-dir <vpath>
//!   atrium-fileops run --root <dir> write <vpath> --content <text>
//!   atrium-fileops run --root <dir> read <vpath>
//!   atrium-fileops run --root <dir> list <vpath>
//!   atrium-fileops run --root <dir> move <from> <to>
//!   atrium-fileops run --root <dir> delete <vpath> [--recursive]
//!   atrium-fileops run --root <dir> fixtures        (build attack-list-2a fixtures)
//!   atrium-fileops demo
//!
//! Output contract (attack-list-2a.md §H): every line prints the VIRTUAL
//! path that was asked for, never the resolved real one — the resolver's own
//! CLI prints the real path on ACCEPT, and that habit must not be copied
//! here (§H.4 note). The real location is available only behind
//! `--show-real`, off by default (§H.5). File bytes from `read` go to
//! stdout after a header line naming the virtual path and the byte length.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use atrium_fileops::{
    create_dir, delete_path, list_dir, move_path, read_file, write_file, FileOpError, VirtualPath,
};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        usage();
        return ExitCode::from(2);
    }
    match args[0].as_str() {
        "run" => mode_run(&args[1..]),
        "demo" => mode_demo(),
        _ => {
            usage();
            ExitCode::from(2)
        }
    }
}

fn usage() {
    eprintln!("usage:");
    eprintln!("  atrium-fileops run --root <dir> create-dir <vpath>");
    eprintln!("  atrium-fileops run --root <dir> write <vpath> --content <text>");
    eprintln!("  atrium-fileops run --root <dir> read <vpath>");
    eprintln!("  atrium-fileops run --root <dir> list <vpath>");
    eprintln!("  atrium-fileops run --root <dir> move <from> <to>");
    eprintln!("  atrium-fileops run --root <dir> delete <vpath> [--recursive]");
    eprintln!("  atrium-fileops run --root <dir> fixtures");
    eprintln!("  atrium-fileops demo");
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

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

fn mode_run(args: &[String]) -> ExitCode {
    let root = match flag_value(args, "--root") {
        Some(r) => PathBuf::from(r),
        None => {
            eprintln!("run: missing --root <dir>");
            return ExitCode::from(2);
        }
    };
    let show_real = has_flag(args, "--show-real");
    // Positional args: everything that is not a flag or a flag's value.
    let mut positional: Vec<String> = Vec::new();
    let mut skip = false;
    for a in args.iter() {
        if skip {
            skip = false;
            continue;
        }
        if a == "--root" || a == "--content" {
            skip = true;
            continue;
        }
        if a == "--recursive" || a == "--show-real" {
            continue;
        }
        positional.push(a.clone());
    }
    if positional.is_empty() {
        usage();
        return ExitCode::from(2);
    }
    let op = positional[0].as_str();
    let rest = &positional[1..];
    run_one(
        &root,
        op,
        rest,
        flag_value(args, "--content"),
        args,
        show_real,
    )
}

fn real_tail(real: Option<&Path>, show_real: bool) -> String {
    if show_real {
        match real {
            Some(r) => format!("  [{}]", r.display()),
            None => String::new(),
        }
    } else {
        String::new()
    }
}

/// One operation, one line of output. Success names the virtual path asked
/// for; refusal names the reason. Exit 0 on success, 1 on refusal, 2 on
/// usage error.
fn run_one(
    root: &Path,
    op: &str,
    args: &[String],
    content: Option<String>,
    all_args: &[String],
    show_real: bool,
) -> ExitCode {
    let recursive = has_flag(all_args, "--recursive");
    if op == "fixtures" {
        return match build_fixtures(root) {
            Ok(()) => {
                println!("OK fixtures under <root>");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("fixtures: {}", e);
                ExitCode::FAILURE
            }
        };
    }
    macro_rules! got_path {
        () => {
            match args.first().map(|s| VirtualPath::new(s)) {
                Some(Ok(p)) => p,
                Some(Err(e)) => {
                    println!("REFUSE {} — {}", op, e);
                    return ExitCode::from(1);
                }
                None => {
                    eprintln!("run: missing <vpath>");
                    return ExitCode::from(2);
                }
            }
        };
        (i) => {
            match args.get(1).map(|s| VirtualPath::new(s)) {
                Some(Ok(p)) => p,
                Some(Err(e)) => {
                    println!("REFUSE {}", e);
                    return ExitCode::from(1);
                }
                None => {
                    eprintln!("run: missing second path");
                    return ExitCode::from(2);
                }
            }
        };
    }
    match op {
        "create-dir" => {
            let p = got_path!();
            match create_dir(root, &p) {
                Ok(()) => print_ok(&format!("create-dir {}", p), root, &p, show_real),
                Err(e) => print_refuse(&format!("create-dir {}", p), e),
            }
        }
        "write" => {
            let p = got_path!();
            let bytes = match content {
                Some(c) => c.into_bytes(),
                None => {
                    eprintln!("run: write needs --content <text>");
                    return ExitCode::from(2);
                }
            };
            match write_file(root, &p, &bytes) {
                Ok(()) => print_ok(
                    &format!("write {} ({} bytes)", p, bytes.len()),
                    root,
                    &p,
                    show_real,
                ),
                Err(e) => print_refuse(&format!("write {}", p), e),
            }
        }
        "read" => {
            let p = got_path!();
            match read_file(root, &p) {
                Ok(bytes) => {
                    println!("OK read {} length={}", p, bytes.len());
                    // The bytes themselves are printed AFTER the header; a
                    // harness scanning headers never ingests file content,
                    // and file content is never mistaken for a header.
                    std::io::Write::write_all(&mut std::io::stdout(), &bytes).ok();
                    ExitCode::SUCCESS
                }
                Err(e) => print_refuse(&format!("read {}", p), e),
            }
        }
        "list" => {
            let p = got_path!();
            match list_dir(root, &p) {
                Ok(entries) => {
                    for e in &entries {
                        println!("OK list {} {} {}", p, e.kind.as_str(), e.name);
                    }
                    if entries.is_empty() {
                        println!("OK list {} (empty)", p);
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => print_refuse(&format!("list {}", p), e),
            }
        }
        "move" => {
            let from = got_path!();
            let to = got_path!(i);
            match move_path(root, &from, &to) {
                Ok(()) => print_ok(&format!("move {} -> {}", from, to), root, &to, show_real),
                Err(e) => print_refuse(&format!("move {} -> {}", from, to), e),
            }
        }
        "delete" => {
            let p = got_path!();
            match delete_path(root, &p, recursive) {
                Ok(()) => print_ok(
                    &format!(
                        "delete {}{}",
                        p,
                        if recursive { " --recursive" } else { "" }
                    ),
                    root,
                    &p,
                    show_real,
                ),
                Err(e) => print_refuse(
                    &format!(
                        "delete {}{}",
                        p,
                        if recursive { " --recursive" } else { "" }
                    ),
                    e,
                ),
            }
        }
        _ => {
            eprintln!("run: unknown operation `{}`", op);
            usage();
            ExitCode::from(2)
        }
    }
}

fn print_ok(label: &str, root: &Path, p: &VirtualPath, show_real: bool) -> ExitCode {
    let real = if show_real {
        atrium_resolver::resolve(root, p.as_str()).ok()
    } else {
        None
    };
    println!("OK {}{}", label, real_tail(real.as_deref(), show_real));
    ExitCode::SUCCESS
}

fn print_refuse(label: &str, e: FileOpError) -> ExitCode {
    println!("REFUSE {} — {}", label, e);
    ExitCode::from(1)
}

/// Build the attack-list-2a.md "Fixtures" section inside `root`, plus the
/// outside directory with its sentinel. `outside` is a REAL host directory
/// the caller created; this function only links to it, never creates or
/// writes it. (The harness creates the outside directory itself; it is the
/// escape detector.)
pub fn build_fixtures(root: &Path) -> std::io::Result<()> {
    use std::fs;
    use std::os::unix::fs::symlink;

    fs::create_dir_all(root)?;
    let r = |p: &str| root.join(p);

    fs::create_dir_all(r("home/documents/subdir"))?;
    fs::write(r("home/documents/notes.txt"), b"notes\n")?;

    // `outside` is taken from the environment so the harness controls where
    // the escape detector lives. In demo mode it is a sibling temp dir.
    let outside = std::env::var("ATRIUM_FIXTURE_OUTSIDE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.parent().unwrap_or(root).join("atrium-2a-outside"));

    let mk = |name: &str, target: &Path| -> std::io::Result<()> {
        let p = r(name);
        if fs::symlink_metadata(&p).is_err() {
            symlink(target, p)?;
        }
        Ok(())
    };

    mk("outside-link", &outside)?;
    mk("inside-link", &root.join("home/documents"))?;
    mk("rel-outside-link", Path::new("../../outside-dir"))?;
    mk("rel-inside-link", Path::new("home/documents"))?;
    mk("chain-b", &outside)?;
    mk("chain-a", &root.join("chain-b"))?;
    mk("dangling-out", &outside.join("absent.txt"))?;
    mk("dangling-in", &root.join("home/documents/absent.txt"))?;
    mk("dangling-in2", &root.join("home/documents/absent2.txt"))?;

    fs::create_dir_all(r("trap"))?;
    fs::write(r("trap/keepme.txt"), b"keepme\n")?;
    mk("trap/escape", &outside)?;

    fs::create_dir_all(r("loopdir"))?;
    // loopdir/loop-a <-> loopdir/loop-b
    if fs::symlink_metadata(r("loopdir/loop-a")).is_err() {
        symlink(Path::new("loop-b"), r("loopdir/loop-a"))?;
    }
    if fs::symlink_metadata(r("loopdir/loop-b")).is_err() {
        symlink(Path::new("loop-a"), r("loopdir/loop-b"))?;
    }
    // Root-level loop pair too (fixtures section lists them at the root).
    mk("loop-a", &root.join("loop-b"))?;
    mk("loop-b", &root.join("loop-a"))?;

    // F.7: an intermediate component that is a file.
    fs::write(r("link-to-file"), b"not a dir\n")?;

    Ok(())
}

/// The same outside directory the harness will create, so `fixtures` can
/// point links at it. Also creates the sentinel files (harness-side
/// convenience; the independent detector is the harness's own checksums).
pub fn build_outside(outside: &Path) -> std::io::Result<()> {
    use std::fs;
    fs::create_dir_all(outside.join("sub"))?;
    fs::write(outside.join("sentinel.txt"), b"sentinel\n")?;
    fs::write(outside.join("sub/keep.txt"), b"keep\n")?;
    Ok(())
}

fn mode_demo() -> ExitCode {
    let root = std::env::temp_dir().join(format!("atrium-2a-demo-{}", std::process::id()));
    let outside = std::env::temp_dir().join(format!("atrium-2a-demo-out-{}", std::process::id()));
    if root.exists() {
        let _ = std::fs::remove_dir_all(&root);
    }
    if outside.exists() {
        let _ = std::fs::remove_dir_all(&outside);
    }
    if let Err(e) = std::fs::create_dir_all(&root) {
        eprintln!("demo: cannot create temp dir {}: {}", root.display(), e);
        return ExitCode::FAILURE;
    }
    std::env::set_var("ATRIUM_FIXTURE_OUTSIDE", &outside);
    if let Err(e) = build_outside(&outside).and_then(|_| build_fixtures(&root)) {
        eprintln!("demo: fixtures failed: {}", e);
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside);
        return ExitCode::FAILURE;
    }
    println!("demo root: {}", root.display());
    println!("demo outside dir: {}", outside.display());
    println!();

    let sentinel_before = {
        let b = std::fs::read(outside.join("sentinel.txt")).unwrap_or_default();
        format!(
            "{} bytes, first byte {}",
            b.len(),
            b.first().copied().unwrap_or(0)
        )
    };

    let vp = |s: &str| VirtualPath::new(s).expect("demo paths are absolute, constant");
    let mut failures = 0usize;
    let mut show = |label: &str, want_ok: bool, r: Result<String, FileOpError>| {
        let ok = r.is_ok() == want_ok;
        if !ok {
            failures += 1;
        }
        let mark = if ok { "ok  " } else { "FAIL" };
        let detail = match &r {
            Ok(s) => s.clone(),
            Err(e) => e.to_string(),
        };
        println!("[{}] {:<52} {}", mark, label, detail);
    };

    // A. Traversal refuses.
    show(
        "A.1 write /home/../../escaped.txt",
        false,
        write_file(&root, &vp("/home/../../escaped.txt"), b"x").map(|_| "wrote".to_string()),
    );
    show(
        "A.2 create-dir /../escaped-dir",
        false,
        create_dir(&root, &vp("/../escaped-dir")).map(|_| "created".to_string()),
    );
    show(
        "A.8 create-dir /..../deep (dots are a NAME)",
        true,
        create_dir(&root, &vp("/..../deep")).map(|_| "created".to_string()),
    );

    // B. Host-looking paths act inside.
    show(
        "B.1 create-dir /etc/passwd",
        true,
        create_dir(&root, &vp("/etc/passwd")).map(|_| "created <root>/etc/passwd".to_string()),
    );
    show(
        "B.4 delete /etc/passwd --recursive",
        true,
        delete_path(&root, &vp("/etc/passwd"), true)
            .map(|_| "removed <root>/etc/passwd".to_string()),
    );

    // C. Symlink routes out refuse.
    show(
        "C.1 write /outside-link/created.txt",
        false,
        write_file(&root, &vp("/outside-link/created.txt"), b"x").map(|_| "wrote".to_string()),
    );
    show(
        "C.3 read /outside-link/sentinel.txt",
        false,
        read_file(&root, &vp("/outside-link/sentinel.txt")).map(|b| format!("{} bytes", b.len())),
    );
    show(
        "C.12 write /dangling-in/newfile",
        true,
        (|| {
            create_dir(&root, &vp("/dangling-in"))?;
            write_file(&root, &vp("/dangling-in/newfile"), b"x")
        })()
        .map(|_| "wrote inside".to_string()),
    );

    // D. Move both ends.
    show(
        "D.1 move /outside-link/sentinel.txt /home/documents/copied.txt",
        false,
        move_path(
            &root,
            &vp("/outside-link/sentinel.txt"),
            &vp("/home/documents/copied.txt"),
        )
        .map(|_| "moved".to_string()),
    );
    show(
        "D.5 move / /home/movedroot",
        false,
        move_path(&root, &vp("/"), &vp("/home/movedroot")).map(|_| "moved".to_string()),
    );

    // E. Delete: the dangerous one.
    show(
        "E.1 delete /trap --recursive (tree holds a link OUT)",
        true,
        delete_path(&root, &vp("/trap"), true)
            .map(|_| "tree removed; link removed AS link".to_string()),
    );
    show(
        "E.2 delete /loopdir --recursive (link loop; must terminate)",
        true,
        delete_path(&root, &vp("/loopdir"), true).map(|_| "loop removed; no hang".to_string()),
    );
    show(
        "E.3 delete /home/documents (not empty, no flag)",
        false,
        delete_path(&root, &vp("/home/documents"), false).map(|_| "deleted".to_string()),
    );
    show(
        "E.6 delete / --recursive",
        false,
        delete_path(&root, &vp("/"), true).map(|_| "deleted".to_string()),
    );

    // F. Write vs create.
    show(
        "F.2 write /home/documents/no-such-dir/x",
        false,
        write_file(&root, &vp("/home/documents/no-such-dir/x"), b"y").map(|_| "wrote".to_string()),
    );
    show(
        "F.3 create-dir /home/newdir/deep/deeper",
        true,
        create_dir(&root, &vp("/home/newdir/deep/deeper"))
            .map(|_| "created with parents".to_string()),
    );

    // I. The boring half.
    show(
        "I.1+ create /home/work, write, list, move, delete",
        true,
        (|| -> Result<String, FileOpError> {
            create_dir(&root, &vp("/home/work"))?;
            write_file(&root, &vp("/home/work/a.txt"), b"alpha")?;
            write_file(&root, &vp("/home/work/b.txt"), b"beta")?;
            let entries = list_dir(&root, &vp("/home/work"))?;
            if entries.len() != 2 || entries[0].name != "a.txt" || entries[1].name != "b.txt" {
                return Err(FileOpError::Io {
                    path: "/home/work".into(),
                    reason: format!("unexpected listing: {:?}", entries),
                });
            }
            move_path(&root, &vp("/home/work/a.txt"), &vp("/home/work/c.txt"))?;
            let bytes = read_file(&root, &vp("/home/work/c.txt"))?;
            if bytes != b"alpha" {
                return Err(FileOpError::Io {
                    path: "/home/work/c.txt".into(),
                    reason: "contents not alpha".into(),
                });
            }
            delete_path(&root, &vp("/home/work/c.txt"), false)?;
            delete_path(&root, &vp("/home/work"), true)?;
            Ok("create/write/list/move/read/delete inside: all behaved".to_string())
        })(),
    );

    // K. The symlink-object limitation, reproduced deliberately.
    println!();
    println!("-- K. recorded limitation (NOT fixed; see README §K):");
    show(
        "K.1 delete /outside-link refuses (fail-closed)",
        false,
        delete_path(&root, &vp("/outside-link"), false).map(|_| "deleted".to_string()),
    );

    let sentinel_after = {
        let b = std::fs::read(outside.join("sentinel.txt")).unwrap_or_default();
        format!(
            "{} bytes, first byte {}",
            b.len(),
            b.first().copied().unwrap_or(0)
        )
    };
    println!();
    println!(
        "outside sentinel: before '{}' after '{}' ({})",
        sentinel_before,
        sentinel_after,
        if sentinel_before == sentinel_after {
            "UNCHANGED"
        } else {
            failures += 1;
            "!! CHANGED"
        }
    );
    println!("root still exists: {}", root.exists());

    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&outside);
    if failures > 0 {
        println!("demo: {} line(s) FAILED", failures);
        ExitCode::FAILURE
    } else {
        println!("demo: all cases behaved as required");
        ExitCode::SUCCESS
    }
}
