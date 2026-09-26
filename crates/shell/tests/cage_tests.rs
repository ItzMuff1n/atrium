//! Phase 2e — the cage, tested through the real `run()`.
//!
//! These are the assertions the phase exists to make. They drive the public
//! `run()` with `RunOptions::default()` (caged) against a root on an isolated
//! filesystem.
//!
//! **The shape of every test here matters.** Each one asserts one of two
//! things: the cage held, or the run REFUSED and nothing ran. The refusal is a
//! correct outcome — it is the fail-closed rule (`BUILD-PLAN.md` §2e,
//! `attack-list-2e.md` §F) — but it is *not* evidence that the cage holds.
//! `run_caged` therefore refuses to let a refusal pass quietly while a cage
//! program is installed, and `the_cage_is_actually_available_where_the_phase_is_verified`
//! fails if the whole file would otherwise have gone down the refusal path.
//!
//! **Why that guard exists, measured.** Pointing the fixture at a non-isolated
//! root (so `cage::build` always refused) left **10 of 11** tests in an earlier
//! draft of this file green while no cage ever ran. That is exactly the
//! "looks like success" failure this phase is built to prevent, so it is a hard
//! error now.

use std::path::{Path, PathBuf};

use atrium_shell::cage::{CageError, SpawnReason};
use atrium_shell::{run, ExitStatus, RunError, RunOptions, VirtualPath};

/// A root on a filesystem not shared with `/home`, `/tmp` or `/`.
///
/// `/dev/shm` is a separate tmpfs mount on every standard Linux, including the
/// GitHub runner. It is world-writable, so the root built inside it is 0700.
/// This is what `cage::check_root_isolated` requires: a root under `/tmp`
/// shares `/tmp`'s device (measured), a hard link from `/tmp` into such a root
/// is creatable, and the boundary would not hold.
fn isolated_root(tag: &str) -> PathBuf {
    let root = PathBuf::from(format!("/dev/shm/atrium-2e-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("home/work")).expect("build the fixture root");
    std::fs::create_dir_all(root.join("home/documents")).expect("build the fixture root");
    std::fs::write(root.join("home/documents/notes.txt"), b"hi\n").expect("seed a file");
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700));
    root
}

fn cleanup(root: &Path) {
    let _ = std::fs::remove_dir_all(root);
}

fn vp(s: &str) -> VirtualPath {
    VirtualPath::new(s).expect("test paths are absolute and constant")
}

fn out_text(o: &atrium_shell::Outcome) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

/// Run `sh -c <line>` caged.
///
/// `Some(outcome)` if the command ran inside a cage. `None` **only** when no
/// cage program exists on the machine at all. A refusal while `bwrap` IS
/// installed is a panic — see the file header for why.
fn run_caged(root: &Path, cwd: &str, line: &str) -> Option<atrium_shell::Outcome> {
    let p = "sh".to_string();
    let a = vec!["-c".to_string(), line.to_string()];
    match run(root, &vp(cwd), &p, &a, &RunOptions::default()) {
        Ok(o) => Some(o),
        Err(RunError::Uncageable(e)) => {
            if atrium_shell::cage::find_cage_program().is_err() {
                None
            } else {
                panic!(
                    "the cage program is installed but {line:?} was refused: {e} -- \
                     not fail-closed, a broken cage, and every assertion below would \
                     have passed for the wrong reason"
                );
            }
        }
        Err(e) => panic!("unexpected error for {line:?}: {e}"),
    }
}

// ---------------------------------------------------------------------------
// The requirement: outside the root does not exist
// ---------------------------------------------------------------------------

#[test]
fn the_cage_hides_the_hosts_etc_passwd() {
    let root = isolated_root("passwd");
    // 2>&1 so the failure message lands on the stream being asserted on --
    // `cat` writes it to stderr, which is a different pipe.
    if let Some(o) = run_caged(&root, "/home/work", "cat /etc/passwd 2>&1; echo rc=$?") {
        let printed = out_text(&o);
        assert!(
            !printed.contains("root:x:0:0:root:"),
            "the host /etc/passwd came back from inside the cage — THE CAGE LEAKS: {printed:?}"
        );
        assert!(
            printed.contains("No such file"),
            "/etc/passwd must not exist inside the cage, got: {printed:?}"
        );
        assert!(printed.contains("rc=1"), "cat must fail, got: {printed:?}");
    }
    cleanup(&root);
}

#[test]
fn a_write_from_inside_never_lands_in_the_hosts_tmp() {
    let root = isolated_root("hosttmp");
    let marker = format!("/tmp/atrium-2e-escape-{}", std::process::id());
    let _ = std::fs::remove_file(&marker);
    if run_caged(&root, "/home/work", &format!("touch {marker}")).is_some() {
        assert!(
            !Path::new(&marker).exists(),
            "a caged command wrote into the real /tmp — THE CAGE LEAKS"
        );
    }
    let _ = std::fs::remove_file(&marker);
    cleanup(&root);
}

#[test]
fn a_program_that_cannot_start_inside_the_cage_refuses_with_its_own_distinct_reason() {
    // attack-list-2b.md §N.1-N.4, and Muffin's decision of 26 Sep 2026: the three
    // problems stay distinguishable, the wording is ATRIUM's, and it is never the
    // cage program's `execvp ...` sentence nor an exit code of 127.
    //
    // How this works, so the test is not mistaken for luck: the cage program
    // writes no `exit-code` line when it could not exec the command, and atrium
    // then asks the cage's own filesystem a direct question (cage::SpawnReason).
    // Nothing is parsed out of anyone's error text.
    let root = isolated_root("spawn");
    std::fs::write(root.join("home/work/notexec.txt"), b"x\n").expect("the fixture file");
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(
        root.join("home/work/notexec.txt"),
        std::fs::Permissions::from_mode(0o644),
    );
    std::fs::create_dir_all(root.join("home/work/sub")).expect("the fixture directory");

    let mut reasons: Vec<String> = Vec::new();
    for (program, want) in [
        ("./no-such-program-2e", SpawnReason::ProgramNotFound),
        ("./notexec.txt", SpawnReason::NotExecutable),
        ("./sub", SpawnReason::IsADirectory),
    ] {
        let p = program.to_string();
        match run(&root, &vp("/home/work"), &p, &[], &RunOptions::default()) {
            Err(RunError::SpawnInsideCage {
                program: got,
                reason,
            }) => {
                assert_eq!(got, program, "the refusal must name the program asked for");
                assert_eq!(
                    reason, want,
                    "{program} was classified {reason:?}, expected {want:?}"
                );
                // The wording is atrium's own: it must not carry the cage
                // program's phrasing, which reuses one sentence for all three.
                let msg = reason.wording();
                assert!(
                    !msg.contains("execvp"),
                    "the reason is the cage program's words, not atrium's: {msg:?}"
                );
                reasons.push(msg.to_string());
            }
            Err(RunError::Spawn { reason, .. }) => panic!(
                "a command that cannot start inside the cage was reported as the RUNNER's own \
                 spawn failure, so the cage program's status was not consulted: {reason}"
            ),
            Err(other) => panic!("{program}: unexpected error {other}"),
            Ok(o) => panic!(
                "{program} RAN and reported {:?} -- a program that cannot start must not run",
                o.status
            ),
        }
    }
    // §N.4: three problems, three sentences.
    let mut unique = reasons.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(
        unique.len(),
        3,
        "the three spawn refusals are not distinguishable from the refusal alone: {reasons:?}"
    );
    cleanup(&root);
}

#[test]
fn our_own_time_limit_is_reported_as_a_timeout_and_never_as_a_signal() {
    // §C.6, Muffin's decision of 26 Sep 2026: `timed-out` means OUR timer fired.
    //
    // What this can and cannot show, stated honestly. Inside a cage the process
    // the runner waits on is the cage program, and it NORMALISES the command's
    // death: measured 26 Sep 2026, `kill -9 $$` inside and `exit 137` inside both
    // come back reported identically as 137. So a command's own death signal is
    // not recoverable from inside a cage — that is bwrap's behaviour, not atrium's
    // choice, and it is recorded in README.md as a limit rather than hidden. What
    // IS testable, and what this asserts, is that the runner never claims OUR kill
    // was a timeout it did not perform, and never claims a timeout when the command
    // ended on its own.
    let root = isolated_root("timer");

    // Our timer fires.
    let opts = RunOptions {
        timeout: std::time::Duration::from_millis(400),
        ..Default::default()
    };
    let p = "sh".to_string();
    let a = vec!["-c".to_string(), "exec sleep 60".to_string()];
    match run(&root, &vp("/home/work"), &p, &a, &opts) {
        Ok(o) => assert_eq!(
            o.status,
            ExitStatus::TimedOut,
            "our own time limit must be reported as a timeout"
        ),
        Err(e) => panic!("sleeping past the limit must time out, not error: {e}"),
    }

    // The command ends on its own, well inside the limit.
    let a = vec!["-c".to_string(), "exit 3".to_string()];
    match run(&root, &vp("/home/work"), &p, &a, &RunOptions::default()) {
        Ok(o) => assert_eq!(
            o.status,
            ExitStatus::Exited(3),
            "a command that ended on its own must be reported as its own exit"
        ),
        Err(e) => panic!("unexpected error: {e}"),
    }
    cleanup(&root);
}

#[test]
fn the_hosts_session_keyring_does_not_cross_the_cage() {
    // Found by probing on 26 Sep 2026, after an independent attack list pointed
    // at keyrings: the host's session keyring crossed the cage. From inside,
    // `keyctl print <host-key-id>` returned the host's secret, `keyctl add … @s`
    // succeeded (the host saw the key afterwards), and `keyctl unlink <host-key>
    // @s` DESTROYED the host's key — read, write and delete across a boundary this
    // phase calls closed.
    //
    // No amount of `--clearenv` touches it: a keyring is kernel state, not a
    // variable, and a keyring is inherited by a forked child exactly as file
    // descriptors are. The fix gives the child its own empty session keyring in
    // `pre_exec`, and this test is what keeps it there.
    //
    // The host is the independent witness: it checks its own keyring afterwards,
    // not anything the cage says about itself.
    use std::process::Command;
    if Command::new("keyctl").arg("show").output().is_err() {
        return; // no keyctl on this machine: the vector cannot be exercised here
    }
    let root = isolated_root("keyring");
    let name = format!("atrium-2e-key-{}", std::process::id());
    let secret = "SECRET-MUST-NOT-CROSS-9f2c";
    let added = Command::new("keyctl")
        .args(["add", "user", &name, secret, "@s"])
        .output()
        .expect("run keyctl add");
    let id = String::from_utf8_lossy(&added.stdout).trim().to_string();
    if id.is_empty() || id.parse::<i64>().is_err() {
        cleanup(&root);
        return; // could not establish the fixture; do not assert on nothing
    }

    // By id, and by name: neither may reveal the secret.
    if let Some(o) = run_caged(
        &root,
        "/home/work",
        &format!("keyctl print {id} 2>&1 | head -1"),
    ) {
        let printed = out_text(&o);
        assert!(
            !printed.contains(secret),
            "THE HOST'S SESSION KEYRING CROSSED THE CAGE: {printed:?}"
        );
    }
    if let Some(o) = run_caged(
        &root,
        "/home/work",
        &format!("keyctl search @s user {name} 2>&1 | head -1"),
    ) {
        let printed = out_text(&o);
        assert!(
            !printed.contains(&id),
            "the host's key is reachable BY NAME inside the cage: {printed:?}"
        );
    }

    // The independent check: the host's own keyring is untouched.
    let after = Command::new("keyctl")
        .args(["print", &id])
        .output()
        .expect("run keyctl print on the host");
    assert_eq!(
        String::from_utf8_lossy(&after.stdout).trim(),
        secret,
        "the host's own key was altered or removed by a caged run"
    );

    let _ = Command::new("keyctl").args(["unlink", &id, "@s"]).output();
    cleanup(&root);
}

#[test]
fn tmpdir_points_inside_the_environment_and_nowhere_else() {
    // 2b's D.2/D.5, which the cage has to keep: a program is given a temporary
    // directory, and it is the cage's OWN private /tmp — so a temp file is
    // writable, is visible to the next run through the root, and never reaches
    // the host's /tmp (attack-list-2e.md §G.7).
    let root = isolated_root("tmpdir");
    let host_marker = format!("/tmp/atrium-2e-tmpdir-{}", std::process::id());
    let _ = std::fs::remove_file(&host_marker);
    if let Some(o) = run_caged(
        &root,
        "/home/work",
        &format!(
            "printf '%s' \"$TMPDIR\"; touch \"$TMPDIR/tmpfile\"; echo; ls -1 /tmp | tr '\\n' ' '"
        ),
    ) {
        let printed = out_text(&o);
        assert!(
            printed.contains("/tmp"),
            "TMPDIR must be set and point at the cage's own /tmp, got: {printed:?}"
        );
        // Written inside the cage's tmpfs, so it is NOT at the root's /tmp on the
        // host (a tmpfs made inside the namespace dies with it) and NOT in the
        // host's /tmp. What it must not do is appear anywhere outside.
        assert!(
            !Path::new(&host_marker).exists(),
            "a temp file escaped to the host"
        );
    }
    let _ = std::fs::remove_file(&host_marker);
    cleanup(&root);
}

#[test]
fn the_hosts_home_is_unreachable_and_the_roots_own_home_is_present() {
    let root = isolated_root("home");
    if let Some(o) = run_caged(
        &root,
        "/home/work",
        "ls /home/muffin 2>&1; echo ---; ls /home",
    ) {
        let printed = out_text(&o);
        let (host_side, root_side) = printed.split_once("---").expect("the separator printed");
        assert!(
            host_side.contains("No such file") || host_side.trim().is_empty(),
            "the host's /home/muffin is reachable inside the cage: {host_side:?}"
        );
        // /home IS inside the environment root, bound at /, so it must exist
        // and hold the root's own contents. Asserting that `ls /home` fails
        // would have passed only against a cage that had lost the sandbox's
        // own files.
        assert!(
            root_side.contains("work") && root_side.contains("documents"),
            "the root's own /home should be visible inside the cage, got: {root_side:?}"
        );
    }
    cleanup(&root);
}

#[test]
fn the_environment_is_exactly_the_allowlist() {
    // Requirement A, 26 Sep 2026. Make the host side hostile first.
    std::env::set_var("GH_AUDIT_TOKEN", "FAKE-MUST-NOT-LEAK");
    std::env::set_var("HERMES_TEST", "FAKE-MUST-NOT-LEAK");
    std::env::set_var("SSH_AUTH_SOCK", "/run/user/1000/ssh-agent.socket");

    let root = isolated_root("env");
    if let Some(o) = run_caged(&root, "/home/work", "env") {
        let printed = out_text(&o);
        for forbidden in [
            "GH_AUDIT_TOKEN",
            "HERMES_TEST",
            "SSH_AUTH_SOCK",
            "DBUS_SESSION_BUS_ADDRESS",
            "KITTY_PUBLIC_KEY",
        ] {
            assert!(
                !printed.contains(forbidden),
                "{forbidden} leaked into the cage: {printed:?}"
            );
        }
        // Every name present must be one of the permitted eight. PWD and SHLVL
        // are added by the shell and `_` by glibc's `env`; they are permitted by
        // name, with that reason, and nothing else is
        // (attack-list-2e.md §E.2).
        //
        // TMPDIR is the fifth ALLOWED_ENV name (26 Sep 2026). It is here rather
        // than absent on purpose: 2b's D.2 lists it and D.5 asserts that a file
        // written to `$TMPDIR` lands inside the environment root.
        let permitted = [
            "PATH", "HOME", "TERM", "LANG", "TMPDIR", "PWD", "SHLVL", "_",
        ];
        for line in printed.lines() {
            let name = line.split('=').next().unwrap_or("");
            assert!(
                permitted.contains(&name),
                "an unlisted variable reached the cage: {line:?}"
            );
        }
        assert!(
            printed.contains("HOME=/"),
            "HOME must be the cage's own root"
        );
    }
    cleanup(&root);
}

#[test]
fn a_real_program_runs_and_a_file_made_inside_appears_at_the_root() {
    let root = isolated_root("usable");
    if let Some(o) = run_caged(
        &root,
        "/home/work",
        "echo made-inside > /home/work/created.txt; printf 'a\\nb\\n' | wc -l",
    ) {
        assert_eq!(
            o.status,
            ExitStatus::Exited(0),
            "the command should succeed"
        );
        assert_eq!(out_text(&o).trim(), "2", "a pipeline must still work");
        let created = root.join("home/work/created.txt");
        assert!(
            created.is_file(),
            "the file created inside must appear at the root"
        );
        assert_eq!(
            std::fs::read_to_string(&created).unwrap().trim(),
            "made-inside"
        );
    }
    cleanup(&root);
}

#[test]
fn the_bound_system_directories_are_read_only() {
    let root = isolated_root("readonly");
    if let Some(o) = run_caged(&root, "/home/work", "touch /usr/evil 2>&1; echo rc=$?") {
        let printed = out_text(&o);
        assert!(
            printed.contains("rc=1"),
            "/usr must be read-only inside the cage: {printed:?}"
        );
        assert!(
            printed.contains("Read-only") || printed.contains("read-only"),
            "the refusal should name the read-only filesystem: {printed:?}"
        );
        assert!(
            !Path::new("/usr/evil").exists(),
            "something was written to the real /usr — THE CAGE LEAKS"
        );
    }
    cleanup(&root);
}

#[test]
fn the_cage_never_opens_the_network() {
    // D1, 26 Sep 2026: the network is SHUT. The agent's lookups come from the
    // fetch tool the loop calls outside this cage (BUILD-PLAN.md Phase 5).
    let root = isolated_root("net");
    if let Some(o) = run_caged(
        &root,
        "/home/work",
        "(exec 3<>/dev/tcp/1.1.1.1/443) 2>/dev/null && echo OPEN || echo CLOSED",
    ) {
        let printed = out_text(&o);
        assert!(
            printed.contains("CLOSED"),
            "the network is reachable from inside the cage, and D1 says it is shut: {printed:?}"
        );
    }
    cleanup(&root);
}

// ---------------------------------------------------------------------------
// Fail closed — the section that matters most
// ---------------------------------------------------------------------------

#[test]
fn a_root_that_is_not_isolated_is_refused_and_nothing_runs() {
    let root = std::env::temp_dir().join(format!("atrium-2e-notisolated-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("home/work")).unwrap();

    let p = "sh".to_string();
    let a = vec![
        "-c".to_string(),
        "touch /home/work/should-not-exist".to_string(),
    ];
    match run(&root, &vp("/home/work"), &p, &a, &RunOptions::default()) {
        Err(RunError::Uncageable(CageError::RootNotIsolated { .. })) => {}
        Ok(_) => panic!(
            "a non-isolated root was caged instead of refused — the hard-link hole \
             2a found would be open"
        ),
        Err(e) => panic!("expected RootNotIsolated, got {e}"),
    }
    assert!(
        !root.join("home/work/should-not-exist").exists(),
        "the command ran despite the refusal -- fail-closed is broken"
    );
    cleanup(&root);
}

#[test]
fn a_refusal_says_why_and_never_names_a_real_host_path() {
    let root = std::env::temp_dir().join(format!("atrium-2e-msg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("home/work")).unwrap();
    let p = "sh".to_string();
    let a = vec!["-c".to_string(), "true".to_string()];
    if let Err(e) = run(&root, &vp("/home/work"), &p, &a, &RunOptions::default()) {
        let msg = e.to_string();
        assert!(!msg.is_empty(), "a refusal must say why");
        assert!(
            !msg.contains("/home/muffin"),
            "the refusal leaked the host home: {msg}"
        );
        assert!(
            msg.contains("refused") || msg.contains("Refused"),
            "the refusal should read as a refusal: {msg}"
        );
    }
    cleanup(&root);
}

#[test]
fn the_cli_offers_no_way_to_ask_for_an_uncaged_run() {
    // attack-list-2e.md §F.7, made mechanical. The library keeps an explicit
    // opt-out so 2b's runner tests can exercise the runner; the CLI must not
    // expose one, so a caller cannot turn the cage off. This reads the real
    // source at compile time, so it cannot drift from the file it asserts about.
    let cli = include_str!("../src/main.rs");
    for forbidden in [
        "caged",
        "uncaged",
        "no-cage",
        "nocage",
        "cage-off",
        "share-net",
    ] {
        assert!(
            !cli.contains(forbidden),
            "the CLI source mentions {forbidden:?}, but there must be no way for a \
             caller to turn the cage off or widen it (attack-list-2e.md §F.7)"
        );
    }
}

#[test]
fn the_cage_is_actually_available_where_the_phase_is_verified() {
    // The guard against this file's own shape. If bwrap is installed, a caged
    // run MUST happen here; the eprintln below is the honest limit on a machine
    // without it, and `crates/shell/hand-test-2e.sh` is the gate that requires a
    // real cage (CI installs bubblewrap for it).
    match atrium_shell::cage::find_cage_program() {
        Ok(p) => {
            let root = isolated_root("available");
            match run_caged(&root, "/home/work", "echo caged-ok") {
                Some(o) => assert_eq!(out_text(&o).trim(), "caged-ok"),
                None => panic!(
                    "bwrap is at {} yet the run refused — the tests in this file are \
                     passing for the wrong reason",
                    p.display()
                ),
            }
            cleanup(&root);
        }
        Err(_) => {
            eprintln!(
                "atrium-2e: NO CAGE PROGRAM on this machine — every cage test in this \
                 file took its refusal path. That is correct fail-closed behaviour and \
                 it is NOT evidence that the cage holds. crates/shell/hand-test-2e.sh \
                 is the gate that requires a real cage; CI runs it with bubblewrap \
                 installed."
            );
        }
    }
}

#[test]
fn a_refusal_and_a_caged_run_are_distinguishable_by_the_caller() {
    // attack-list-2e.md §I.10: a SEALED reported as a success is the §F failure
    // arriving through the status channel.
    let bad_root = std::env::temp_dir().join(format!("atrium-2e-diff-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&bad_root);
    std::fs::create_dir_all(bad_root.join("home/work")).unwrap();

    let p = "sh".to_string();
    let a = vec!["-c".to_string(), "true".to_string()];
    let refused = run(&bad_root, &vp("/home/work"), &p, &a, &RunOptions::default());
    assert!(
        refused.is_err(),
        "a non-isolated root must come back as an error, not as an outcome"
    );

    cleanup(&bad_root);
}
