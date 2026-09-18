//! Phase 2d tests — `attack-list-2d.md`, line by line where a line can be a test.
//!
//! In-process rather than through the CLI, because some of these cannot be
//! expressed as arguments: a non-UTF-8 filename, a rename pair's cookie, a
//! directory moved out of the root, and an event whose path must not be reported.
//!
//! **A test here is weak evidence.** It shares the assumptions of the code it
//! tests, including the wrong ones (`AGENT-RULES.md` §1). The hands-on harness is
//! the gate; these are the regression net that keeps a fixed defect fixed.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use atrium_watcher::{Change, GoneReason, Options, WatchError, Watcher};

/// A throwaway root, removed on drop.
struct Fixture {
    base: PathBuf,
    root: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Fixture {
        let base =
            std::env::temp_dir().join(format!("atrium-2d-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("root");
        std::fs::create_dir_all(root.join("home/documents/sub/deep")).unwrap();
        std::fs::write(root.join("home/documents/notes.txt"), b"first\n").unwrap();
        std::fs::create_dir_all(base.join("outside")).unwrap();
        Fixture { base, root }
    }

    fn outside(&self) -> PathBuf {
        self.base.join("outside")
    }

    fn start(&self) -> Watcher {
        let mut o = Options::new(&self.root);
        o.settle_ms = 50;
        Watcher::start(&o).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

/// Collect every change until a predicate is satisfied, or time out.
///
/// Used instead of a fixed sleep so a test is not flaky on a loaded machine, while
/// still failing if the watcher reports *nothing*.
fn wait_for<F: Fn(&[Change]) -> bool>(w: &mut Watcher, want: F, ms: u64) -> Vec<Change> {
    let deadline = std::time::Instant::now() + Duration::from_millis(ms);
    let mut all = Vec::new();
    loop {
        all.extend(w.drain());
        if want(&all) || std::time::Instant::now() >= deadline {
            return all;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Every change until a deadline, for the NOT-REPORTS cases.

/// Drain briefly right after `start`, discarding what arrives.
///
/// Used by the tests whose claim is "nothing appeared". A close of a handle that
/// opened before the watcher started arrives just after it (measured: `mask = 0x8`
/// alone) and is reported on purpose — see `j9`, which pins that boundary. This
/// absorbs that one startup artifact so the assertion is about the window after it,
/// rather than being a race the test cannot win.
fn settle_after_start(w: &mut Watcher) {
    let _ = collect_for(w, 40);
}

fn collect_for(w: &mut Watcher, ms: u64) -> Vec<Change> {
    let deadline = std::time::Instant::now() + Duration::from_millis(ms);
    let mut all = Vec::new();
    while std::time::Instant::now() < deadline {
        all.extend(w.drain());
        std::thread::sleep(Duration::from_millis(10));
    }
    all
}

fn vpath(c: &Change) -> Option<String> {
    c.path().map(|p| p.to_string_lossy().into_owned())
}

fn has_create(all: &[Change], suffix: &str) -> bool {
    all.iter().any(|c| {
        matches!(c, Change::Created { .. })
            && vpath(c).map(|p| p.ends_with(suffix)).unwrap_or(false)
    })
}

fn has_modified(all: &[Change], suffix: &str) -> bool {
    all.iter().any(|c| {
        matches!(c, Change::Modified { .. })
            && vpath(c).map(|p| p.ends_with(suffix)).unwrap_or(false)
    })
}

fn has_deleted(all: &[Change], suffix: &str) -> bool {
    all.iter().any(|c| {
        matches!(c, Change::Deleted { .. })
            && vpath(c).map(|p| p.ends_with(suffix)).unwrap_or(false)
    })
}

// ---------------------------------------------------------------------------
// §A — the plain case, including the plan's own verification step
// ---------------------------------------------------------------------------

#[test]
fn a1_a_shell_command_creating_a_file_is_noticed() {
    // "Run a shell command that creates a file. Confirm the watcher noticed."
    let f = Fixture::new("a1");
    let mut w = f.start();
    let _ = w.drain();

    let out = f.root.join("home/documents/by-command.txt");
    let st = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("echo hi > '{}'", out.display()))
        .status()
        .unwrap();
    assert!(st.success());

    let all = wait_for(&mut w, |a| has_modified(a, "by-command.txt"), 3000);
    assert!(
        has_create(&all, "by-command.txt"),
        "no CREATE for a file made by a shell command: {all:?}"
    );
    assert!(
        has_modified(&all, "by-command.txt"),
        "the command's write was not reported: {all:?}"
    );
}

#[test]
fn a6_chmod_is_reported_as_attrib() {
    let f = Fixture::new("a6");
    let mut w = f.start();
    let _ = w.drain();
    let p = f.root.join("home/documents/notes.txt");
    let mut perms = std::fs::metadata(&p).unwrap().permissions();
    perms.set_mode(0o600);
    std::fs::set_permissions(&p, perms).unwrap();
    let all = wait_for(
        &mut w,
        |a| a.iter().any(|c| matches!(c, Change::Attributed { .. })),
        3000,
    );
    assert!(
        all.iter().any(|c| matches!(c, Change::Attributed { .. })),
        "a permission change was not reported: {all:?}"
    );
}

// ---------------------------------------------------------------------------
// §B — the reduction, measured. These fix the correction in the attack list.
// ---------------------------------------------------------------------------

#[test]
fn b1_one_open_many_writes_is_one_modified() {
    use std::io::Write;
    let f = Fixture::new("b1");
    let mut w = f.start();
    let _ = w.drain();
    let p = f.root.join("home/documents/big.bin");
    {
        let mut fh = std::fs::File::create(&p).unwrap();
        for i in 0..10 {
            fh.write_all(format!("chunk {i}\n").repeat(256).as_bytes())
                .unwrap();
            fh.flush().unwrap();
        }
    } // closed once
    let all = wait_for(&mut w, |a| has_modified(a, "big.bin"), 3000);
    let n = all
        .iter()
        .filter(|c| matches!(c, Change::Modified { .. }) && vpath(c).unwrap().ends_with("big.bin"))
        .count();
    assert_eq!(
        n, 1,
        "one open/close cycle should be exactly one Modified, got {n}: {all:?}"
    );
}

#[test]
fn b2_three_separate_edits_are_three_modified() {
    // The correction in attack-list-2d.md §B. Three open/close cycles are three
    // changes; collapsing them would be the watcher losing two.
    let f = Fixture::new("b2");
    let mut w = f.start();
    let _ = w.drain();
    let p = f.root.join("home/documents/churn.txt");
    for i in 0..3 {
        std::fs::write(&p, format!("edit {i}\n")).unwrap();
        std::thread::sleep(Duration::from_millis(30));
    }
    let all = wait_for(
        &mut w,
        |a| {
            a.iter()
                .filter(|c| {
                    matches!(c, Change::Modified { .. })
                        && vpath(c).map(|p| p.ends_with("churn.txt")).unwrap_or(false)
                })
                .count()
                >= 3
        },
        3000,
    );
    let n = all
        .iter()
        .filter(|c| {
            matches!(c, Change::Modified { .. })
                && vpath(c).map(|p| p.ends_with("churn.txt")).unwrap_or(false)
        })
        .count();
    assert_eq!(
        n, 3,
        "three separate edits should be three Modified, got {n}: {all:?}"
    );
}

#[test]
fn b5_reading_a_file_is_not_a_change() {
    // §B.5/§B.6/§B.7: a read, a stat and a listing are not changes. This is what
    // keeps the stream usable — otherwise a snapshot or an explorer refresh would
    // flood it.
    //
    // **Diagnostic note, 15 Sep 2026.** This test flaked once in 8 consecutive
    // suite runs, reporting one `Modified /home/documents/notes.txt` in a window
    // where nothing wrote the file, and did not reproduce in 30 sequential rounds
    // or in 12 instrumented suite runs. Rather than assume, the test now checks
    // whether the file ACTUALLY changed, which separates the two possible faults:
    // a real write by something else (test-environment noise) from the watcher
    // inventing an event for an untouched file (a serious defect).
    let f = Fixture::new("b5");
    let mut w = f.start();
    settle_after_start(&mut w);
    let p = f.root.join("home/documents/notes.txt");
    let before = std::fs::read(&p).unwrap();
    let before_md = std::fs::metadata(&p).unwrap();
    let _ = std::fs::read(&p).unwrap();
    let _ = std::fs::metadata(&p).unwrap();
    let _ = std::fs::read_dir(f.root.join("home/documents"))
        .unwrap()
        .count();
    let all = collect_for(&mut w, 400);
    let after = std::fs::read(&p).unwrap();
    let after_md = std::fs::metadata(&p).unwrap();

    if !all.is_empty() {
        let changed = before != after
            || before_md.len() != after_md.len()
            || before_md.modified().ok() != after_md.modified().ok();
        panic!(
            "a read/stat/list produced records: {all:?}\n\
             the file itself changed during the window: {changed}\n\
             (content before/after equal: {}, len {} -> {}, mtime changed: {})",
            before == after,
            before_md.len(),
            after_md.len(),
            before_md.modified().ok() != after_md.modified().ok()
        );
    }
}

#[test]
fn b8_create_then_delete_before_a_drain_reports_both() {
    let f = Fixture::new("b8");
    let mut w = f.start();
    let _ = w.drain();
    let p = f.root.join("home/documents/transient.txt");
    std::fs::write(&p, b"x").unwrap();
    std::fs::remove_file(&p).unwrap();
    let all = wait_for(&mut w, |a| has_deleted(a, "transient.txt"), 3000);
    assert!(
        has_create(&all, "transient.txt"),
        "the create was lost: {all:?}"
    );
    assert!(
        has_deleted(&all, "transient.txt"),
        "the delete was lost: {all:?}"
    );
}

// ---------------------------------------------------------------------------
// §C — the name is right, including names that are not text
// ---------------------------------------------------------------------------

#[test]
fn c4_non_utf8_name_survives_as_bytes() {
    // A filename is an arbitrary byte string. A lossy conversion would print the
    // replacement character and two distinct files could print identically (§C.5).
    let f = Fixture::new("c4");
    let mut w = f.start();
    let _ = w.drain();
    let bad = OsStr::from_bytes(b"bad-\xff\xfe-name.txt");
    let p = f.root.join("home/documents").join(bad);
    std::fs::write(&p, b"z").unwrap();
    let all = wait_for(&mut w, |a| has_create(a, "name.txt"), 3000);

    let found = all.iter().find(|c| {
        matches!(c, Change::Created { .. })
            && c.path()
                .map(|q| q.as_bytes().ends_with(b"-name.txt"))
                .unwrap_or(false)
    });
    let c = found.unwrap_or_else(|| panic!("non-UTF-8 name not reported: {all:?}"));
    let bytes = c.path().unwrap().as_bytes();
    assert!(
        bytes.contains(&0xff) && bytes.contains(&0xfe),
        "the raw bytes did not survive to the record: {bytes:?}"
    );
    // And the rendering is reversible rather than lossy.
    let line = c.line();
    assert!(
        line.contains("\\xFF") && line.contains("\\xFE"),
        "rendered line is lossy, so two names could collide: {line}"
    );
}

#[test]
fn c9_a_name_with_a_newline_does_not_break_the_one_record_per_line_rule() {
    let f = Fixture::new("c9");
    let mut w = f.start();
    let _ = w.drain();
    let name = OsStr::from_bytes(b"two\nlines.txt");
    std::fs::write(f.root.join("home/documents").join(name), b"x").unwrap();
    let all = wait_for(&mut w, |a| has_create(a, "lines.txt"), 3000);
    let c = all
        .iter()
        .find(|c| {
            matches!(c, Change::Created { .. })
                && vpath(c).map(|p| p.ends_with("lines.txt")).unwrap_or(false)
        })
        .unwrap_or_else(|| panic!("a name with a newline was not reported: {all:?}"));
    assert_eq!(
        c.line().lines().count(),
        1,
        "the rendered record spans more than one line: {:?}",
        c.line()
    );
}

// ---------------------------------------------------------------------------
// §C.8 / §H — the disclosure boundary
// ---------------------------------------------------------------------------

#[test]
fn c8_no_reported_line_ever_contains_the_real_root() {
    // DESIGN.md §3.1: the agent never learns the real path exists. Checked as
    // bytes, not by eye.
    let f = Fixture::new("c8");
    let mut w = f.start();
    let _ = w.drain();
    std::fs::write(f.root.join("home/documents/x.txt"), b"a").unwrap();
    std::fs::rename(
        f.root.join("home/documents/x.txt"),
        f.root.join("home/documents/sub/x.txt"),
    )
    .unwrap();
    std::fs::remove_file(f.root.join("home/documents/sub/x.txt")).unwrap();
    let all = collect_for(&mut w, 400);
    assert!(
        !all.is_empty(),
        "the fixture produced no changes at all, so this proves nothing"
    );
    let real = f.root.to_string_lossy().into_owned();
    for c in &all {
        let line = c.line();
        assert!(
            !line.contains(&real),
            "a reported line contains the root's real path: {line}"
        );
    }
}

#[test]
fn c9showreal_prints_real_paths_when_asked() {
    let f = Fixture::new("showreal");
    let mut o = Options::new(&f.root);
    o.show_real = true;
    let mut w = Watcher::start(&o).unwrap();
    let _ = w.drain();
    std::fs::write(f.root.join("home/documents/y.txt"), b"a").unwrap();
    let all = collect_for(&mut w, 400);
    let real = f.root.to_string_lossy().into_owned();
    assert!(
        all.iter().any(|c| c.line().contains(&real)),
        "--show-real did not print a real path: {all:?}"
    );
}

// ---------------------------------------------------------------------------
// §D — the tree: what is watched, and what is not
// ---------------------------------------------------------------------------

#[test]
fn d1_a_file_deep_in_the_tree_is_noticed() {
    let f = Fixture::new("d1");
    let mut w = f.start();
    let _ = w.drain();
    std::fs::write(f.root.join("home/documents/sub/deep/leaf-new.txt"), b"x").unwrap();
    let all = wait_for(&mut w, |a| has_create(a, "leaf-new.txt"), 3000);
    assert!(
        has_create(&all, "leaf-new.txt"),
        "three levels down was not watched: {all:?}"
    );
}

#[test]
fn d2_a_directory_created_later_gets_watched() {
    let f = Fixture::new("d2");
    let mut w = f.start();
    let _ = w.drain();
    std::fs::create_dir(f.root.join("home/later")).unwrap();
    // Give the watcher a turn to attach a watch, as any real use would.
    let _ = wait_for(&mut w, |a| has_create(a, "later"), 2000);
    std::fs::write(f.root.join("home/later/inside.txt"), b"x").unwrap();
    let all = wait_for(&mut w, |a| has_create(a, "inside.txt"), 3000);
    assert!(
        has_create(&all, "inside.txt"),
        "a new directory's contents were not watched: {all:?}"
    );
}

#[test]
fn d4_a_symlink_out_of_the_root_is_not_followed() {
    // The sharp one. Observed: without IN_DONT_FOLLOW the kernel follows the link
    // and a write inside the OUTSIDE target is reported — host activity leaking
    // into the sandbox's view.
    let f = Fixture::new("d4");
    std::os::unix::fs::symlink(f.outside(), f.root.join("home/link-out")).unwrap();
    let mut w = f.start();
    settle_after_start(&mut w);

    let np = f.root.join("home/documents/notes.txt");
    let before_md = std::fs::metadata(&np).unwrap();
    let before_inode = {
        use std::os::unix::fs::MetadataExt;
        before_md.ino()
    };

    std::fs::write(f.outside().join("host-side.txt"), b"secret").unwrap();

    let all = collect_for(&mut w, 500);
    if !all.is_empty() {
        let after_md = std::fs::metadata(&np).unwrap();
        use std::os::unix::fs::MetadataExt;
        panic!(
            "activity OUTSIDE the root was reported, so a watch was placed through a symlink: {all:?}\n\
             notes.txt inode {before_inode} -> {}, mtime {:?} -> {:?}, len {} -> {}",
            after_md.ino(),
            before_md.modified().ok(),
            after_md.modified().ok(),
            before_md.len(),
            after_md.len()
        );
    }
}

#[test]
fn d5_a_dangling_symlink_is_not_an_error() {
    let f = Fixture::new("d5");
    std::os::unix::fs::symlink("/nowhere/at/all", f.root.join("home/dead")).unwrap();
    let mut w = f.start();
    let all = collect_for(&mut w, 300);
    assert!(
        all.iter().all(|c| !c.is_fault()),
        "a dangling symlink produced a fault, but it is not an error: {all:?}"
    );
}

#[test]
fn d7_an_unwatchable_directory_is_reported_and_does_not_blind_the_rest() {
    let f = Fixture::new("d7");
    let sealed = f.root.join("home/sealed");
    std::fs::create_dir(&sealed).unwrap();
    std::fs::write(sealed.join("hidden.txt"), b"x").unwrap();
    std::fs::set_permissions(&sealed, std::fs::Permissions::from_mode(0o000)).unwrap();

    let mut w = f.start();

    // The rest of the tree must still work. The unwatchable report arrives on the
    // FIRST drain, so this test keeps every batch rather than discarding any —
    // discarding the first drain is exactly how that report would be missed.
    std::fs::write(f.root.join("home/documents/still-seen.txt"), b"x").unwrap();
    let mut all = w.drain();
    all.extend(wait_for(&mut w, |a| has_create(a, "still-seen.txt"), 3000));

    // Restore permissions so the fixture can be cleaned up.
    std::fs::set_permissions(&sealed, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert!(
        has_create(&all, "still-seen.txt"),
        "one unwatchable directory blinded the rest of the tree: {all:?}"
    );
    assert!(
        all.iter().any(|c| matches!(c, Change::Error { .. })),
        "an unwatchable directory was skipped in silence: {all:?}"
    );
}

// ---------------------------------------------------------------------------
// §E — moves: the cookie is the pairing, not the watch
// ---------------------------------------------------------------------------

#[test]
fn e1_move_within_one_directory_shares_a_cookie() {
    let f = Fixture::new("e1");
    let mut w = f.start();
    let _ = w.drain();
    std::fs::rename(
        f.root.join("home/documents/notes.txt"),
        f.root.join("home/documents/renamed.txt"),
    )
    .unwrap();
    let all = wait_for(
        &mut w,
        |a| a.iter().any(|c| matches!(c, Change::MovedTo { .. })),
        3000,
    );
    let from = all.iter().find_map(|c| match c {
        Change::MovedFrom { cookie, .. } => Some(*cookie),
        _ => None,
    });
    let to = all.iter().find_map(|c| match c {
        Change::MovedTo { cookie, .. } => Some(*cookie),
        _ => None,
    });
    assert_eq!(
        from, to,
        "the two halves of a move did not share a cookie: {all:?}"
    );
}

#[test]
fn e2_move_between_two_watched_directories_pair_by_cookie() {
    // Observed: the two halves can arrive from two DIFFERENT watches. Pairing
    // per-watch would report one move as two unrelated events.
    let f = Fixture::new("e2");
    let mut w = f.start();
    let _ = w.drain();
    std::fs::rename(
        f.root.join("home/documents/notes.txt"),
        f.root.join("home/documents/sub/moved-here.txt"),
    )
    .unwrap();
    let all = wait_for(
        &mut w,
        |a| a.iter().any(|c| matches!(c, Change::MovedTo { .. })),
        3000,
    );
    let from = all.iter().find_map(|c| match c {
        Change::MovedFrom { cookie, .. } => Some(*cookie),
        _ => None,
    });
    let to = all.iter().find_map(|c| match c {
        Change::MovedTo { cookie, .. } => Some(*cookie),
        _ => None,
    });
    assert!(from.is_some(), "no MOVEFROM: {all:?}");
    assert_eq!(
        from, to,
        "cross-directory move halves did not pair: {all:?}"
    );
}

#[test]
fn e3_move_out_of_the_root_reports_one_half_only() {
    let f = Fixture::new("e3");
    let mut w = f.start();
    let _ = w.drain();
    std::fs::rename(
        f.root.join("home/documents/notes.txt"),
        f.outside().join("left-the-environment.txt"),
    )
    .unwrap();
    let all = wait_for(
        &mut w,
        |a| a.iter().any(|c| matches!(c, Change::MovedFrom { .. })),
        3000,
    );
    assert!(
        all.iter().any(|c| matches!(c, Change::MovedFrom { .. })),
        "a move out of the root was not reported: {all:?}"
    );
    assert!(
        !all.iter().any(|c| matches!(c, Change::MovedTo { .. })),
        "a MOVETO was invented for a move out of the root: {all:?}"
    );
}

#[test]
fn e5_a_renamed_directory_reports_changes_under_its_new_path() {
    // Observed: after a rename the events keep arriving on the same wd, so the
    // table must be re-keyed or the file is reported at a path it no longer has.
    let f = Fixture::new("e5");
    let mut w = f.start();
    let _ = w.drain();
    std::fs::rename(
        f.root.join("home/documents/sub"),
        f.root.join("home/documents/subrenamed"),
    )
    .unwrap();
    let _ = wait_for(
        &mut w,
        |a| a.iter().any(|c| matches!(c, Change::MovedTo { .. })),
        2000,
    );
    std::fs::write(
        f.root.join("home/documents/subrenamed/deep/after.txt"),
        b"x",
    )
    .unwrap();
    let all = wait_for(&mut w, |a| has_create(a, "after.txt"), 3000);
    let p = all
        .iter()
        .find(|c| has_create(std::slice::from_ref(*c), "after.txt"))
        .map(|c| c.path().unwrap().to_string_lossy().into_owned())
        .unwrap_or_else(|| panic!("the file was not reported at all: {all:?}"));
    assert!(
        p.contains("subrenamed"),
        "a change was reported under the directory's OLD path after a rename: {p}"
    );
    assert!(
        !p.contains("/sub/"),
        "a change was reported under a path that no longer exists: {p}"
    );
}

// ---------------------------------------------------------------------------
// §J — the quiet failure modes
// ---------------------------------------------------------------------------

#[test]
fn j4_deleting_a_watched_directory_is_reported_as_gone() {
    let f = Fixture::new("j4");
    let mut w = f.start();
    let _ = w.drain();
    std::fs::remove_dir_all(f.root.join("home/documents/sub")).unwrap();
    let all = wait_for(
        &mut w,
        |a| a.iter().any(|c| matches!(c, Change::DirectoryGone { .. })),
        3000,
    );
    assert!(
        all.iter()
            .any(|c| matches!(c, Change::DirectoryGone { .. })),
        "a deleted watched directory was silent: {all:?}"
    );
}

#[test]
fn j5_a_directory_moved_out_of_the_root_is_not_reported_as_an_in_root_change() {
    // **This is the defect the probes found, and the reason `resync` exists.**
    //
    // Observed 15 Sep 2026: a watched directory (and its children's watches) moved
    // OUT of the root keeps every watch, and writes made outside the root then
    // arrive on those wds. A watcher trusting its stale table would name an in-root
    // path for activity that happened outside the environment.
    let f = Fixture::new("j5");
    let mut w = f.start();
    let _ = w.drain();

    std::fs::rename(f.root.join("home/documents/sub"), f.outside().join("taken")).unwrap();
    let moved = wait_for(
        &mut w,
        |a| a.iter().any(|c| matches!(c, Change::MovedFrom { .. })),
        3000,
    );
    assert!(
        moved.iter().any(|c| matches!(c, Change::MovedFrom { .. })),
        "the move out was not reported at all: {moved:?}"
    );

    // Now write INSIDE the moved-out subtree, which is now outside the root.
    std::fs::write(f.outside().join("taken/deep/host-side.txt"), b"secret").unwrap();

    let after = collect_for(&mut w, 600);
    let leaked: Vec<String> = after
        .iter()
        .filter(|c| !c.is_fault())
        .map(|c| c.line())
        .collect();
    assert!(
        leaked.is_empty(),
        "activity OUTSIDE the root was reported as an in-root change: {leaked:?}"
    );
    // And the watches whose directories left must have been retired, not left
    // silently in place. The retirement happens in the same batch as the move, so
    // both batches are checked — the earlier one is where it lands.
    let retired = moved
        .iter()
        .chain(after.iter())
        .any(|c| matches!(c, Change::DirectoryGone { .. }));
    assert!(
        retired,
        "the watch that followed the directory out of the root was not reported: \
         moved={moved:?} after={after:?}"
    );
}

#[test]
fn j8_the_watcher_reports_how_many_watches_it_holds() {
    let f = Fixture::new("j8");
    let w = f.start();
    // root, home, home/documents, home/documents/sub, home/documents/sub/deep = 5
    assert!(
        w.live_watches() >= 3,
        "expected a watch per directory, got {}",
        w.live_watches()
    );
}

// ---------------------------------------------------------------------------
// §G — refusals
// ---------------------------------------------------------------------------

#[test]
fn g1_a_missing_root_is_refused() {
    let o = Options::new("/tmp/atrium-2d-definitely-not-here-31337");
    match Watcher::start(&o) {
        Err(WatchError::Root { path, detail }) => {
            assert!(detail.to_lowercase().contains("no such file") || !detail.is_empty());
            assert!(path.to_string_lossy().contains("31337"));
        }
        Ok(_) | Err(WatchError::Usage(_)) => panic!("a missing root was not refused"),
    }
}

#[test]
fn g2_a_root_that_is_a_file_is_refused() {
    let f = Fixture::new("g2");
    let file = f.base.join("not-a-dir");
    std::fs::write(&file, b"x").unwrap();
    match Watcher::start(&Options::new(&file)) {
        Err(WatchError::Root { detail, .. }) => {
            assert!(
                detail.contains("not a directory"),
                "unexpected reason: {detail}"
            )
        }
        Ok(_) | Err(WatchError::Usage(_)) => panic!("a file as root was not refused"),
    }
}

#[test]
fn g3_a_relative_root_is_refused() {
    match Watcher::start(&Options::new("relative/path")) {
        Err(WatchError::Usage(msg)) => assert!(msg.contains("absolute"), "unexpected: {msg}"),
        Ok(_) | Err(WatchError::Root { .. }) => panic!("a relative root was not refused"),
    }
}

// ---------------------------------------------------------------------------
// §I — what the watcher cannot see, as tests where a test is possible
// ---------------------------------------------------------------------------

#[test]
fn i8_a_hard_link_change_from_outside_the_root_is_invisible() {
    // **Recorded, not fixed.** Observed: a change to an in-root inode made through
    // a hard link outside the root produces NO event. This is the 2a/§N hard-link
    // channel appearing in the watcher; it is closed by the root being its own
    // mount, which is 2e's work. The test asserts the current, honest behaviour so
    // that a later phase which closes it will see this test change.
    let f = Fixture::new("i8");
    let target = f.outside().join("target.txt");
    std::fs::write(&target, b"original").unwrap();
    std::fs::hard_link(&target, f.root.join("home/documents/hardlinked.txt")).unwrap();

    let mut w = f.start();
    let _ = w.drain();

    // Write through the OUTSIDE name: same inode, different name.
    std::fs::write(&target, b"changed through the outside name").unwrap();

    let all = collect_for(&mut w, 500);
    assert!(
        all.is_empty(),
        "a hard-link change reported events; if a phase closed this channel, update \
         attack-list-2d.md §I.8 rather than deleting this test: {all:?}"
    );
}

#[test]
fn i3_no_effects_are_emitted() {
    // This crate produces Change records and nothing else: no kind, no count, no
    // label, no SQLite, no log. Phase 3 owns the effect stream. Checked by the type
    // surface rather than by inspection — every variant here is an observation.
    let f = Fixture::new("i3");
    let mut w = f.start();
    let _ = w.drain();
    std::fs::write(f.root.join("home/documents/e.txt"), b"x").unwrap();
    let all = collect_for(&mut w, 300);
    for c in &all {
        let tag = c.tag();
        assert!(
            [
                "CREATE", "MODIF", "DELETE", "MOVEFROM", "MOVETO", "ATTRIB", "GONE", "OVERFLOW",
                "ERROR"
            ]
            .contains(&tag),
            "an unexpected record kind reached the stream: {tag}"
        );
    }
}

// ---------------------------------------------------------------------------
// §B/J — the startup straddle: a change in flight when the watches are placed
// ---------------------------------------------------------------------------

#[test]
fn j9_a_close_that_straddles_start_is_reported_and_that_boundary_is_deliberate() {
    // **The measured boundary, not an aspiration.**
    //
    // Probes 9 established the kernel behaviour precisely:
    //   * an open/write/close that COMPLETES before its watch exists delivers
    //     **nothing** — so a finished write cannot leak in;
    //   * but an open/write that lands before the watch and whose **close** lands
    //     after it delivers **`mask = 0x8` alone** — `IN_CLOSE_WRITE` with no
    //     `IN_CREATE` and no `IN_MODIFY`.
    //
    // That second case is a genuine ambiguity: the bytes were written before the
    // watcher existed, but the close — the event — happened after it. There is no
    // way to tell it apart from a write that happened a microsecond ago, and this
    // phase's whole purpose is not losing changes, so **it is reported**. The
    // alternative (dropping a close-write) would be the watcher losing a real
    // change, which is the failure this phase exists to prevent.
    //
    // This test pins the boundary so it cannot drift silently: such a record IS
    // reported, and changes made after start still are too. Recorded in
    // `watcher/README.md` and `attack-list-2d.md` §L.
    for round in 0..10 {
        let f = Fixture::new(&format!("j9-{round}"));

        // Opened and written BEFORE the watcher starts; closed after it.
        let mut held = Vec::new();
        for i in 0..3 {
            use std::io::Write;
            let mut fh =
                std::fs::File::create(f.root.join(format!("home/documents/held{i}"))).unwrap();
            fh.write_all(b"written before the watcher started").unwrap();
            fh.flush().unwrap();
            held.push(fh);
        }

        let mut w = f.start();
        drop(held); // the closes now arrive after start

        let all = collect_for(&mut w, 250);
        // The boundary: these are reported. If a future change starts suppressing
        // them, that is a lost-change decision and must not happen silently.
        let straddled: Vec<String> = all
            .iter()
            .filter(|c| {
                c.path()
                    .map(|p| p.to_string_lossy().contains("held"))
                    .unwrap_or(false)
            })
            .map(|c| c.line())
            .collect();
        assert!(
            !straddled.is_empty(),
            "round {round}: a close arriving after start was NOT reported. That is a \
             deliberate boundary (attack-list-2d.md §L) — if it is being changed, the \
             attack list and the README must change with it."
        );
        // And they must be Modified records, never a fabricated create or delete.
        for l in &straddled {
            assert!(
                l.starts_with("MODIF"),
                "round {round}: a straddled close produced something other than a \
                 modification: {l}"
            );
        }

        // A change made after start is still reported normally.
        std::fs::write(f.root.join("home/documents/after-start.txt"), b"x").unwrap();
        let later = collect_for(&mut w, 250);
        assert!(
            later.iter().any(|c| {
                c.path()
                    .map(|p| p.to_string_lossy().ends_with("after-start.txt"))
                    .unwrap_or(false)
            }),
            "round {round}: the watcher stopped reporting changes made after start: {:?}",
            later.iter().map(|c| c.line()).collect::<Vec<_>>()
        );
    }
}

// ---------------------------------------------------------------------------
// §K — the boring half
// ---------------------------------------------------------------------------

#[test]
fn k3_an_empty_root_produces_nothing_and_does_not_fault() {
    let base = std::env::temp_dir().join(format!("atrium-2d-empty-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let mut w = Watcher::start(&Options::new(&base)).unwrap();
    let all = collect_for(&mut w, 300);
    assert!(all.is_empty(), "an empty root produced records: {all:?}");
    assert_eq!(w.live_watches(), 1, "the root itself must be watched");
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn k4_a_large_root_is_walked_completely() {
    let f = Fixture::new("k4");
    for a in 0..20 {
        for b in 0..15 {
            std::fs::create_dir_all(f.root.join(format!("home/bulk/a{a}/b{b}"))).unwrap();
        }
    }
    let w = f.start();
    // 1 (root) + 1 (home) + 1 (documents) + 2 (sub, deep) + 300 bulk dirs
    assert!(
        w.live_watches() >= 300,
        "the walk missed directories: {} watches placed",
        w.live_watches()
    );
}

#[test]
fn k7_a_directory_gone_is_a_fault_so_a_run_cannot_be_green_while_blind() {
    // The property the CLI's exit code rests on: losing a watch must be a fault.
    let c = Change::DirectoryGone {
        dir: OsStr::from_bytes(b"/x").to_os_string(),
        reason: GoneReason::Vanished,
    };
    assert!(c.is_fault());
    let o = Change::Overflow { dropped_before: 1 };
    assert!(o.is_fault());
    let e = Change::Error { detail: "x".into() };
    assert!(e.is_fault());
    let ok = Change::Modified {
        path: OsStr::from_bytes(b"/x").to_os_string(),
    };
    assert!(!ok.is_fault());
}

#[test]
fn paths_are_virtual_and_rooted_at_slash() {
    let f = Fixture::new("virt");
    let w = f.start();
    let v = w.virtual_of(&f.root.join("home/documents/notes.txt"));
    assert_eq!(v.to_string_lossy(), "/home/documents/notes.txt");
    assert_eq!(w.virtual_of(&f.root).to_string_lossy(), "/");
    // And a path outside the root does not become a plausible-looking virtual path.
    let out = w.virtual_of(Path::new("/etc/passwd"));
    assert_eq!(out.to_string_lossy(), "<outside the environment>");
}
