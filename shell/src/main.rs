//! atrium-shell — CLI harness (brief §8: a `run` mode and a `demo` mode,
//! like fileops').
//!
//!   atrium-shell run --root <dir> --cwd <vpath> [--timeout-ms N]
//!                    [--max-output-bytes N] [--show-real] -- program [args...]
//!   atrium-shell demo
//!
//! Output contract (attack-list-2b.md §H): header lines print only the
//! VIRTUAL cwd that was asked for, never the resolved real one. The
//! command's own stdout/stderr bytes are passed through UNALTERED after
//! the headers (§H.4) — including any real paths the command itself
//! printed; redaction would corrupt data and is the runner lying. The
//! resolved location appears only behind `--show-real`, off by default
//! (§H.5).
//!
//! Machine-readable shape:
//!   line 1: `OK status=<exit N|signal N|timed-out> stdout=<n> bytes stderr=<m> bytes[ stdout-truncated][ stderr-truncated]`
//!   then `--- stdout ---` and the exact stdout bytes,
//!   then `--- stderr ---` and the exact stderr bytes.
//! Refusals: one line `REFUSE <cwd> — <reason>`, exit code 1. The runner
//! itself exits 0 whenever the command ran (whatever ITS exit code was;
//! the status line carries it), 1 on refusal, 2 on usage error.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use atrium_shell::{run, ExitStatus, RunOptions, VirtualPath, CHILD_PATH};

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
    eprintln!("  atrium-shell run --root <dir> --cwd <vpath> [--timeout-ms N] [--max-output-bytes N] [--show-real] -- program [args...]");
    eprintln!("  atrium-shell demo");
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

fn mode_run(args: &[String]) -> ExitCode {
    let root = match flag_value(args, "--root") {
        Some(r) => PathBuf::from(r),
        None => {
            eprintln!("run: missing --root <dir>");
            return ExitCode::from(2);
        }
    };
    let cwd_s = match flag_value(args, "--cwd") {
        Some(c) => c,
        None => {
            eprintln!("run: missing --cwd <vpath>");
            return ExitCode::from(2);
        }
    };
    let show_real = args.iter().any(|a| a == "--show-real");

    let mut opts = RunOptions::default();
    if let Some(t) = flag_value(args, "--timeout-ms") {
        match t.parse::<u64>() {
            Ok(ms) => opts.timeout = std::time::Duration::from_millis(ms),
            Err(_) => {
                eprintln!("run: --timeout-ms must be a number");
                return ExitCode::from(2);
            }
        }
    }
    if let Some(m) = flag_value(args, "--max-output-bytes") {
        match m.parse::<usize>() {
            Ok(n) => opts.max_output_bytes = n,
            Err(_) => {
                eprintln!("run: --max-output-bytes must be a number");
                return ExitCode::from(2);
            }
        }
    }

    // Everything after a literal `--` is the program and its argv,
    // passed through UNTOUCHED (no parsing, splitting, quoting — brief
    // §9.3 requirement 8).
    let dashdash = match args.iter().position(|a| a == "--") {
        Some(i) => i,
        None => {
            eprintln!("run: missing `--` before the program");
            return ExitCode::from(2);
        }
    };
    let cmd_args = &args[dashdash + 1..];
    if cmd_args.is_empty() {
        eprintln!("run: missing the program after `--`");
        return ExitCode::from(2);
    }
    let program = cmd_args[0].clone();
    let argv: Vec<String> = cmd_args[1..].to_vec();

    let cwd = match VirtualPath::new(&cwd_s) {
        Ok(p) => p,
        Err(e) => {
            println!("REFUSE {} — {}", cwd_s, e);
            return ExitCode::from(1);
        }
    };

    match run(&root, &cwd, &program, &argv, &opts) {
        Ok(o) => {
            let status_s = match o.status {
                ExitStatus::Exited(c) => format!("exit {}", c),
                ExitStatus::Signalled(s) => format!("signal {}", s),
                ExitStatus::TimedOut => "timed-out".to_string(),
            };
            let trunc = match (o.truncated_stdout, o.truncated_stderr) {
                (true, true) => " stdout-truncated stderr-truncated",
                (true, false) => " stdout-truncated",
                (false, true) => " stderr-truncated",
                (false, false) => "",
            };
            println!(
                "OK status={} stdout={} bytes stderr={} bytes{}",
                status_s,
                o.stdout.len(),
                o.stderr.len(),
                trunc
            );
            if show_real {
                // Debug flag, off by default (§H.5): the resolved location,
                // for the runner's own debugging.
                match atrium_resolver::resolve(&root, cwd.as_str()) {
                    Ok(r) => println!("[real cwd: {}]", r.display()),
                    Err(_) => {}
                }
            }
            let mut out = std::io::stdout();
            let _ = out.write_all(b"--- stdout ---\n");
            let _ = out.write_all(&o.stdout);
            let _ = out.write_all(b"\n--- stderr ---\n");
            let _ = out.write_all(&o.stderr);
            let _ = out.flush();
            ExitCode::SUCCESS
        }
        Err(e) => {
            println!("REFUSE {} — {}", cwd, e);
            ExitCode::from(1)
        }
    }
}

/// A hands-on walk through the phase's promise, in a throwaway root.
/// Both directories are deleted at the end; nothing else is touched.
fn mode_demo() -> ExitCode {
    let root = std::env::temp_dir().join(format!("atrium-2b-demo-{}", std::process::id()));
    let outside = std::env::temp_dir().join(format!("atrium-2b-demo-out-{}", std::process::id()));
    if root.exists() {
        let _ = std::fs::remove_dir_all(&root);
    }
    if outside.exists() {
        let _ = std::fs::remove_dir_all(&outside);
    }
    if std::fs::create_dir_all(root.join("home/work")).is_err()
        || std::fs::create_dir_all(root.join("home/documents/sub")).is_err()
        || std::fs::write(root.join("home/documents/notes.txt"), b"hi\n").is_err()
        || std::fs::create_dir_all(outside.join("sub")).is_err()
        || std::fs::write(outside.join("sentinel.txt"), b"sentinel\n").is_err()
        || std::fs::write(outside.join("sub/keep.txt"), b"keep\n").is_err()
    {
        eprintln!("demo: could not build throwaway fixtures");
        return ExitCode::FAILURE;
    }
    println!("demo root: {}", root.display());
    println!("demo outside dir: {}", outside.display());
    println!();

    let vp = |s: &str| VirtualPath::new(s).expect("demo paths are absolute, constant");
    let sh = |line: &str| vec!["sh".to_string(), "-c".to_string(), line.to_string()];
    let prog = |p: &str, a: &[&str]| {
        let mut v = vec![p.to_string()];
        v.extend(a.iter().map(|s| s.to_string()));
        v
    };
    let failures = std::cell::Cell::new(0usize);
    let show = |label: &str, cwd: &str, cmd: Vec<String>, want_ok: bool| {
        let args: Vec<String> = cmd[1..].to_vec();
        let r = run(&root, &vp(cwd), &cmd[0], &args, &RunOptions::default());
        let ok = r.is_ok() == want_ok;
        if !ok {
            failures.set(failures.get() + 1);
        }
        let mark = if ok { "ok  " } else { "FAIL" };
        match r {
            Ok(o) => {
                let mut so = String::from_utf8_lossy(&o.stdout).into_owned();
                if so.len() > 60 {
                    so.truncate(60);
                    so.push('…');
                }
                println!(
                    "[{}] {:<50} {} (stdout: {:?})",
                    mark,
                    label,
                    o.status,
                    so.trim_end()
                );
            }
            Err(e) => println!("[{}] {:<50} {}", mark, label, e),
        }
    };

    // A. Where the command starts.
    show("A.1 pwd in /home/work", "/home/work", sh("pwd"), true);
    show("A.4 pwd at virtual /", "/", sh("pwd"), true);
    show(
        "A.5 touch ../sibling.txt",
        "/home/work",
        sh("touch ../sibling.txt"),
        true,
    );
    println!(
        "      on disk: {}/home/sibling.txt exists: {}",
        "<root>",
        root.join("home/sibling.txt").exists()
    );
    show(
        "A.6 deeper cwd /home/documents/sub",
        "/home/documents/sub",
        sh("pwd"),
        true,
    );

    // B. Refusals — nothing starts.
    show(
        "B.1 cwd /home/../../work",
        "/home/../../work",
        sh("touch marker"),
        false,
    );
    show(
        "B.2 cwd /nonexistent/work",
        "/nonexistent/work",
        sh("touch marker"),
        false,
    );
    show("B.5 cwd (empty)", "", sh("touch marker"), false);
    show(
        "B.6 cwd home/work (relative)",
        "home/work",
        sh("touch marker"),
        false,
    );

    // B.3: a file as a cwd — build it first.
    let _ = std::fs::write(root.join("afile.txt"), b"x\n");
    show(
        "B.3 cwd /afile.txt",
        "/afile.txt",
        sh("touch marker"),
        false,
    );

    // C. Output and exit code unaltered.
    show("C.1 stdout only", "/home/work", prog("echo", &["hi"]), true);
    show("C.2 stderr only", "/home/work", sh("echo oops >&2"), true);
    show("C.4 exit 42", "/home/work", sh("exit 42"), true);
    show(
        "C.6 killed by SIGKILL",
        "/home/work",
        sh("kill -9 $$"),
        true,
    );
    show(
        "C.8 no trailing newline",
        "/home/work",
        prog("printf", &["x"]),
        true,
    );

    // D. The environment.
    std::env::set_var("ATRIUM_2B_CANARY", "demo-canary-value");
    let args = sh("if [ -n \"${ATRIUM_2B_CANARY:-}\" ]; then echo LEAKED; else echo absent; fi");
    let r = run(
        &root,
        &vp("/home/work"),
        &args[0],
        &args[1..].to_vec(),
        &RunOptions::default(),
    );
    match r {
        Ok(o) => {
            let s = String::from_utf8_lossy(&o.stdout);
            let ok = s.trim() == "absent";
            if !ok {
                failures.set(failures.get() + 1);
            }
            println!(
                "[{}] {:<50} canary in child env: {}",
                if ok { "ok  " } else { "FAIL" },
                "D.1 environment canary",
                s.trim()
            );
        }
        Err(e) => {
            failures.set(failures.get() + 1);
            println!("[FAIL] D.1 environment canary — {}", e);
        }
    }
    println!(
        "      (child PATH is fixed: {}, HOME/TMPDIR = <root>)",
        CHILD_PATH
    );

    // E. stdin is at EOF.
    show("E.1 cat returns immediately", "/home/work", sh("cat"), true);

    // F. Timeout: a command that never exits is stopped and reported.
    {
        let mut o = RunOptions::default();
        o.timeout = std::time::Duration::from_millis(500);
        let args = sh("while true; do :; done");
        let r = run(&root, &vp("/home/work"), &args[0], &args[1..].to_vec(), &o);
        match r {
            Ok(out) => {
                let ok = out.status == ExitStatus::TimedOut;
                if !ok {
                    failures.set(failures.get() + 1);
                }
                println!(
                    "[{}] {:<50} {}",
                    if ok { "ok  " } else { "FAIL" },
                    "F.1 never-exits is stopped at the limit",
                    out.status
                );
            }
            Err(e) => {
                failures.set(failures.get() + 1);
                println!("[FAIL] F.1 — {}", e);
            }
        }
    }

    println!();
    println!("demo: the runner does NOT cage the command. `sh -c 'cd / && pwd'` run");
    println!("here would print the host's `/` — that hole is BUILD-PLAN.md §2e's, and");
    println!("attack-list-2b.md §G demonstrates it in the hands-on harness.");

    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&outside);

    println!();
    if failures.get() > 0 {
        println!("demo: {} FAILURE(S)", failures.get());
        ExitCode::FAILURE
    } else {
        println!("demo: every line behaved as described");
        ExitCode::SUCCESS
    }
}
