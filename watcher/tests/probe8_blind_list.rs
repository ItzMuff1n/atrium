//! Probe 8 — the blind list's candidate defects, run against the real crate.
//!
//! Kept as a test file so it can use the library directly (`#[ignore]`-free; these
//! run with the suite). Each test is named for the blind list item it answers, so
//! the mapping survives.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::time::Duration;

use atrium_watcher::{Change, Options, Watcher};

struct Fx {
    base: PathBuf,
    root: PathBuf,
}
impl Fx {
    fn new(n: &str) -> Fx {
        let base =
            std::env::temp_dir().join(format!("atrium-2d-probe8-{n}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("root");
        std::fs::create_dir_all(root.join("home/documents")).unwrap();
        std::fs::create_dir_all(base.join("outside")).unwrap();
        Fx { base, root }
    }
    fn outside(&self) -> PathBuf {
        self.base.join("outside")
    }
    fn start(&self) -> Watcher {
        Watcher::start(&Options::new(&self.root)).unwrap()
    }
}
impl Drop for Fx {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn collect(w: &mut Watcher, ms: u64) -> Vec<Change> {
    let end = std::time::Instant::now() + Duration::from_millis(ms);
    let mut all = Vec::new();
    while std::time::Instant::now() < end {
        all.extend(w.drain());
        std::thread::sleep(Duration::from_millis(10));
    }
    all
}

fn lines(v: &[Change]) -> Vec<String> {
    v.iter().map(|c| c.line()).collect()
}

// ---------------------------------------------------------------------------
// Blind §1.3 — a POPULATED subtree moved INTO the root.
// ---------------------------------------------------------------------------
#[test]
fn blind_1_3_populated_subtree_moved_into_the_root_is_watched_afterwards() {
    let f = Fx::new("movedin");
    // Build the tree OUTSIDE the root first, with nested subdirectories.
    let src = f.outside().join("incoming");
    std::fs::create_dir_all(src.join("sub/deeper")).unwrap();
    std::fs::write(src.join("top.txt"), b"t").unwrap();
    std::fs::write(src.join("sub/mid.txt"), b"m").unwrap();
    std::fs::write(src.join("sub/deeper/deep.txt"), b"d").unwrap();

    let mut w = f.start();
    let _ = w.drain();

    // Move the whole populated tree in.
    std::fs::rename(&src, f.root.join("home/incoming")).unwrap();
    let after_move = collect(&mut w, 400);
    eprintln!("probe8 §1.3 after the move-in: {:?}", lines(&after_move));

    // Now change a file in each level of the moved-in tree.
    std::fs::write(f.root.join("home/incoming/top2.txt"), b"x").unwrap();
    std::fs::write(f.root.join("home/incoming/sub/mid2.txt"), b"x").unwrap();
    std::fs::write(f.root.join("home/incoming/sub/deeper/deep2.txt"), b"x").unwrap();
    let after = collect(&mut w, 700);
    eprintln!(
        "probe8 §1.3 after writing in each level: {:?}",
        lines(&after)
    );

    let seen = |name: &str| {
        after.iter().any(|c| {
            c.path()
                .map(|p| p.to_string_lossy().ends_with(name))
                .unwrap_or(false)
        })
    };
    assert!(
        seen("top2.txt"),
        "level 1 of the moved-in tree is not watched"
    );
    assert!(
        seen("mid2.txt"),
        "level 2 of the moved-in tree is NOT watched — the move-in only attached a \
         watch to the top directory: {:?}",
        lines(&after)
    );
    assert!(
        seen("deep2.txt"),
        "level 3 of the moved-in tree is NOT watched: {:?}",
        lines(&after)
    );
}

// ---------------------------------------------------------------------------
// Blind §4.3 — wd reuse after rapid delete/recreate, cross-labelling.
// ---------------------------------------------------------------------------
#[test]
fn blind_4_3_rapid_directory_recreation_does_not_cross_label_events() {
    let f = Fx::new("wdreuse");
    let mut w = f.start();
    let _ = w.drain();

    // Delete and recreate many directories quickly, so the kernel is likely to
    // hand out recycled watch descriptors.
    for i in 0..40 {
        let d = f.root.join(format!("home/churn{i}"));
        std::fs::create_dir_all(d.join("inner")).unwrap();
        std::fs::write(d.join("inner/f.txt"), b"1").unwrap();
        std::fs::remove_dir_all(&d).unwrap();
    }
    let _ = collect(&mut w, 300);

    // Now, on a settled tree, write a uniquely-named file and check it is named
    // correctly and nowhere else.
    std::fs::write(f.root.join("home/documents/final.txt"), b"x").unwrap();
    let after = collect(&mut w, 600);
    let created = after.iter().find(|c| {
        matches!(c, Change::Created { .. })
            && c.path()
                .map(|p| p.to_string_lossy().ends_with("final.txt"))
                .unwrap_or(false)
    });
    assert!(
        created.is_some(),
        "the final file was not reported: {:?}",
        lines(&after)
    );
    let p = created
        .unwrap()
        .path()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert!(
        p.starts_with("/home/documents/") && !p.contains("churn"),
        "an event was labelled with a recycled watch descriptor's old path: {p}"
    );
}

// ---------------------------------------------------------------------------
// Blind §5.6 — a FIFO in the root must not wedge the watcher.
// ---------------------------------------------------------------------------
#[test]
fn blind_5_6_a_fifo_does_not_wedge_the_watcher() {
    let f = Fx::new("fifo");
    let mk = std::process::Command::new("mkfifo")
        .arg(f.root.join("home/documents/pipe"))
        .status()
        .unwrap();
    assert!(mk.success(), "mkfifo failed, so this probe proves nothing");

    // The watcher must start and drain without ever opening the FIFO. If it did
    // open it for reading, this call would block forever and the test would hang
    // rather than fail — which is why it is worth running.
    let mut w = f.start();
    let evs = collect(&mut w, 300);
    eprintln!("probe8 §5.6 after creating a FIFO: {:?}", lines(&evs));

    // And a normal change afterwards is still reported.
    std::fs::write(f.root.join("home/documents/after-fifo.txt"), b"x").unwrap();
    let after = collect(&mut w, 600);
    assert!(
        after.iter().any(|c| c
            .path()
            .map(|p| p.to_string_lossy().ends_with("after-fifo.txt"))
            .unwrap_or(false)),
        "the watcher stopped working after a FIFO appeared: {:?}",
        lines(&after)
    );
}

// ---------------------------------------------------------------------------
// Blind §5.5 — TOCTOU: a directory swapped for an outside symlink.
// ---------------------------------------------------------------------------
#[test]
fn blind_5_5_a_directory_swapped_for_an_outside_symlink_is_not_followed() {
    let f = Fx::new("toctou");
    // Simulate the post-swap state: a path that is now a symlink pointing outside,
    // where a directory used to be.
    let d = f.root.join("home/swapme");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::remove_dir(&d).unwrap();
    std::os::unix::fs::symlink(f.outside(), &d).unwrap();

    let mut w = f.start();
    let _ = w.drain();
    // Write into the target. With IN_DONT_FOLLOW on add_watch and symlink_metadata
    // on the walk, nothing outside may be reported.
    std::fs::write(f.outside().join("host.txt"), b"secret").unwrap();
    let all = collect(&mut w, 500);
    let leaked: Vec<String> = all
        .iter()
        .filter(|c| !c.is_fault())
        .map(|c| c.line())
        .collect();
    assert!(
        leaked.is_empty(),
        "an outside write was reported through a swapped symlink: {leaked:?}"
    );
}

// ---------------------------------------------------------------------------
// Blind §3.2 — a deep tree (long reconstructed paths).
// ---------------------------------------------------------------------------
#[test]
fn blind_3_2_a_deep_tree_is_reported_intact() {
    let f = Fx::new("deep");
    // ~60 levels of 8-character names = ~540 bytes of path. Well under PATH_MAX
    // (4096), but deep enough that a fixed-size buffer would truncate.
    let mut p = f.root.join("home/documents");
    for i in 0..60 {
        p = p.join(format!("lvl{i:05}"));
    }
    std::fs::create_dir_all(&p).unwrap();

    // The watcher starts FIRST, then the file is written — otherwise the change
    // predates every watch and zero events is the correct result, not a defect.
    let mut w = f.start();
    let _ = w.drain();
    let mk = std::process::Command::new("bash")
        .arg("-c")
        .arg(format!("echo deep > '{}'", p.join("leaf.txt").display()))
        .status()
        .unwrap();
    assert!(mk.success());

    let all = collect(&mut w, 600);
    let hit = all.iter().find(|c| {
        c.path()
            .map(|q| q.to_string_lossy().ends_with("leaf.txt"))
            .unwrap_or(false)
    });
    assert!(
        hit.is_some(),
        "a file 60 levels down was not watched: {}",
        lines(&all).len()
    );
    let path = hit.unwrap().path().unwrap().to_string_lossy().into_owned();
    assert!(
        path.ends_with("lvl00059/leaf.txt"),
        "the deep path was truncated or misassembled: {path}"
    );
}

// ---------------------------------------------------------------------------
// Blind §1.8 — atomic-replace (write tmp, rename over the target).
// ---------------------------------------------------------------------------
#[test]
fn blind_1_8_atomic_replace_is_reported_as_a_move_not_silence() {
    let f = Fx::new("atomic");
    let target = f.root.join("home/documents/config.txt");
    std::fs::write(&target, b"old\n").unwrap();
    let mut w = f.start();
    let _ = w.drain();

    let tmp = f.root.join("home/documents/.config.tmp");
    std::fs::write(&tmp, b"new\n").unwrap();
    std::fs::rename(&tmp, &target).unwrap();

    let all = collect(&mut w, 600);
    eprintln!("probe8 §1.8 atomic replace: {:?}", lines(&all));
    // The target's content changed; the report must say something about it. It is
    // not silence, and it is a MOVETO onto the target's path.
    assert!(
        all.iter().any(|c| c
            .path()
            .map(|p| p.to_string_lossy().ends_with("config.txt"))
            .unwrap_or(false)),
        "an atomic replace produced no record naming the changed file: {:?}",
        lines(&all)
    );
}

// ---------------------------------------------------------------------------
// Blind §1.7 — create in a dir, then rename the dir, before processing.
// ---------------------------------------------------------------------------
#[test]
fn blind_1_7_a_file_whose_directory_is_renamed_before_processing_is_reported_once() {
    let f = Fx::new("renamedir");
    let mut w = f.start();
    let _ = w.drain();

    let d = f.root.join("home/documents/d");
    std::fs::create_dir(&d).unwrap();
    std::fs::write(d.join("f.txt"), b"x").unwrap();
    std::fs::rename(&d, f.root.join("home/documents/e")).unwrap();

    let all = collect(&mut w, 700);
    eprintln!("probe8 §1.7 create-then-rename-dir: {:?}", lines(&all));
    // Every record that names f.txt must name it under a path that could have been
    // real at the moment of the event (d/f.txt before the move, e/f.txt after).
    for l in lines(&all) {
        if l.contains("f.txt") {
            assert!(
                l.contains("/d/f.txt") || l.contains("/e/f.txt"),
                "a record names a path that never existed: {l}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Blind §2.3 — delete + immediate recreate of a watched directory (watch death
// vs path gone).
// ---------------------------------------------------------------------------
#[test]
fn blind_2_3_delete_and_recreate_a_directory_keeps_reporting_afterwards() {
    let f = Fx::new("deldir");
    let d = f.root.join("home/documents/recreated");
    std::fs::create_dir(&d).unwrap();
    let mut w = f.start();
    let _ = w.drain();

    std::fs::remove_dir_all(&d).unwrap();
    std::fs::create_dir(&d).unwrap();
    let _ = collect(&mut w, 400);

    std::fs::write(d.join("after.txt"), b"x").unwrap();
    let after = collect(&mut w, 700);
    assert!(
        after.iter().any(|c| c
            .path()
            .map(|p| p.to_string_lossy().ends_with("after.txt"))
            .unwrap_or(false)),
        "after delete+recreate the path went deaf: {:?}",
        lines(&after)
    );
}

// ---------------------------------------------------------------------------
// Blind §6.2 — a directory explosion is reported, not silently half-covered.
// ---------------------------------------------------------------------------
#[test]
fn blind_6_2_a_wide_tree_covers_every_directory() {
    let f = Fx::new("wide");
    for i in 0..600 {
        std::fs::create_dir_all(f.root.join(format!("home/documents/w{i:04}"))).unwrap();
    }
    let mut w = f.start();
    let placed = w.live_watches();
    let _ = w.drain();

    // Write in the LAST-created directory: the one most likely to be missed.
    std::fs::write(f.root.join("home/documents/w0599/late.txt"), b"x").unwrap();
    let all = collect(&mut w, 800);
    assert!(
        w.unwatchable().is_empty(),
        "some directories could not be watched and were recorded: {:?}",
        w.unwatchable()
    );
    assert!(
        all.iter().any(|c| c
            .path()
            .map(|p| p.to_string_lossy().ends_with("late.txt"))
            .unwrap_or(false)),
        "{placed} watches placed but the last directory is blind: {:?}",
        lines(&all)
    );
}

// ---------------------------------------------------------------------------
// Blind §5.4 — an error path must not print the real root either.
// ---------------------------------------------------------------------------
#[test]
fn blind_5_4_no_error_path_prints_the_real_root() {
    let f = Fx::new("errpath");
    let sealed = f.root.join("home/sealed");
    std::fs::create_dir(&sealed).unwrap();
    std::fs::set_permissions(&sealed, std::os::unix::fs::PermissionsExt::from_mode(0o000)).unwrap();

    let mut w = f.start();
    let all = collect(&mut w, 400);
    std::fs::set_permissions(&sealed, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();

    let real = f.root.to_string_lossy().into_owned();
    for c in &all {
        let l = c.line();
        assert!(
            !l.contains(&real),
            "an error record leaked the real root path: {l}"
        );
    }
    assert!(
        all.iter().any(|c| matches!(c, Change::Error { .. })),
        "the sealed directory produced no error record at all: {:?}",
        lines(&all)
    );
}

// Silence the unused-import warning when a helper is not used on some paths.
#[allow(dead_code)]
fn _u() {
    let _ = OsStr::from_bytes(b"");
}
