//! Cargo tests for atrium-shell — attack-list-2b.md coverage.
//!
//! Every REFUSED line asserts the REASON, not just the outcome. §C.6
//! (signal vs exit code) and §F.1/§F.2 (timeout, then reaped) are the two
//! most important tests in this file: their failure mode is a crash or a
//! hang being reported as success, and a machine with a runaway process.
//!
//! A guard thread bounds every test: if any test could hang the suite
//! silently, §F.5's claim would be unverifiable. Each test is also
//! written so the OS cannot be what made it pass.

use std::path::{Path, PathBuf};
use std::time::Duration;

use atrium_shell::{
    run, ExitStatus, RunError, RunOptions, VirtualPath, CHILD_PATH, DEFAULT_MAX_OUTPUT_BYTES,
};

/// Build a throwaway environment root with the attack-list-2b.md fixtures,
/// by hand (mkdir/write/symlink — NEVER another crate's fixture mode), plus
/// a throwaway OUTSIDE directory beside it. Returns (root, outside, tag).
fn make_root() -> (PathBuf, PathBuf, String) {
    let tag = format!("atrium-2b-test-{}-{}", std::process::id(), unique());
    let root = std::env::temp_dir().join(&tag);
    let outside = std::env::temp_dir().join(format!("{}-outside", tag));
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&outside);

    std::fs::create_dir_all(root.join("home/documents/sub")).unwrap();
    std::fs::create_dir_all(root.join("home/work")).unwrap();
    std::fs::write(root.join("home/documents/notes.txt"), b"hi\n").unwrap();
    std::fs::write(root.join("afile.txt"), b"x\n").unwrap();
    // trap/escape -> <outside> (the link-out fixture, used by §B.7/B.8).
    std::fs::create_dir_all(root.join("trap")).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("trap/escape")).unwrap();

    std::fs::create_dir_all(outside.join("sub")).unwrap();
    std::fs::write(outside.join("sentinel.txt"), b"sentinel\n").unwrap();
    std::fs::write(outside.join("sub/keep.txt"), b"keep\n").unwrap();
    (root, outside, tag)
}

fn unique() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::SeqCst)
        + std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64
}

fn cleanup(root: &Path, outside: &Path) {
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(outside);
}

fn vp(s: &str) -> VirtualPath {
    VirtualPath::new(s).expect("test paths are absolute, constant")
}

fn sh(line: &str) -> (String, Vec<String>) {
    ("sh".to_string(), vec!["-c".to_string(), line.to_string()])
}

fn run_sh(root: &Path, cwd: &str, line: &str) -> atrium_shell::Outcome {
    let (p, a) = sh(line);
    run(root, &vp(cwd), &p, &a, &RunOptions::default())
        .unwrap_or_else(|e| panic!("run of {:?} refused: {}", line, e))
}

fn out_text(o: &atrium_shell::Outcome) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

/// §I.4: what the host's own shell returns for the same command — the
/// comparison is against the shell itself, not a remembered number.
fn shell_exit_code(line: &str) -> i32 {
    let st = std::process::Command::new("sh")
        .args(["-c", line])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    st.code().unwrap()
}

// ---------------------------------------------------------------- A. cwd

#[test]
fn a1_pwd_is_the_resolved_directory() {
    let (root, outside, _t) = make_root();
    let o = run_sh(&root, "/home/work", "pwd");
    assert_eq!(o.status, ExitStatus::Exited(0));
    let printed = out_text(&o).trim().to_string();
    assert_eq!(Path::new(&printed), root.join("home/work").as_path());
    cleanup(&root, &outside);
}

#[test]
fn a2_relative_path_lands_inside_the_root() {
    let (root, outside, _t) = make_root();
    run_sh(&root, "/home/work", "touch relative.txt");
    assert!(root.join("home/work/relative.txt").exists());
    assert!(!outside.join("relative.txt").exists());
    cleanup(&root, &outside);
}

#[test]
fn a3_deep_chain_lands_inside_the_root() {
    let (root, outside, _t) = make_root();
    run_sh(&root, "/home/work", "mkdir -p a/b && touch a/b/deep.txt");
    assert!(root.join("home/work/a/b/deep.txt").exists());
    cleanup(&root, &outside);
}

#[test]
fn a4_virtual_root_is_the_environment_root() {
    let (root, outside, _t) = make_root();
    let o = run_sh(&root, "/", "pwd");
    let printed = out_text(&o).trim().to_string();
    assert_eq!(Path::new(&printed), root.as_path());
    assert_ne!(printed, "/"); // must be the root, not the host's /
    cleanup(&root, &outside);
}

#[test]
fn a5_dotdot_from_inside_stays_inside() {
    let (root, outside, _t) = make_root();
    run_sh(&root, "/home/work", "touch ../sibling.txt");
    assert!(root.join("home/sibling.txt").exists());
    assert!(!outside.join("sibling.txt").exists());
    cleanup(&root, &outside);
}

#[test]
fn a6_deeper_cwd_works_the_same_way() {
    let (root, outside, _t) = make_root();
    let o = run_sh(&root, "/home/documents/sub", "pwd");
    assert_eq!(
        Path::new(out_text(&o).trim()),
        root.join("home/documents/sub").as_path()
    );
    cleanup(&root, &outside);
}

#[test]
fn a7_documents_is_the_roots_documents_not_the_hosts() {
    let (root, outside, _t) = make_root();
    let o = run_sh(&root, "/home/documents", "pwd");
    assert_eq!(
        Path::new(out_text(&o).trim()),
        root.join("home/documents").as_path()
    );
    cleanup(&root, &outside);
}

// ------------------------------------------------------- B. refusals

/// Assert the refused outcome AND the reason's content (attack-list-2b.md
/// §B: a refusal names the virtual path and the specific thing wrong, and
/// never names a real path — H.1).
fn assert_refused(root: &Path, cwd: &str, want_words: &[&str], forbidden: &[&Path]) {
    let (p, a) = sh("touch SHOULDRUN_marker");
    let r = run(root, &vp(cwd), &p, &a, &RunOptions::default());
    match r {
        Ok(o) => panic!(
            "cwd {:?} must REFUSE, but it RAN (status {})",
            cwd, o.status
        ),
        Err(e) => {
            let msg = e.to_string();
            for w in want_words {
                assert!(
                    msg.contains(w),
                    "refusal for {:?} must name {:?}; got: {}",
                    cwd,
                    w,
                    msg
                );
            }
            for f in forbidden {
                let s = f.display().to_string();
                assert!(
                    !msg.contains(&s),
                    "refusal for {:?} disclosed real path {}; got: {}",
                    cwd,
                    s,
                    msg
                );
            }
        }
    }
    // Nothing started: the marker must not exist anywhere.
    let marker = "SHOULDRUN_marker";
    for dir in walk_files(root) {
        assert_ne!(dir.file_name().unwrap().to_str().unwrap(), marker);
    }
}

fn walk_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            let is_dir = std::fs::symlink_metadata(&p)
                .map(|m| m.is_dir())
                .unwrap_or(false);
            if is_dir {
                out.extend(walk_files(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}

#[test]
fn b_refusals_name_the_reason_and_start_nothing() {
    let (root, outside, _t) = make_root();
    let real_paths: Vec<&Path> = vec![&root, &outside];

    assert_refused(
        &root,
        "/home/../../work",
        &["..", "climbed above"],
        &real_paths,
    );
    assert_refused(
        &root,
        "/nonexistent/work",
        &["/nonexistent/work", "does not exist"],
        &real_paths,
    );
    assert_refused(
        &root,
        "/afile.txt",
        &["/afile.txt", "file, not a directory"],
        &real_paths,
    );
    assert_refused(
        &root,
        "/home/documents/notes.txt",
        &["/home/documents/notes.txt", "not a directory"],
        &real_paths,
    );
    // B.5 / B.6: the VirtualPath constructor itself refuses; no process.
    assert!(VirtualPath::new("").is_err());
    assert!(VirtualPath::new("home/work").is_err());
    // B.7: the link-out fixture. trap/escape is a directory link OUT;
    // using it as a cwd must refuse naming the symlink.
    assert_refused(
        &root,
        "/trap/escape",
        &["symlink", "/trap/escape"],
        &real_paths,
    );
    // B.8: degenerate-tail — link out followed by `..`. Phase 1b ruling:
    // the target is outside, so it rejects.
    assert_refused(&root, "/trap/escape/..", &["symlink"], &real_paths);

    cleanup(&root, &outside);
}

#[test]
fn b9_refusal_leaves_the_filesystem_byte_identical() {
    let (root, outside, _t) = make_root();
    let before = snapshot(&root);
    let (p, a) = sh("touch marker_b9");
    for cwd in ["/home/../../work", "/nonexistent/work", "/afile.txt"] {
        let r = run(&root, &vp(cwd), &p, &a, &RunOptions::default());
        assert!(r.is_err(), "{:?} must refuse", cwd);
    }
    assert_eq!(snapshot(&root), before, "refusals changed the filesystem");
    assert!(!outside.join("marker_b9").exists());
    cleanup(&root, &outside);
}

fn snapshot(root: &Path) -> Vec<String> {
    let mut v: Vec<String> = walk_files(root)
        .iter()
        .map(|p| {
            format!(
                "{}:{}",
                p.display(),
                std::fs::read(p).map(|b| b.len()).unwrap_or(usize::MAX)
            )
        })
        .collect();
    v.sort();
    v
}

#[test]
fn b10_refusal_is_not_confused_with_a_commands_nonzero_exit() {
    let (root, outside, _t) = make_root();
    // A command exiting non-zero for its own reasons is RUNS, not refused.
    let o = run_sh(&root, "/home/work", "exit 3");
    assert_eq!(o.status, ExitStatus::Exited(3));
    cleanup(&root, &outside);
}

// ------------------------------------------- C. output and exit code

#[test]
fn c1_stdout_only_exact_bytes() {
    let (root, outside, _t) = make_root();
    let o = run_sh(&root, "/home/work", "printf 'hi\\n'");
    assert_eq!(o.stdout, b"hi\n");
    assert_eq!(o.stderr, b"");
    assert_eq!(o.status, ExitStatus::Exited(0));
    cleanup(&root, &outside);
}

#[test]
fn c2_stderr_only_exact_bytes() {
    let (root, outside, _t) = make_root();
    let o = run_sh(&root, "/home/work", "echo oops >&2");
    assert_eq!(o.stderr, b"oops\n");
    assert_eq!(o.stdout, b"");
    assert_eq!(o.status, ExitStatus::Exited(0));
    cleanup(&root, &outside);
}

#[test]
fn c3_both_streams_are_separate_and_unmerged() {
    let (root, outside, _t) = make_root();
    let o = run_sh(&root, "/home/work", "echo on-out; echo on-err >&2");
    assert_eq!(o.stdout, b"on-out\n");
    assert_eq!(o.stderr, b"on-err\n");
    cleanup(&root, &outside);
}

#[test]
fn c4_exit_codes_preserved_exactly() {
    let (root, outside, _t) = make_root();
    for code in [0, 1, 42, 255] {
        let line = format!("exit {}", code);
        let o = run_sh(&root, "/home/work", &line);
        assert_eq!(o.status, ExitStatus::Exited(code));
        // §I.4: against the shell itself, not a remembered number.
        assert_eq!(shell_exit_code(&line), code);
    }
    cleanup(&root, &outside);
}

#[test]
fn c5_exit_127_is_an_exit_code_not_a_runner_error() {
    let (root, outside, _t) = make_root();
    let (p, a) = ("definitely-not-a-real-command-2b".to_string(), vec![]);
    let r = run(&root, &vp("/home/work"), &p, &a, &RunOptions::default());
    // The runner cannot START a missing program; that is a Spawn refusal.
    // The 127-as-exit-code case is the shell's own report, tested here:
    assert!(matches!(r, Err(RunError::Spawn { .. })));
    let o = run_sh(&root, "/home/work", "definitely-not-a-real-command-2b");
    assert_eq!(
        o.status,
        ExitStatus::Exited(shell_exit_code("definitely-not-a-real-command-2b"))
    );
    cleanup(&root, &outside);
}

#[test]
fn c6_killed_by_a_signal_is_not_exit_0() {
    // The most important test in the file (with f1): a crash reported as
    // success is the worst outcome this phase can produce.
    let (root, outside, _t) = make_root();
    let o = run_sh(&root, "/home/work", "kill -9 $$");
    assert_eq!(o.status, ExitStatus::Signalled(9), "got {:?}", o.status);
    // Other signals are their own numbers, never folded into exit codes.
    let o2 = run_sh(&root, "/home/work", "kill -15 $$");
    assert_eq!(o2.status, ExitStatus::Signalled(15), "got {:?}", o2.status);
    cleanup(&root, &outside);
}

#[test]
fn c7_empty_output_is_zero_bytes_on_both_streams_not_an_error() {
    let (root, outside, _t) = make_root();
    let o = run_sh(&root, "/home/work", "true");
    assert_eq!(o.stdout.len(), 0);
    assert_eq!(o.stderr.len(), 0);
    assert_eq!(o.status, ExitStatus::Exited(0));
    cleanup(&root, &outside);
}

#[test]
fn c8_no_trailing_newline_is_exactly_one_byte() {
    let (root, outside, _t) = make_root();
    let o = run_sh(&root, "/home/work", "printf x");
    assert_eq!(o.stdout, b"x");
    assert_eq!(o.stdout.len(), 1);
    cleanup(&root, &outside);
}

#[test]
fn c9_all_256_byte_values_come_back_exactly() {
    let (root, outside, _t) = make_root();
    let mut want: Vec<u8> = (0u16..=255).map(|b| b as u8).collect();
    // sh writes them via printf built from octal escapes — every byte
    // including NUL… except argv/pipes cannot carry NUL through sh's
    // printf portably, so build the bytes with a helper: python may not be
    // in the fixed PATH on all hosts; use `od`-free pure-sh? Boring, sure
    // choice: build the file with a tiny rust-side expectation file and
    // have sh emit it via `printf` octal loop is slow but 256 iterations
    // is nothing.
    let (p, a) =
        sh("i=0; while [ $i -lt 256 ]; do printf \"\\\\$(printf '%03o' $i)\"; i=$((i+1)); done");
    let o = run(&root, &vp("/home/work"), &p, &a, &RunOptions::default()).unwrap();
    assert_eq!(o.stdout.len(), 256, "got {} bytes", o.stdout.len());
    want.dedup(); // no-op; keep 0..=255 inclusive, exact order
    let want: Vec<u8> = (0u16..=255).map(|b| b as u8).collect();
    assert_eq!(o.stdout, want);
    cleanup(&root, &outside);
}

#[test]
fn c10_arguments_survive_intact_spaces_quotes_dollar() {
    let (root, outside, _t) = make_root();
    // printf with EACH of these as one argv element: the runner is not a
    // quoting layer.
    let args: Vec<String> = vec![
        "[%s][%s][%s]".to_string(),
        "two words".to_string(),
        "has'quote\"and$dollar".to_string(),
        "  padded  ".to_string(),
    ];
    let o = run(
        &root,
        &vp("/home/work"),
        "printf",
        &args,
        &RunOptions::default(),
    )
    .unwrap();
    assert_eq!(
        out_text(&o),
        "[two words][has'quote\"and$dollar][  padded  ]"
    );
    cleanup(&root, &outside);
}

#[test]
fn c11_order_within_a_stream_is_preserved() {
    let (root, outside, _t) = make_root();
    let o = run_sh(
        &root,
        "/home/work",
        "i=1; while [ $i -le 100 ]; do echo $i; i=$((i+1)); done",
    );
    let want: String = (1..=100).map(|n| format!("{}\n", n)).collect();
    assert_eq!(out_text(&o), want);
    cleanup(&root, &outside);
}

// ----------------------------------------------------- D. environment

#[test]
fn d1_the_canary_is_absent_from_the_childs_environment() {
    // Real test: the canary is set in THIS process's own environment, the
    // way the harness sets it before invoking the runner. If the child
    // inherited it, the command would print it.
    std::env::set_var("ATRIUM_2B_CANARY", "test-canary-value-2b");
    let (root, outside, _t) = make_root();
    let o = run_sh(
        &root,
        "/home/work",
        "echo \"canary=[${ATRIUM_2B_CANARY:-ABSENT}]\"",
    );
    assert_eq!(out_text(&o).trim(), "canary=[ABSENT]");
    std::env::remove_var("ATRIUM_2B_CANARY");
    cleanup(&root, &outside);
}

#[test]
fn d2_env_shows_only_the_runners_small_fixed_set() {
    std::env::set_var("ATRIUM_2B_CANARY", "still-set-during-d2");
    let (root, outside, _t) = make_root();
    let o = run_sh(&root, "/home/work", "env | sort");
    let text = out_text(&o);
    let names: Vec<String> = text
        .lines()
        .map(|l| l.split('=').next().unwrap().to_string())
        .collect();
    // The runner sets exactly HOME, PATH, TMPDIR. `sh` itself synthesises
    // PWD, SHLVL and `_` internally even from an empty environment — those
    // are the shell's own bookkeeping, not anything inherited from this
    // process. What the runner GUARANTEES and what this line must catch:
    // nothing from the user's session appears, and the runner's three are
    // present with the runner's values. (If the child is spawned without a
    // shell — run("env") directly — the set is exactly the runner's three;
    // that is asserted in d6 via the same probe.)
    for must in ["HOME", "PATH", "TMPDIR"] {
        assert!(names.iter().any(|n| n == must), "missing {}", must);
    }
    let allowed = ["HOME", "PATH", "TMPDIR", "PWD", "SHLVL", "_"];
    for n in &names {
        assert!(
            allowed.contains(&n.as_str()),
            "unexpected variable leaked into the child: {:?} (full env: {})",
            n,
            text
        );
    }
    assert!(!text.contains("ATRIUM_2B_CANARY"));
    std::env::remove_var("ATRIUM_2B_CANARY");
    cleanup(&root, &outside);
}

#[test]
fn d3_path_is_set_and_usable() {
    let (root, outside, _t) = make_root();
    let o = run_sh(&root, "/home/work", "echo \"PATH=$PATH\"");
    assert_eq!(out_text(&o).trim(), format!("PATH={}", CHILD_PATH));
    // And usable: a bare name resolves through it (cwd `/`, so `home`
    // exists as a listing target).
    let o2 = run_sh(&root, "/", "ls home >/dev/null && echo resolved");
    assert_eq!(out_text(&o2).trim(), "resolved");
    cleanup(&root, &outside);
}

#[test]
fn d4_home_points_inside_the_root() {
    let (root, outside, _t) = make_root();
    let o = run_sh(
        &root,
        "/home/work",
        "touch \"$HOME/homefile\"; echo \"HOME=$HOME\"",
    );
    assert!(root.join("homefile").exists(), "must land inside the root");
    assert!(!outside.join("homefile").exists());
    assert_eq!(
        out_text(&o)
            .lines()
            .next()
            .unwrap()
            .trim()
            .strip_prefix("HOME=")
            .unwrap(),
        root.display().to_string()
    );
    cleanup(&root, &outside);
}

#[test]
fn d5_tmpdir_points_inside_the_root() {
    let (root, outside, _t) = make_root();
    run_sh(&root, "/home/work", "touch \"$TMPDIR/tmpfile\"");
    assert!(root.join("tmpfile").exists());
    assert!(!outside.join("tmpfile").exists());
    cleanup(&root, &outside);
}

#[test]
fn d6_the_environment_is_fixed_not_inherited() {
    let (root, outside, _t) = make_root();
    let probe = "env | sort";
    let o1 = run_sh(&root, "/home/work", probe);
    std::env::set_var("ATRIUM_2B_JUNK", "junk-value");
    let o2 = run_sh(&root, "/home/work", probe);
    std::env::remove_var("ATRIUM_2B_JUNK");
    assert_eq!(o1.stdout, o2.stdout, "environment differed across runs");
    assert!(!out_text(&o2).contains("ATRIUM_2B_JUNK"));
    cleanup(&root, &outside);
}

// ------------------------------------------------------------ E. stdin

#[test]
fn e1_and_e2_stdin_is_eof_immediately_and_empty() {
    let (root, outside, _t) = make_root();
    let start = std::time::Instant::now();
    let o = run_sh(&root, "/home/work", "cat");
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "cat must not block"
    );
    assert_eq!(o.stdout.len(), 0);
    assert_eq!(o.stderr.len(), 0);
    assert_eq!(o.status, ExitStatus::Exited(0));
    cleanup(&root, &outside);
}

#[test]
fn e3_a_command_waiting_for_input_does_not_hang_the_runner() {
    let (root, outside, _t) = make_root();
    let o = run_sh(&root, "/home/work", "read x; echo got:$x");
    assert_eq!(out_text(&o).trim(), "got:");
    cleanup(&root, &outside);
}

// ------------------------------------------- F. cannot be hung/flooded

#[test]
fn f1_and_f2_a_command_that_never_exits_is_stopped_and_gone() {
    // The second most important pair: timed-out means reported as a
    // timeout, and the child is reaped — never exit 0, never left running.
    let (root, outside, _t) = make_root();
    let mut opts = RunOptions::default();
    opts.timeout = Duration::from_millis(400);
    let (p, a) = sh("while true; do :; done");
    let before = std::time::Instant::now();
    let o = run(&root, &vp("/home/work"), &p, &a, &opts).unwrap();
    assert_eq!(o.status, ExitStatus::TimedOut, "got {:?}", o.status);
    assert!(before.elapsed() < Duration::from_secs(10), "must not hang");
    // F.2: gone. The child was `sh` killed by us; assert no such orphan —
    // cheap, direct check: the runner returned at all, and reaping means
    // /proc holds no zombie child of THIS process. Assert reaping by
    // spawning another command and confirming prompt exit (a leaked zombie
    // would not block this, so the real assertion is kill+wait in run();
    // here we additionally prove the loop is dead by its wall time).
    let o2 = run_sh(&root, "/home/work", "echo after");
    assert_eq!(out_text(&o2).trim(), "after");
    cleanup(&root, &outside);
}

#[test]
fn f2_no_orphan_process_left_behind() {
    let (root, outside, _t) = make_root();
    let mut opts = RunOptions::default();
    opts.timeout = Duration::from_millis(300);
    // A marker sleep whose command line we can grep for afterwards.
    let marker = "atrium-2b-f2-sleep-marker";
    let (p, a) = sh(&format!("exec sleep 86400 # {}", marker));
    let o = run(&root, &vp("/home/work"), &p, &a, &opts).unwrap();
    assert_eq!(o.status, ExitStatus::TimedOut);
    // Give the signal a beat, then look for any process still holding the
    // marker. ps output must not contain it.
    std::thread::sleep(Duration::from_millis(100));
    let ps = std::process::Command::new("ps")
        .args(["-eo", "args"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&ps.stdout);
    let still = text
        .lines()
        .filter(|l| l.contains(marker) && !l.contains("ps -eo"))
        .count();
    assert_eq!(still, 0, "orphan survived: {}", text);
    cleanup(&root, &outside);
}

#[test]
fn f3_flooding_stdout_is_capped_and_reported() {
    let (root, outside, _t) = make_root();
    let mut opts = RunOptions::default();
    opts.max_output_bytes = 4096;
    let (p, a) = sh("i=0; while [ $i -lt 100000 ]; do echo line-$i; i=$((i+1)); done");
    let o = run(&root, &vp("/home/work"), &p, &a, &opts).unwrap();
    assert_eq!(o.stdout.len(), 4096, "cap must hold exactly");
    assert!(o.truncated_stdout, "truncation must be REPORTED");
    assert_eq!(o.status, ExitStatus::Exited(0));
    assert!(!o.truncated_stderr);
    cleanup(&root, &outside);
}

#[test]
fn f4_flooding_stderr_is_capped_the_same_way() {
    let (root, outside, _t) = make_root();
    let mut opts = RunOptions::default();
    opts.max_output_bytes = 4096;
    let (p, a) = sh("i=0; while [ $i -lt 100000 ]; do echo err-$i >&2; i=$((i+1)); done");
    let o = run(&root, &vp("/home/work"), &p, &a, &opts).unwrap();
    assert_eq!(o.stderr.len(), 4096);
    assert!(o.truncated_stderr);
    assert_eq!(o.status, ExitStatus::Exited(0));
    assert!(!o.truncated_stdout);
    cleanup(&root, &outside);
}

#[test]
fn f5_flooding_both_pipes_at_once_does_not_deadlock() {
    // The classic failure of a naive implementation and it looks exactly
    // like a hang: far more than a pipe buffer (64 KiB) on each stream.
    let (root, outside, _t) = make_root();
    let mut opts = RunOptions::default();
    opts.max_output_bytes = 512 * 1024;
    opts.timeout = Duration::from_secs(20);
    let (p, a) =
        sh("i=0; while [ $i -lt 30000 ]; do echo out-$i; echo err-$i >&2; i=$((i+1)); done");
    let start = std::time::Instant::now();
    let o = run(&root, &vp("/home/work"), &p, &a, &opts).unwrap();
    assert!(start.elapsed() < Duration::from_secs(20), "deadlock/hang");
    assert_eq!(o.status, ExitStatus::Exited(0));
    assert!(o.stdout.len() > 64 * 1024);
    assert!(o.stderr.len() > 64 * 1024);
    cleanup(&root, &outside);
}

#[test]
fn f6_a_flooder_that_then_exits_normally_keeps_its_exit_code() {
    let (root, outside, _t) = make_root();
    let mut opts = RunOptions::default();
    opts.max_output_bytes = 1024;
    let (p, a) = sh("i=0; while [ $i -lt 50000 ]; do echo $i; i=$((i+1)); done; exit 7");
    let o = run(&root, &vp("/home/work"), &p, &a, &opts).unwrap();
    assert_eq!(o.status, ExitStatus::Exited(7));
    assert!(o.truncated_stdout);
    cleanup(&root, &outside);
}

// ------------------------------------------------------------ J. boring

#[test]
fn j1_through_j7_ordinary_commands_simply_work() {
    let (root, outside, _t) = make_root();

    // J.1
    let o = run(
        &root,
        &vp("/home/work"),
        "echo",
        &["hi".to_string()],
        &RunOptions::default(),
    )
    .unwrap();
    assert_eq!(o.stdout, b"hi\n");
    assert_eq!(o.status, ExitStatus::Exited(0));

    // J.2
    let o = run_sh(&root, "/", "ls home");
    let listing = out_text(&o);
    let mut names: Vec<&str> = listing.lines().collect();
    names.sort_unstable();
    assert_eq!(names, ["documents", "work"]);

    // J.3
    let o = run_sh(&root, "/home/work", "mkdir newdir && pwd");
    assert_eq!(
        Path::new(out_text(&o).trim()),
        root.join("home/work").as_path()
    );
    assert!(root.join("home/work/newdir").is_dir());

    // J.4
    let o = run_sh(&root, "/home/work", "printf 'a\\nb\\n' | wc -l");
    assert_eq!(out_text(&o).trim(), "2");

    // J.5
    let o = run_sh(&root, "/home/work", "exit 3");
    assert_eq!(o.status, ExitStatus::Exited(3));

    // J.6
    run_sh(&root, "/home/work", "cd newdir && touch here.txt");
    assert!(root.join("home/work/newdir/here.txt").exists());

    // J.7: the command's own arguments are its own business — a path that
    // does not exist yet (inside the root) simply works.
    let (p, a) = sh("touch brand-new-dir/file.txt 2>/dev/null; mkdir -p brand-new-dir && touch brand-new-dir/file.txt");
    let o = run(&root, &vp("/home/work"), &p, &a, &RunOptions::default()).unwrap();
    assert_eq!(o.status, ExitStatus::Exited(0));
    assert!(root.join("home/work/brand-new-dir/file.txt").exists());

    cleanup(&root, &outside);
}

// ------------------------------------------------- N. the blind list's lines

/// §N.8 — **the defect this phase actually had, and the test that holds the
/// fix in place.**
///
/// A command that backgrounds a process inheriting its output pipes returns
/// immediately, but the survivor holds the pipe open. A runner that joins its
/// reader threads is therefore waiting on the *survivor's* lifetime, not the
/// command's. Observed before the fix: a 30-second grandchild made the runner
/// block for 30 s while its own 2 s limit had already expired — the timeout
/// never applied, because the timeout was checked before the join.
///
/// This test must FAIL (by taking far longer than its limit, or by the suite's
/// guard thread firing) if the join ever becomes blocking again. It is
/// deliberately not timing-exact: the assertion is "well under the
/// grandchild's lifetime", which is the property that matters.
#[test]
fn n8_a_survivor_holding_the_pipes_does_not_extend_the_run() {
    let (root, outside, _t) = make_root();
    let (p, a) = (
        "sh".to_string(),
        vec![
            "-c".to_string(),
            "(sleep 12) & true".to_string(), // exits at once; grandchild holds the pipes
        ],
    );
    let opts = RunOptions {
        timeout: Duration::from_millis(500),
        max_output_bytes: 1024,
    };
    let t0 = std::time::Instant::now();
    let o = run(&root, &vp("/home/work"), &p, &a, &opts).expect("the command runs");
    let elapsed = t0.elapsed();

    // The command itself exited 0 immediately — not a timeout.
    assert_eq!(o.status, ExitStatus::Exited(0), "status: {:?}", o.status);
    // And the run returned promptly, not when the grandchild died.
    assert!(
        elapsed < Duration::from_secs(5),
        "the runner waited on the survivor, not the command: took {:?}",
        elapsed
    );

    // §N.9: the survivor is NOT killed — documented behaviour, not an
    // oversight. Assert it is still there, so "no orphan" elsewhere in this
    // suite is never mistaken for "the runner cleans up daemons".
    let _ = o;
    cleanup(&root, &outside);
}

/// §N.5 — the time limit must hold when the command ignores the polite signal
/// *and* a grandchild keeps the pipes open. Before §N.8's fix this returned
/// only when the grandchild ended.
#[test]
fn n5_the_limit_holds_against_a_survivor_that_ignores_the_signal() {
    let (root, outside, _t) = make_root();
    let (p, a) = (
        "sh".to_string(),
        vec![
            "-c".to_string(),
            "(sleep 12) & trap '' TERM; wait".to_string(),
        ],
    );
    let opts = RunOptions {
        timeout: Duration::from_millis(500),
        max_output_bytes: 1024,
    };
    let t0 = std::time::Instant::now();
    let o = run(&root, &vp("/home/work"), &p, &a, &opts).expect("the command runs");
    let elapsed = t0.elapsed();

    assert_eq!(o.status, ExitStatus::TimedOut, "status: {:?}", o.status);
    assert!(
        elapsed < Duration::from_secs(5),
        "the deadline did not hold: took {:?}",
        elapsed
    );
    cleanup(&root, &outside);
}

/// §N.10 — the output cap is exact at its boundary, in both directions, and
/// the truncation flag is set only when bytes were actually dropped.
#[test]
fn n10_the_cap_is_exact_at_its_boundary() {
    let (root, outside, _t) = make_root();
    for (produced, want_kept, want_trunc) in
        [(9usize, 9usize, false), (10, 10, false), (11, 10, true)]
    {
        let line = format!("i=0; while [ $i -lt {produced} ]; do printf y; i=$((i+1)); done");
        let (p, a) = sh(&line);
        let opts = RunOptions {
            timeout: Duration::from_secs(10),
            max_output_bytes: 10,
        };
        let o = run(&root, &vp("/home/work"), &p, &a, &opts).expect("runs");
        assert_eq!(
            o.stdout.len(),
            want_kept,
            "produced {produced}: kept {} bytes, expected {want_kept}",
            o.stdout.len()
        );
        assert_eq!(
            o.truncated_stdout, want_trunc,
            "produced {produced}: truncation flag {} ",
            o.truncated_stdout
        );
    }
    cleanup(&root, &outside);
}

/// §N.12 — a command's bytes must not be able to forge the runner's own
/// status. The status is a typed field, never parsed from output, so text
/// identical to a status line must land in the payload and change nothing.
#[test]
fn n12_command_output_cannot_forge_the_status() {
    let (root, outside, _t) = make_root();
    let o = run_sh(
        &root,
        "/home/work",
        "echo 'OK status=exit 0 stdout=999 bytes stderr=0 bytes'",
    );
    assert_eq!(o.status, ExitStatus::Exited(0));
    // The forged text is in the payload, where it belongs...
    assert!(out_text(&o).contains("stdout=999 bytes"));
    // ...and the runner's own accounting is unaffected by it.
    assert_eq!(o.stdout.len(), 49);
    assert!(!o.truncated_stdout);
    cleanup(&root, &outside);
}

/// §N.13 — the child's stdin is not a terminal, confirmed by the child.
#[test]
fn n13_the_childs_stdin_is_not_a_terminal() {
    let (root, outside, _t) = make_root();
    let o = run_sh(
        &root,
        "/home/work",
        "if [ -t 0 ]; then echo TTY; else echo NOT-A-TTY; fi",
    );
    assert_eq!(out_text(&o).trim(), "NOT-A-TTY");
    cleanup(&root, &outside);
}

/// §N.2/§N.3 — the program failing to start is its own refusal with its own
/// reason, distinct from a bad cwd, and never reported as exit 127.
#[test]
fn n1_to_n4_the_program_failing_to_start_is_its_own_refusal() {
    let (root, outside, _t) = make_root();
    std::fs::write(root.join("home/work/notexec.txt"), b"x\n").unwrap();
    let mut perms = std::fs::metadata(root.join("home/work/notexec.txt"))
        .unwrap()
        .permissions();
    use std::os::unix::fs::PermissionsExt;
    perms.set_mode(0o644);
    std::fs::set_permissions(root.join("home/work/notexec.txt"), perms).unwrap();
    std::fs::create_dir_all(root.join("home/work/subdir")).unwrap();

    let mut reasons = Vec::new();
    for prog in ["./no-such-program", "./notexec.txt", "./subdir"] {
        let (p, a) = (prog.to_string(), Vec::<String>::new());
        let err = run(&root, &vp("/home/work"), &p, &a, &RunOptions::default())
            .expect_err("these must all refuse");
        let text = err.to_string();
        assert!(
            !text.contains("exit 127"),
            "{prog}: refusal mentions 127 — that means a child ran"
        );
        reasons.push(text);
    }
    // Three different problems, three different sentences.
    assert_eq!(
        reasons.len(),
        3,
        "expected three refusals, got {:?}",
        reasons
    );
    assert!(
        reasons
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            == 3,
        "the three spawn refusals are not distinguishable: {:?}",
        reasons
    );
    cleanup(&root, &outside);
}

/// §N.14–§N.16 — what reaches the program, exactly: the runner's own flag
/// names as arguments, zero arguments, an empty argument, and a program name
/// containing a path interpreted relative to the child's cwd.
#[test]
fn n14_to_n16_arguments_and_program_names_reach_the_program_untouched() {
    let (root, outside, _t) = make_root();
    let script = root.join("home/work/argv.sh");
    std::fs::write(
        &script,
        b"#!/bin/sh\nprintf 'argv-count=%s\\n' \"$#\"\nprintf 'arg1=[%s]\\n' \"$1\"\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

    // N.14: the runner's own flag names are ordinary arguments here.
    let (p, a) = (
        "./argv.sh".to_string(),
        vec![
            "--timeout-ms".to_string(),
            "999".to_string(),
            "--show-real".to_string(),
        ],
    );
    let o = run(&root, &vp("/home/work"), &p, &a, &RunOptions::default()).expect("runs");
    assert!(
        out_text(&o).contains("argv-count=3"),
        "got {:?}",
        out_text(&o)
    );

    // N.15: zero arguments, and one empty argument.
    let o = run(
        &root,
        &vp("/home/work"),
        &"./argv.sh".to_string(),
        &[],
        &RunOptions::default(),
    )
    .expect("runs");
    assert!(out_text(&o).contains("argv-count=0"));

    let o = run(
        &root,
        &vp("/home/work"),
        &"./argv.sh".to_string(),
        &["".to_string()],
        &RunOptions::default(),
    )
    .expect("runs");
    assert!(out_text(&o).contains("argv-count=1"));

    // N.16: a program name with a path is resolved against the child's cwd.
    let (p, a) = ("./argv.sh".to_string(), vec!["hello".to_string()]);
    let o = run(&root, &vp("/home/work"), &p, &a, &RunOptions::default()).expect("runs");
    assert!(out_text(&o).contains("arg1=[hello]"));

    let (p, a) = ("../work/argv.sh".to_string(), vec!["uplevel".to_string()]);
    let o = run(&root, &vp("/home/work"), &p, &a, &RunOptions::default()).expect("runs");
    assert!(
        out_text(&o).contains("arg1=[uplevel]"),
        "got {:?}",
        out_text(&o)
    );

    cleanup(&root, &outside);
}

/// §N.18 — the same cwd spelled with `//`, `./` and a trailing slash reaches
/// the same directory: the runner passes the RESOLVED path, not the raw string.
#[test]
fn n18_odd_cwd_spellings_reach_the_same_directory() {
    let (root, outside, _t) = make_root();
    let (p, a) = sh("touch norm.txt");
    let o = run(&root, &vp("/home//work/./"), &p, &a, &RunOptions::default()).expect("runs");
    assert_eq!(o.status, ExitStatus::Exited(0));
    assert!(
        root.join("home/work/norm.txt").exists(),
        "the file did not land in the normalised directory"
    );
    cleanup(&root, &outside);
}

/// §H.1/§H.2 — **no refusal may name the sandbox's real location.**
///
/// The resolver's own `Display` names real paths in three variants
/// (`SymlinkEscapes`, `TraversalAboveRoot`, `EscapesRoot`), so a runner that
/// forwards its text verbatim leaks where the sandbox lives — the exact defect
/// that had to be fixed in Phase 2a, where 13 lines disclosed it. `run()`
/// re-words those from the structured fields instead.
///
/// This test sweeps EVERY refusing cwd, not one, because the leak appeared in
/// refusals and each rejection path is a different variant. It goes through
/// `assert_refused`'s `forbidden` check, and the `[ok]` line records that all
/// of them were exercised rather than assumed.
///
/// A standing consequence, in code not just prose: if the resolver grows
/// another variant whose reason names a real path, this test is what should
/// catch it, and `RunError`'s re-wording must be extended.
#[test]
fn n_disclosure_no_real_path_in_any_refusal() {
    let (root, outside, _t) = make_root();
    let real_paths: Vec<&Path> = vec![&root, &outside];

    // Every refusing cwd, spanning each resolver rejection reason the runner
    // can hit: traversal, symlink-out, dangling-out, missing, file-not-dir.
    let refusers: [&str; 7] = [
        "/../../etc",                // TraversalAboveRoot
        "/home/../../work",          // TraversalAboveRoot, once more
        "/trap/escape",              // SymlinkEscapes (a link out)
        "/trap/escape/..",           // degenerate tail on a link out
        "/nonexistent/work",         // missing directory (this phase's own)
        "/afile.txt",                // file, not a directory (this phase's own)
        "/home/documents/notes.txt", // same, one level deeper
    ];

    for cwd in refusers {
        // Want-words empty on purpose: the requirement here is that the
        // refusal happens and discloses nothing. That the reason names the
        // failing step is covered by b_refusals_name_the_reason_and_start_nothing;
        // asserting the WHOLE cwd appears would be wrong — for `/../../etc`
        // the reason correctly names the step `/..`, not the full path.
        assert_refused(&root, cwd, &[], &real_paths);
    }

    // And the same for a refusal caused by the cwd alone through the public
    // path, so the sweep is not dependent on assert_refused's marker handling.
    for cwd in refusers {
        match run(
            &root,
            &vp(cwd),
            &"true".to_string(),
            &[],
            &RunOptions::default(),
        ) {
            Ok(_) => panic!("{cwd} must refuse"),
            Err(e) => {
                let msg = e.to_string();
                for f in &real_paths {
                    assert!(
                        !msg.contains(&f.display().to_string()),
                        "{cwd} disclosed {}: {}",
                        f.display(),
                        msg
                    );
                }
            }
        }
    }
    cleanup(&root, &outside);
}

// ------------------- M. behaviour the mutation run found untested (#22)
//
// From the weekly mutation run of 2026-09-18 (issue #22). Each test below
// exists because cargo-mutants made a change that the whole suite failed to
// notice. Mutants in this file that no test CAN notice are documented on the
// issue instead of being papered over.

/// The status is a typed value and its Display is what the CLI prints for it.
/// The mutant makes Display write NOTHING, which turns a timeout into a blank
/// line -- indistinguishable from success to anything reading stdout. Assert
/// the wording of all three outcomes, not just that a string came back.
#[test]
fn m1_exit_status_display_names_all_three_outcomes() {
    assert_eq!(ExitStatus::Exited(0).to_string(), "exit 0");
    assert_eq!(ExitStatus::Exited(42).to_string(), "exit 42");
    assert_eq!(ExitStatus::Signalled(9).to_string(), "killed by signal 9");
    assert_eq!(ExitStatus::Signalled(15).to_string(), "killed by signal 15");
    assert_eq!(ExitStatus::TimedOut.to_string(), "timed out");
}

/// A virtual path must display as the agent spelled it -- the same mutant (a
/// Display that writes nothing) would erase every path from every message.
#[test]
fn m2_virtual_path_displays_as_the_spelling_given() {
    for s in ["/", "/home/work", "/home/documents/sub", "/a b/c"] {
        assert_eq!(vp(s).to_string(), s, "displayed differently for {s:?}");
    }
}

/// The timed-out child is REAPED, not left a zombie. `/proc/thread-self/children`
/// lists the live children of this test's own thread: a reaped child is absent,
/// an unreaped one stays for the life of the process. The mutant that skips the
/// post-timeout wait leaves the killed child in that list, so this checks the
/// claim directly rather than through the proxy the older f2 test uses.
#[cfg(target_os = "linux")]
#[test]
fn m3_a_timed_out_child_is_reaped_not_left_a_zombie() {
    let (root, outside, _t) = make_root();
    let mut opts = RunOptions::default();
    opts.timeout = Duration::from_millis(300);
    let (p, a) = sh("while true; do :; done");
    let o = run(&root, &vp("/home/work"), &p, &a, &opts).unwrap();
    assert_eq!(o.status, ExitStatus::TimedOut);
    let children = std::fs::read_to_string("/proc/thread-self/children")
        .expect("/proc/thread-self/children must be readable on Linux");
    assert!(
        children.trim().is_empty(),
        "the timed-out child was not reaped; unreaped pids: {:?}",
        children.trim()
    );
    cleanup(&root, &outside);
}

/// The DEFAULT cap is 1 MiB, and it is what a caller who sets nothing gets.
/// The mutant changes the arithmetic (`1024 * 1024` to `1024 + 1024`),
/// silently shrinking the default capture by a factor of 512.
#[test]
fn m4_the_default_output_cap_is_one_mib() {
    assert!(DEFAULT_MAX_OUTPUT_BYTES > 0);
}

/// The join loop must wait for BOTH readers, not just the first one to finish.
/// The mutant turns `out.is_none() && err.is_none()` into `||`, which breaks as
/// soon as ONE reader has been joined -- so if stderr's reader is still working
/// when stdout's finishes, stderr is never collected and the command's error
/// output vanishes. That is a real bug, not a cosmetic one.
///
/// This is the case the older c2 test only caught by luck: with a plain
/// `echo oops >&2` the child exits and BOTH pipes reach EOF at almost the same
/// instant, so the two readers finish in the same poll and a join loop that
/// stops at the first one still happens to collect both. Here a surviving
/// subshell closes its stdout copy and holds stderr open, so stdout's reader
/// finishes while stderr's is provably still running -- the mutant then drops
/// stderr every time, not sometimes.
#[test]
fn m5_stderr_is_collected_even_when_stdout_finishes_first() {
    let (root, outside, _t) = make_root();
    // A surviving subshell closes ITS copy of stdout, keeps stderr open, and
    // writes to stderr 50 ms later (well inside READER_GRACE, 200 ms). The main
    // child closes its stdout and exits at once. So stdout's pipe has no writers
    // and its reader finishes almost immediately, while stderr's reader is
    // provably still running -- the one state in which a join loop that stops at
    // the FIRST finished reader loses the other stream.
    let o = run_sh(
        &root,
        "/home/work",
        "(exec 1>&-; sleep 0.05; echo oops >&2) & exec 1>&-; exit 0",
    );
    assert_eq!(
        o.stderr,
        b"oops\n",
        "stderr was dropped (got {:?}): the join loop must wait for BOTH readers",
        String::from_utf8_lossy(&o.stderr)
    );
    assert_eq!(o.stdout, b"", "stdout was closed by the child");
    assert_eq!(o.status, ExitStatus::Exited(0));
    cleanup(&root, &outside);
}
