//! Fixture lifecycle regressions — issues #59, #60, #62.
//!
//! Each test pins a failure that was **reproduced before it was fixed**, and the
//! reproduction is kept rather than only asserting the new shape. The old
//! behaviour is re-implemented here on purpose: a test that exercised only the new
//! code could not tell you whether the new code is what fixed anything.
//!
//! Measured before the fixes (this file's output, `--nocapture`):
//!
//! ```text
//! OLD  shape: 2 leaked on panic        <- #59
//! NEW  shape: 0 leaked
//! OLD  shape: stale corpse survives    <- #60
//! NEW  shape: swept
//! OLD  write: writable fd on the exec'd path = Some("fd 3 flags 02100001")   <- #62
//! NEW  write: none on the exec'd path
//! INJECT: exec failed: Text file busy (os error 26)
//! ```
//!
//! The `02100001` flags are the point of #62: `02000000` is `O_CLOEXEC`, so a
//! child drops the fd at its own `exec` — the exposure is that child's fork→exec
//! window, and any thread's fork can carry any thread's open write fd.
//!
//! ## Parallel safety, stated because the first version got it wrong
//!
//! These tests run in parallel with each other and with `shell_tests.rs`. The
//! first version counted every `atrium-2b-test-*` directory in `/tmp` before and
//! after, and had a helper that deleted all of them — which made four of seven
//! tests fail, including one that deleted a fixture another test was about to
//! exec (`ENOENT` instead of `ETXTBSY`). Nothing here counts global state or
//! removes anything it did not create: each assertion is about the exact path the
//! test itself built.
//!
//! ## Caveat
//!
//! `sweep_like_the_fix` duplicates the sweep rule from `shell_tests.rs` rather
//! than calling it, because that one is private to its test binary. If the rule
//! changes, this copy must change with it. It takes the prefix as a parameter so
//! this file can exercise the rule without touching the real fixture namespace.
//! The real sweep's prefix and threshold are asserted in their own test below.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

/// The real prefix, matching `shell_tests.rs`. Asserted in the source check below
/// rather than referenced here, which is why it needs the allow.
#[allow(dead_code)]
const REAL_PREFIX: &str = "atrium-2b-test-";
const REAL_STALE_AFTER_SECS: u64 = 60 * 60;

fn tag(prefix: &str) -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    format!(
        "{prefix}{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    )
}

// ------------------------------------------------ the shape that had the bugs

/// Exactly what `shell_tests.rs` did before these fixes: sweep with a discarded
/// error, build by hand, hand back plain paths, and rely on the test body calling
/// `cleanup()` at the end.
fn old_make_root(prefix: &str) -> (PathBuf, PathBuf) {
    let t = tag(prefix);
    let root = std::env::temp_dir().join(&t);
    let outside = std::env::temp_dir().join(format!("{t}-outside"));
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&outside);
    std::fs::create_dir_all(root.join("home/work")).unwrap();
    std::fs::create_dir_all(root.join("trap")).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("trap/escape")).unwrap();
    std::fs::create_dir_all(outside.join("sub")).unwrap();
    std::fs::write(outside.join("sentinel.txt"), b"sentinel\n").unwrap();
    (root, outside)
}

fn old_cleanup(root: &Path, outside: &Path) {
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(outside);
}

/// The shape the fix introduced: `Drop` removes both paths, so the removal cannot
/// be skipped by a panicking test body.
struct Guard {
    root: PathBuf,
    outside: PathBuf,
}

impl Drop for Guard {
    fn drop(&mut self) {
        for p in [&self.root, &self.outside] {
            let _ = std::fs::remove_dir_all(p);
        }
    }
}

fn new_make_root(prefix: &str) -> Guard {
    let t = tag(prefix);
    let root = std::env::temp_dir().join(&t);
    let outside = std::env::temp_dir().join(format!("{t}-outside"));
    std::fs::create_dir_all(root.join("home/work")).unwrap();
    std::fs::create_dir_all(root.join("trap")).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("trap/escape")).unwrap();
    std::fs::create_dir_all(outside.join("sub")).unwrap();
    Guard { root, outside }
}

// ------------------------------------------------------------------- helpers

/// Is `p` open somewhere in this process as a writable fd? Reads `/proc/self/fd`,
/// so it reports what the kernel sees rather than what the code intends.
fn writable_fd_on(p: &Path) -> Option<String> {
    let target = std::fs::canonicalize(p).ok()?;
    for e in std::fs::read_dir("/proc/self/fd").ok()?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if let Ok(link) = std::fs::read_link(e.path()) {
            if link != target {
                continue;
            }
            let info = std::fs::read_to_string(format!("/proc/self/fdinfo/{name}")).ok()?;
            let flags_oct = info
                .lines()
                .find_map(|l| l.strip_prefix("flags:"))
                .map(str::trim)?;
            let flags = u32::from_str_radix(flags_oct, 8).unwrap_or(0);
            if flags & 0o3 != 0 {
                return Some(format!("fd {name} flags {flags_oct}"));
            }
        }
    }
    None
}

fn write_script_direct(path: &Path, body: &[u8]) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, body).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// The fix's shape: write a sibling temp file, then rename it into place, so the
/// path that gets `exec`'d is never open for writing at any point.
fn write_script_atomic(path: &Path, body: &[u8]) {
    use std::os::unix::fs::PermissionsExt;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, body).unwrap();
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::rename(&tmp, path).unwrap();
}

/// The sweep rule, parameterised by prefix so this file cannot disturb the real
/// fixture namespace while testing the rule itself. See the caveat at the top.
fn sweep(prefix: &str, stale_after: Duration) -> usize {
    let now = SystemTime::now();
    let mut removed = 0;
    let entries = match std::fs::read_dir(std::env::temp_dir()) {
        Ok(e) => e,
        Err(_) => return 0,
    };
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().starts_with(prefix) {
            continue;
        }
        let age = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|m| now.duration_since(m).ok());
        if matches!(age, Some(a) if a > stale_after) {
            if std::fs::remove_dir_all(entry.path()).is_ok() {
                removed += 1;
            }
        }
    }
    removed
}

fn age_file(p: &Path, age: Duration) {
    use std::ffi::CString;
    #[repr(C)]
    struct Timespec {
        tv_sec: i64,
        tv_nsec: i64,
    }
    extern "C" {
        fn utimensat(
            dirfd: i32,
            path: *const std::os::raw::c_char,
            times: *const Timespec,
            flags: i32,
        ) -> i32;
    }
    let c = CString::new(p.to_string_lossy().as_bytes()).unwrap();
    let secs = (SystemTime::now() - age)
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let times = [
        Timespec {
            tv_sec: secs,
            tv_nsec: 0,
        },
        Timespec {
            tv_sec: secs,
            tv_nsec: 0,
        },
    ];
    let rc = unsafe { utimensat(-100, c.as_ptr(), times.as_ptr(), 0) };
    assert_eq!(rc, 0, "utimensat on {} failed", p.display());
}

// ==========================================================================
// #59 — a panicking test must not lose its fixture
// ==========================================================================

/// The defect: `cleanup()` sits at the end of the body, so a panic skips it.
/// Asserted on the exact path this test built — not on a count of `/tmp`.
#[test]
fn old_shape_leaks_its_fixture_when_the_body_panics() {
    let result = std::panic::catch_unwind(|| {
        let (root, outside) = old_make_root("atrium-t7leak-old-");
        assert!(root.join("trap/escape").exists());
        // Hand the paths out before the panic so the assertion below can look at
        // this test's own fixture rather than guessing from a global count.
        LEAKED.with(|c| *c.borrow_mut() = Some((root.clone(), outside.clone())));
        panic!("simulating a failing assertion mid-test");
        #[allow(unreachable_code)]
        old_cleanup(&root, &outside);
    });
    assert!(result.is_err(), "the body must have panicked");
    let leaked = LEAKED.with(|c| c.borrow_mut().take());
    let (root, outside) = leaked.expect("the fixture paths were recorded before the panic");

    println!("  OLD  shape: root still present = {}", root.exists());
    println!("  OLD  shape: outside still present = {}", outside.exists());
    assert!(
        root.exists() && outside.exists(),
        "the old shape leaked nothing, so this failure path is not reproduced"
    );
    // Tidy only what this test made.
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&outside);
}

thread_local! {
    static LEAKED: std::cell::RefCell<Option<(PathBuf, PathBuf)>> =
        const { std::cell::RefCell::new(None) };
}

/// The fix: `Drop` runs while unwinding, so the tree goes away anyway.
#[test]
fn new_shape_keeps_nothing_when_the_body_panics() {
    thread_local! {
        static GONE: std::cell::RefCell<Option<PathBuf>> =
            const { std::cell::RefCell::new(None) };
    }
    let result = std::panic::catch_unwind(|| {
        let g = new_make_root("atrium-t7leak-new-");
        assert!(g.root.join("trap/escape").exists());
        GONE.with(|c| c.replace(Some(g.root.clone())));
        panic!("simulating a failing assertion mid-test");
    });
    assert!(result.is_err(), "the body must have panicked");
    let root = GONE.with(|c| c.take()).expect("path recorded");
    println!("  NEW  shape: root still present = {}", root.exists());
    assert!(
        !root.exists(),
        "the guard must remove the fixture while unwinding"
    );
}

// ==========================================================================
// #60 — fixtures from killed runs must be collected
// ==========================================================================

/// A killed process runs no `Drop`, so the leftovers have to be swept instead.
/// Shows both halves on one path: the old shape leaves it, the fix removes it.
#[test]
fn a_stale_corpse_survives_the_old_shape_and_is_swept_by_the_fix() {
    // Its own prefix, so the sweep under test cannot reach any other test's dirs.
    let prefix = format!("atrium-t7corpse-{}-", std::process::id());
    let corpse = std::env::temp_dir().join(tag(&prefix));
    std::fs::create_dir_all(corpse.join("trap")).unwrap();
    std::fs::write(corpse.join("trap/escape"), b"").unwrap();
    age_file(&corpse, Duration::from_secs(2 * 60 * 60));
    println!("  planted corpse: {} (aged 2h)", corpse.display());

    // The old shape has no sweep at all, so an unrelated run cannot clear it.
    let (old_root, old_outside) = old_make_root("atrium-t7corpse-old-");
    println!(
        "  OLD  shape after a run: corpse still present = {}",
        corpse.exists()
    );
    assert!(
        corpse.exists(),
        "the old shape has no sweep, so the corpse must survive it"
    );
    // Tidy what this test made: the old shape leaves its own tree behind too, and
    // a detector that litters is the thing it is testing for.
    let _ = std::fs::remove_dir_all(&old_root);
    let _ = std::fs::remove_dir_all(&old_outside);

    let removed = sweep(&prefix, Duration::from_secs(REAL_STALE_AFTER_SECS));
    println!(
        "  NEW  shape: swept {removed}, corpse still present = {}",
        corpse.exists()
    );
    assert!(
        !corpse.exists(),
        "the sweep must remove a fixture older than the threshold"
    );
}

/// The age threshold is what makes the sweep safe alongside a live run: a fresh
/// fixture belongs to a process that may still be using it.
#[test]
fn the_sweep_leaves_a_fresh_fixture_alone() {
    let prefix = format!("atrium-t7fresh-{}-", std::process::id());
    let fresh = std::env::temp_dir().join(tag(&prefix));
    std::fs::create_dir_all(&fresh).unwrap();

    let removed = sweep(&prefix, Duration::from_secs(REAL_STALE_AFTER_SECS));

    println!(
        "  swept {removed}; fresh fixture survived = {}",
        fresh.exists()
    );
    assert_eq!(removed, 0, "the sweep removed a fresh fixture");
    assert!(
        fresh.exists(),
        "the sweep removed a fresh fixture — it would break a concurrent run"
    );
    let _ = std::fs::remove_dir_all(&fresh);
}

/// The sweep that actually ships must use the real prefix and the one-hour
/// threshold this file tests against. Read from the source, because that is where
/// the rule lives — the runtime copy here is parameterised and cannot prove it.
#[test]
fn the_shipped_sweep_uses_the_real_prefix_and_threshold() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/shell_tests.rs"))
        .expect("shell_tests.rs must be readable");
    println!("  checking shell_tests.rs for the real prefix and threshold");
    assert!(
        src.contains("\"atrium-2b-test-\""),
        "shell_tests.rs no longer uses the real fixture prefix"
    );
    assert!(
        src.contains("Duration::from_secs(60 * 60)"),
        "shell_tests.rs no longer uses a 1-hour staleness threshold"
    );
    assert!(
        src.contains("sweep_stale_fixtures()"),
        "shell_tests.rs no longer calls the sweep"
    );
}

// ==========================================================================
// #62 — the exec'd path must never be open for writing
// ==========================================================================

/// The mechanism, observed directly. The old write shape holds a writable fd on
/// the path it is about to exec; the fix never does.
#[test]
fn the_old_write_shape_holds_a_write_fd_on_the_exec_path_and_the_fix_never_does() {
    let root = std::env::temp_dir().join(tag("atrium-t7fd-"));
    std::fs::create_dir_all(&root).unwrap();
    let script = root.join("argv.sh");

    // OLD: write straight to the final path and inspect /proc/self/fd while open.
    {
        use std::io::Write;
        let mut fh = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&script)
            .unwrap();
        fh.write_all(b"#!/bin/sh\necho hi\n").unwrap();
        fh.flush().unwrap();
        let found = writable_fd_on(&script);
        println!("  OLD  write: writable fd on the exec'd path = {found:?}");
        assert!(
            found.is_some(),
            "the old shape showed no writable fd, so #62's mechanism is not reproduced"
        );
    }

    // FIX: the write goes to a sibling temp name; the exec'd path stays clean.
    {
        use std::io::Write;
        let tmp = script.with_extension("tmp");
        let mut fh = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp)
            .unwrap();
        fh.write_all(b"#!/bin/sh\necho hi\n").unwrap();
        fh.flush().unwrap();
        let on_tmp = writable_fd_on(&tmp);
        let on_final = writable_fd_on(&script);
        println!(
            "  NEW  write: writable fd on the temp = {on_tmp:?}; on the exec'd path = {on_final:?}"
        );
        assert!(on_tmp.is_some(), "the temp file is the one being written");
        assert!(
            on_final.is_none(),
            "the exec'd path was open for writing — another thread's fork could carry it"
        );
    }

    write_script_atomic(&script, b"#!/bin/sh\necho hi\n");
    assert!(
        writable_fd_on(&script).is_none(),
        "the final path must be clean after the rename"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// Injection: with a write fd deliberately held on the target, the exec really
/// does fail ETXTBSY. Without this, "no ETXTBSY" elsewhere would prove nothing —
/// a detector that cannot see the failure cannot report its absence.
#[test]
fn inject_shows_etxtbsy_is_reachable_when_a_write_fd_is_held() {
    let root = std::env::temp_dir().join(tag("atrium-t7inject-"));
    std::fs::create_dir_all(&root).unwrap();
    let script = root.join("argv.sh");
    write_script_direct(&script, b"#!/bin/sh\nexit 0\n");

    let holder = std::fs::OpenOptions::new()
        .write(true)
        .open(&script)
        .unwrap();
    match std::process::Command::new(&script).output() {
        Err(e) => {
            println!("  INJECT: exec failed as expected: {e}");
            assert_eq!(e.raw_os_error(), Some(26), "expected ETXTBSY (os error 26)");
        }
        Ok(_) => panic!(
            "the exec SUCCEEDED while a write fd was held — this file cannot see \
             ETXTBSY, so its other results mean nothing"
        ),
    }
    drop(holder);
    let _ = std::fs::remove_dir_all(&root);
}

/// The fix must still produce a script that actually runs, with the right content
/// and mode — an atomic rename that broke executability would be a silent
/// regression in the other direction.
#[test]
fn the_atomic_write_still_produces_a_runnable_script() {
    let root = std::env::temp_dir().join(tag("atrium-t7atomic-"));
    std::fs::create_dir_all(&root).unwrap();
    let script = root.join("argv.sh");
    write_script_atomic(&script, b"#!/bin/sh\nprintf 'ran=%s\\n' \"$1\"\n");

    let out = std::process::Command::new(&script)
        .arg("hello")
        .output()
        .expect("the renamed script must be executable");
    let stdout = String::from_utf8_lossy(&out.stdout);
    println!("  atomic write: ran, stdout = {:?}", stdout.trim());
    assert!(
        stdout.contains("ran=hello"),
        "got {stdout:?}, expected the script's own output"
    );

    // A later write must replace it cleanly (the temp name is reused).
    write_script_atomic(&script, b"#!/bin/sh\necho second\n");
    let out2 = std::process::Command::new(&script).output().unwrap();
    assert!(
        String::from_utf8_lossy(&out2.stdout).contains("second"),
        "a second atomic write must replace the first"
    );

    let _ = std::fs::remove_dir_all(&root);
}
