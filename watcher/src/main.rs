//! `atrium-watcher` — the Phase 2d command line.
//!
//! Exit codes: `0` a clean run, `1` a refusal **or a run that lost events / hit an
//! error**, `2` usage error.
//!
//! ```
//! atrium-watcher watch --root <dir> [--duration-ms N] [--settle-ms N] [--show-real]
//! atrium-watcher demo
//! atrium-watcher fixtures --root <dir> [--outside <dir>]
//! ```
//!
//! **Exit 1 on any fault is deliberate** (`attack-list-2d.md` §J.2, §K.7). A run
//! that dropped events, lost a watch, or saw something outside the root is not a
//! clean run, and the harness depends on that: it means a green exit cannot be
//! produced by a watcher that quietly missed things.
//!
//! **The default output names virtual paths only.** `DESIGN.md` §3.1: the agent
//! never learns the real path exists. `--show-real` is the documented exception,
//! matching `shell/`'s flag. A *refusal* does name the real path on purpose — the
//! reader is the user, who must know which directory was refused (the `snapshot/`
//! precedent, `DECISIONS.md`).

use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use atrium_watcher::{Options, WatchError, Watcher};

const USAGE: &str = "\
atrium-watcher — watch the environment root and report what changed (Phase 2d)

USAGE:
  atrium-watcher watch --root <dir> [--duration-ms N] [--settle-ms N] [--show-real]
  atrium-watcher demo
  atrium-watcher fixtures --root <dir> [--outside <dir>]

EXIT CODES:
  0  clean run
  1  refusal, or a run that lost events or reported an error
  2  usage error

WHAT THIS IS:
  A watcher, not a gate. It reports changes made inside the environment root --
  by shell commands or by anything else -- and it decides nothing, blocks nothing
  and writes nothing.

WHAT IT CANNOT SEE (printed at the end of every run):
  * Changes made to an in-root file through a hard link outside the root.
  * A file written into a brand-new directory before the watch is attached.
  * Anything at all once the kernel's event queue overflows; that is reported as
    OVERFLOW rather than guessed at.
  * WHO made a change: a shell command and Atrium's own file operations look
    identical here. De-duplication is Phase 3's job.

By default it prints VIRTUAL paths (the agent's view). --show-real prints real host
paths, for a person reading the output.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("atrium-watcher: {e}");
            ExitCode::from(e.exit_code())
        }
    }
}

fn run(args: &[String]) -> Result<u8, WatchError> {
    let mut it = args.iter();
    let cmd = match it.next() {
        Some(c) => c.clone(),
        None => {
            println!("{USAGE}");
            return Ok(0);
        }
    };
    match cmd.as_str() {
        "watch" => cmd_watch(it.cloned().collect()),
        "demo" => cmd_demo(),
        "fixtures" => cmd_fixtures(it.cloned().collect()),
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(0)
        }
        other => Err(WatchError::Usage(format!(
            "unrecognised command '{other}'. Try `atrium-watcher --help`."
        ))),
    }
}

/// A small argument reader that refuses anything it does not understand, rather
/// than silently ignoring it.
struct Args {
    root: Option<PathBuf>,
    outside: Option<PathBuf>,
    duration_ms: Option<u64>,
    settle_ms: Option<u64>,
    show_real: bool,
}

fn parse_args(raw: &[String]) -> Result<Args, WatchError> {
    let mut a = Args {
        root: None,
        outside: None,
        duration_ms: None,
        settle_ms: None,
        show_real: false,
    };
    let mut i = 0;
    while i < raw.len() {
        let k = raw[i].as_str();
        let mut take = |name: &str| -> Result<String, WatchError> {
            i += 1;
            raw.get(i)
                .cloned()
                .ok_or_else(|| WatchError::Usage(format!("{name} needs a value")))
        };
        match k {
            "--root" => a.root = Some(PathBuf::from(take("--root")?)),
            "--outside" => a.outside = Some(PathBuf::from(take("--outside")?)),
            "--duration-ms" => {
                let v = take("--duration-ms")?;
                a.duration_ms = Some(v.parse().map_err(|_| {
                    WatchError::Usage(format!("--duration-ms must be a whole number, got '{v}'"))
                })?);
            }
            "--settle-ms" => {
                let v = take("--settle-ms")?;
                a.settle_ms = Some(v.parse().map_err(|_| {
                    WatchError::Usage(format!("--settle-ms must be a whole number, got '{v}'"))
                })?);
            }
            "--show-real" => a.show_real = true,
            other => {
                return Err(WatchError::Usage(format!(
                    "unrecognised argument '{other}'. Try `atrium-watcher --help`."
                )))
            }
        }
        i += 1;
    }
    Ok(a)
}

// ---------------------------------------------------------------------------
// watch
// ---------------------------------------------------------------------------

fn cmd_watch(raw: Vec<String>) -> Result<u8, WatchError> {
    let a = parse_args(&raw)?;
    let root = a.root.ok_or_else(|| {
        WatchError::Usage("watch needs --root <dir> (an absolute path)".to_string())
    })?;

    let mut opts = Options::new(root);
    opts.show_real = a.show_real;
    opts.settle_ms = a.settle_ms.unwrap_or(150);

    let mut w = Watcher::start(&opts)?;

    // **Observed defect, found by this phase's own harness.** This line used to
    // print the root's REAL on-disk path, which `DESIGN.md` §3.1 forbids and
    // `attack-list-2d.md` §H.1 checks for on every line. The default output is the
    // agent's view, so the root is `/`; the real path appears only under
    // `--show-real`, which exists for the person reading the tool.
    let root_shown = if opts.show_real {
        w.root().display().to_string()
    } else {
        "/".to_string()
    };
    println!("# watching {root_shown} ({} directories)", w.live_watches());
    if w.live_watches() == 0 {
        // Not exit 0 quietly: a watcher with no watches can see nothing, and
        // saying nothing would look exactly like an idle tree (§J.8).
        eprintln!(
            "atrium-watcher: no watches could be placed on {} — this watcher can see nothing",
            w.root().display()
        );
    }

    let settle = Duration::from_millis(opts.settle_ms);
    let deadline = a
        .duration_ms
        .map(|ms| Instant::now() + Duration::from_millis(ms));

    let mut faults = 0usize;
    let mut total = 0usize;
    loop {
        let batch = w.drain();
        for c in &batch {
            if c.is_fault() {
                faults += 1;
            }
            total += 1;
            println!("{}", c.line());
        }
        if let Some(d) = deadline {
            if Instant::now() >= d {
                break;
            }
        }
        std::thread::sleep(settle);
    }

    println!("# {} records, {} of them faults", total, faults);

    // The statements, printed in every run so the boundary is visible where the
    // result is read (attack-list-2d.md §I).
    println!("# cannot see: a change to an in-root file made through a hard link outside the root");
    println!(
        "# cannot see: a file written into a brand-new directory before its watch was attached"
    );
    println!("# cannot see: anything after the kernel's queue overflows (reported as OVERFLOW)");
    println!("# cannot tell: who made a change — a shell command and Atrium's own file operations look identical here (Phase 3 de-duplicates)");
    println!(
        "# emits no effects, gates nothing, blocks nothing, writes nothing (Phase 3 owns effects)"
    );

    // A run with any fault is not clean.
    Ok(if faults > 0 { 1 } else { 0 })
}

// ---------------------------------------------------------------------------
// demo — a self-contained scenario against its own throwaway root
// ---------------------------------------------------------------------------

fn cmd_demo() -> Result<u8, WatchError> {
    let root = std::env::temp_dir().join(format!("atrium-2d-demo-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("home/documents/sub")).map_err(|e| WatchError::Root {
        path: root.clone(),
        detail: e.to_string(),
    })?;
    std::fs::write(root.join("home/documents/notes.txt"), b"first\n").map_err(|e| {
        WatchError::Root {
            path: root.clone(),
            detail: e.to_string(),
        }
    })?;

    let mut w = Watcher::start(&Options::new(&root))?;
    // The demo prints its own throwaway root's path on purpose: it is a
    // self-contained scenario whose reader is the person running it, and it makes
    // no claim about the sandbox boundary. Stated so the difference from `watch`
    // is deliberate rather than an inconsistency.
    println!(
        "# demo root {} ({} directories watched)",
        root.display(),
        w.live_watches()
    );

    let mut faults = 0usize;

    let show = |w: &mut Watcher, label: &str, faults: &mut usize| {
        println!("--- {label}");
        for c in w.drain() {
            if c.is_fault() {
                *faults += 1;
            }
            println!("{}", c.line());
        }
    };

    // 1. A shell command creates a file — the plan's own verification step.
    let st = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!(
            "echo hello > '{0}/home/documents/by-command.txt'",
            root.display()
        ))
        .status()
        .map_err(|e| WatchError::Root {
            path: root.clone(),
            detail: e.to_string(),
        })?;
    let _ = st;
    std::thread::sleep(Duration::from_millis(200));
    show(
        &mut w,
        "a command created home/documents/by-command.txt",
        &mut faults,
    );

    // 2. Three writes to one file: one record, not three.
    for i in 0..3 {
        std::fs::write(root.join("home/documents/notes.txt"), format!("edit {i}\n")).ok();
    }
    std::thread::sleep(Duration::from_millis(200));
    show(
        &mut w,
        "three writes to notes.txt (expect ONE MODIF)",
        &mut faults,
    );

    // 3. A move, paired by cookie.
    std::fs::rename(
        root.join("home/documents/by-command.txt"),
        root.join("home/documents/sub/moved.txt"),
    )
    .ok();
    std::thread::sleep(Duration::from_millis(200));
    show(
        &mut w,
        "moved by-command.txt into sub/ (expect a matched cookie pair)",
        &mut faults,
    );

    // 4. A directory created, then written into.
    std::fs::create_dir(root.join("home/fresh")).ok();
    std::thread::sleep(Duration::from_millis(150));
    std::fs::write(root.join("home/fresh/inside.txt"), b"x").ok();
    std::thread::sleep(Duration::from_millis(200));
    show(
        &mut w,
        "created home/fresh, then wrote inside it",
        &mut faults,
    );

    // 5. A read is not a change.
    let _ = std::fs::read(root.join("home/documents/notes.txt"));
    let _ = std::fs::metadata(root.join("home/documents/notes.txt"));
    std::thread::sleep(Duration::from_millis(150));
    show(
        &mut w,
        "read and stat notes.txt (expect NOTHING: a read is not a change)",
        &mut faults,
    );

    // 6. Delete.
    std::fs::remove_file(root.join("home/documents/sub/moved.txt")).ok();
    std::thread::sleep(Duration::from_millis(200));
    show(&mut w, "deleted sub/moved.txt", &mut faults);

    let _ = std::fs::remove_dir_all(&root);
    println!("# demo done ({faults} faults)");
    Ok(if faults > 0 { 1 } else { 0 })
}

// ---------------------------------------------------------------------------
// fixtures — the harness's setup, in one place
// ---------------------------------------------------------------------------

fn cmd_fixtures(raw: Vec<String>) -> Result<u8, WatchError> {
    let a = parse_args(&raw)?;
    let root = a
        .root
        .ok_or_else(|| WatchError::Usage("fixtures needs --root <dir>".to_string()))?;
    let outside = a.outside.unwrap_or_else(|| {
        root.parent()
            .unwrap_or(Path::new("/tmp"))
            .join("atrium-2d-outside")
    });

    let fail = |p: &Path, e: std::io::Error| WatchError::Root {
        path: p.to_path_buf(),
        detail: e.to_string(),
    };

    std::fs::create_dir_all(root.join("home/documents/sub/deep")).map_err(|e| fail(&root, e))?;
    std::fs::write(root.join("home/documents/notes.txt"), b"first line\n")
        .map_err(|e| fail(&root, e))?;
    std::fs::write(root.join("home/documents/sub/deep/leaf.txt"), b"deep\n")
        .map_err(|e| fail(&root, e))?;
    std::fs::create_dir_all(&outside).map_err(|e| fail(&outside, e))?;
    std::fs::write(outside.join("sentinel.txt"), b"sentinel\n").map_err(|e| fail(&outside, e))?;
    std::fs::write(outside.join("keep.txt"), b"keep\n").map_err(|e| fail(&outside, e))?;

    // A symlink pointing OUT of the root, and a dangling one. Both are fixtures
    // the watcher must not follow (§D.4, §D.5).
    let link = root.join("home/link-to-outside");
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(&outside, &link).map_err(|e| fail(&link, e))?;
    let dangling = root.join("home/dangling");
    let _ = std::fs::remove_file(&dangling);
    std::os::unix::fs::symlink("/nowhere/at/all", &dangling).map_err(|e| fail(&dangling, e))?;

    println!(
        "fixtures: root={} outside={}",
        root.display(),
        outside.display()
    );
    Ok(0)
}

/// Only used by `fixtures` when it needs a byte-exact name.
#[allow(dead_code)]
fn os_from_bytes(b: &[u8]) -> OsString {
    OsStr::from_bytes(b).to_os_string()
}
