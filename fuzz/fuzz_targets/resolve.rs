//! Fuzz target: the Atrium sandbox path resolver.
//!
//! One process builds a **fixed fixture tree** once (see `fixture()`), then every
//! input the fuzzer produces is handed to `atrium_resolver::resolve()` as the
//! virtual path. The oracle is checked on every input and panics when it is
//! violated, so libFuzzer records the input and the crash is reportable.
//!
//! ## The oracle
//!
//! Two things must hold for **every** input, and neither is allowed to depend on
//! what the resolver *said*:
//!
//! 1. **No panic.** The resolver is a pure reader: it canonicalises, stats and
//!    reads links, and nothing else. It is not supposed to abort on any input at
//!    all, so an unwind out of `resolve()` is a defect whether or not the input
//!    was "reasonable".
//! 2. **Every `Ok` result is really inside the root.** Not spelled inside it —
//!    *really* inside it. The escape this project has already been bitten by
//!    (`docs/STATUS.md`, 18 Sep 2026) returned a path whose spelling sat under
//!    the root and which canonicalised to a location outside it, so a string
//!    prefix test passed and the sandbox leaked. The check here is therefore
//!    done on the **canonical** location, independently of the resolver:
//!
//!    - if the returned path exists, canonicalise it and compare;
//!    - if it does not exist (Phase 1b accepts absent paths inside the root),
//!      canonicalise its longest existing prefix and append the remaining
//!      components, then compare.
//!
//!    A lexical prefix check is kept as well, plus a check that no `..` or `.`
//!    component survives into a returned path — the resolver's own walk is
//!    supposed to have consumed those, and one appearing in a result is exactly
//!    the smell that preceded the last escape.
//!
//! ## The fixture tree
//!
//! It mirrors the shapes the hands-on attack lists test (`hand-test-1b.sh`
//! sections C–E, M and Q, and `src/main.rs`'s `fixtures` mode), because those are
//! the shapes the resolver is claimed to handle: files, directories, links that
//! stay inside, links that leave, links whose target is dangling on both sides of
//! the boundary, a link loop, an out-and-back chain that leaves the root and
//! returns, and a link that lands on an ancestor of the root.
//!
//! It lives in a temp directory beside the root, so the "outside" the fixture
//! escapes into is real and reachable rather than a name that cannot exist.
//!
//! ## Panic messages are deliberately input-independent
//!
//! The workflow deduplicates issues by panic message, so the message names the
//! *rule* that broke and never the input that broke it. The input is printed on
//! its own `stderr` line before the panic and is saved by libFuzzer as the
//! artifact, so nothing is lost and one bug class stays one issue.

#![no_main]

use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use libfuzzer_sys::fuzz_target;

/// The throwaway environment. `root` and `outside` are canonical and are
/// siblings, so a link can genuinely leave the root and land next to it.
struct Fixture {
    root: PathBuf,
}

static FIXTURE: OnceLock<Fixture> = OnceLock::new();

fn fixture() -> &'static Fixture {
    FIXTURE.get_or_init(build_fixture)
}

fn build_fixture() -> Fixture {
    let pid = std::process::id();
    let tmp = std::env::temp_dir();
    let base = tmp.join(format!("atrium-fuzz-resolver-{}", pid));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).expect("fuzz fixture: could not create the temp base directory");

    // Canonicalise the base BEFORE anything is built under it, and build every
    // symlink target from the canonical spelling. A symlink is stored as a
    // string, so a target written through a non-canonical spelling (a symlinked
    // /tmp, say) would point somewhere the canonical root does not, and the
    // fixture would be testing the wrong thing on such a machine.
    let parent =
        fs::canonicalize(&base).expect("fuzz fixture: could not canonicalise the base dir");

    let root = parent.join("root");
    let outside = parent.join("outside");
    fs::create_dir_all(&root).expect("fuzz fixture: could not create the root");
    fs::create_dir_all(&outside).expect("fuzz fixture: could not create the outside directory");

    let r = |p: &str| root.join(p);
    let o = |p: &str| outside.join(p);

    // Files and directories inside the root.
    for d in [
        "home/documents/sub",
        "home/empty",
        "a/b/c",
        "good-dir",
        "caf\u{e9}",
        "\u{65e5}\u{672c}\u{8a9e}",
        ".hidden",
        "deep/deep/deep",
    ] {
        fs::create_dir_all(r(d)).expect("fuzz fixture: could not create a directory");
    }
    for f in [
        "home/documents/notes.txt",
        "home/documents/file.txt",
        "home/documents/sub/inner.txt",
        "a/file.txt",
        "afile",
        "good-dir/inner.txt",
        ".hidden/inner.txt",
        "deep/deep/deep/leaf.txt",
    ] {
        fs::write(r(f), b"x\n").expect("fuzz fixture: could not write a file");
    }

    // Real content outside the root, so an escape that reaches it is an escape
    // that could read something rather than merely name it.
    for f in ["secret.txt", "keep.txt"] {
        fs::write(o(f), b"outside\n").expect("fuzz fixture: could not write an outside file");
    }

    let link = |target: &Path, at: PathBuf| {
        // A pre-existing entry is not an error here: the fixture is built once.
        if fs::symlink_metadata(&at).is_err() {
            symlink(target, &at).expect("fuzz fixture: could not create a symlink");
        }
    };

    // Links that stay inside the root.
    link(&r("home/documents"), r("link-to-inside"));
    link(Path::new("home/documents"), r("rel-inside"));
    link(Path::new("home/empty"), r("rel-empty"));
    link(&r("home/documents/notes.txt"), r("link-to-file"));

    // Links that leave the root.
    link(Path::new("/etc"), r("link-to-etc"));
    link(Path::new("/home"), r("link-to-home"));
    link(Path::new("/"), r("link-to-root"));
    link(Path::new(".."), r("link-to-parent"));
    link(Path::new("/etc/hostname"), r("link-to-etc-file"));
    link(&outside, r("link-out"));
    link(Path::new("../outside"), r("link-rel-out"));
    // `../../outside` is the shape hand-test-1b §M.3 uses: two levels up from
    // the root, which on this fixture is a directory that does not exist.
    link(Path::new("../../outside"), r("link-rel-out-2"));
    link(Path::new("/proc/self/cwd"), r("link-to-proc-self"));

    // Dangling links, on both sides of the boundary. A dangling link is still a
    // link: its target decides, whether or not the target exists (ruling K.1a).
    link(&r("home/documents/absent.txt"), r("dangle-inside"));
    link(Path::new("notes-absent.txt"), r("dangle-rel"));
    link(&o("absent.txt"), r("dangle-out"));
    link(Path::new("/etc/absent.txt"), r("dangle-etc"));
    link(Path::new("../absent-sibling.txt"), r("dangle-sibling"));

    // A loop.
    link(Path::new("loop-b"), r("loop-a"));
    link(Path::new("loop-a"), r("loop-b"));
    link(Path::new("loop-self"), r("loop-self"));

    // A chain that ends outside, reached through two intermediate links.
    link(Path::new("/etc"), r("chain-end-out"));
    link(&r("chain-end-out"), r("chain-2"));
    link(&r("chain-2"), r("chain-1"));
    // A chain that ends inside.
    link(&r("home/documents"), r("chain-in-2"));
    link(&r("chain-in-2"), r("chain-in-1"));

    // Out and back. `<root>/q-out` leaves the root; `<outside>/relay` points back
    // in; so the chain through `q-back` leaves the root and returns to it, which
    // ruling 5 (18 Sep 2026) refuses even though it ends inside.
    link(&outside, r("q-out"));
    link(&root, o("relay"));
    link(&o("relay"), r("q-back"));
    // The same departure in the middle of a longer chain: inside, then out, then
    // back in.
    link(&r("q-mid-2"), r("q-mid"));
    link(&o("relay"), r("q-mid-2"));

    // A landing on an ancestor of the root, and on a directory beside it. Both
    // have left the environment even when the spelling walks straight back in.
    link(&parent, r("q-anc"));
    link(&outside, r("q-sib"));

    let root = fs::canonicalize(&root).expect("fuzz fixture: root did not canonicalise");
    Fixture { root }
}

/// Printable form of a raw input, so the log line is readable without a hex
/// dump. ASCII printables stay as they are; every other byte becomes `\xNN`.
fn escaped(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() + 2);
    s.push('"');
    for &b in bytes {
        match b {
            b'\\' => s.push_str("\\\\"),
            b'"' => s.push_str("\\\""),
            0x20..=0x7e => s.push(b as char),
            _ => s.push_str(&format!("\\x{:02x}", b)),
        }
    }
    s.push('"');
    s
}

/// Canonical form of `p`, whether or not the whole path exists: canonicalise the
/// longest existing prefix and append the rest. This is what `realpath -m` does,
/// and it is the only honest way to ask "where does this really point" about a
/// path Phase 1b is allowed to accept before it exists.
///
/// `None` means the prefix walk fell off the filesystem without finding anything
/// canonicalisable, which cannot happen for a path under a canonical root.
fn canonical_form(p: &Path) -> Option<PathBuf> {
    let mut probe = p.to_path_buf();
    let mut tail: Vec<OsString> = Vec::new();
    loop {
        if let Ok(canonical) = fs::canonicalize(&probe) {
            let mut joined = canonical;
            for name in tail.iter().rev() {
                joined.push(name);
            }
            return Some(joined);
        }
        match probe.file_name() {
            Some(name) => tail.push(name.to_os_string()),
            None => return None,
        }
        if !probe.pop() {
            return None;
        }
    }
}

fn oracle(input: &[u8]) {
    let f = fixture();
    // The resolver takes `&str`. `from_utf8_lossy` keeps every byte that is
    // valid UTF-8 as it is — which includes NUL and every ASCII path shape — and
    // replaces invalid sequences, so the fuzzer's byte soup always reaches the
    // resolver instead of being skipped.
    let virtual_path = String::from_utf8_lossy(input);

    let resolved = atrium_resolver::resolve(&f.root, &virtual_path);
    let resolved = match resolved {
        Ok(p) => p,
        // A rejection is a valid outcome. What a rejection *says* is the
        // attack lists' business, not this oracle's: it is checked here only for
        // the one property that matters to the sandbox, which is that a refusal
        // does not claim to have found something outside the root.
        Err(_) => return,
    };

    // (a) Nothing that was refused as an escape may come back as an accepted
    // path containing a relative step: the walk is supposed to have consumed
    // every `..` and `.` before returning.
    for component in resolved.components() {
        if matches!(component, Component::ParentDir | Component::CurDir) {
            eprintln!("FUZZ ORACLE INPUT: {}", escaped(input));
            panic!("resolver oracle: an Ok result still contains a relative component");
        }
    }

    // (b) Lexical containment. Cheap, and it catches a result assembled from
    // somewhere other than the canonical root.
    if !resolved.starts_with(&f.root) {
        eprintln!("FUZZ ORACLE INPUT: {}", escaped(input));
        eprintln!(
            "FUZZ ORACLE RESULT: {}",
            escaped(resolved.as_os_str().as_encoded_bytes())
        );
        panic!("resolver oracle: an Ok result is not inside the canonical root as spelled");
    }

    // (c) Real containment — the check the last escape walked through. This is
    // computed by the oracle, not read from the resolver.
    match canonical_form(&resolved) {
        Some(canonical) if canonical.starts_with(&f.root) => {}
        Some(canonical) => {
            eprintln!("FUZZ ORACLE INPUT: {}", escaped(input));
            eprintln!(
                "FUZZ ORACLE RESULT: {}",
                escaped(resolved.as_os_str().as_encoded_bytes())
            );
            eprintln!(
                "FUZZ ORACLE CANONICAL: {}",
                escaped(canonical.as_os_str().as_encoded_bytes())
            );
            eprintln!(
                "FUZZ ORACLE ROOT: {}",
                escaped(f.root.as_os_str().as_encoded_bytes())
            );
            panic!("resolver oracle: an Ok result canonicalises outside the canonical root");
        }
        None => {
            eprintln!("FUZZ ORACLE INPUT: {}", escaped(input));
            panic!("resolver oracle: an Ok result could not be canonicalised at all");
        }
    }
}

fuzz_target!(|data: &[u8]| {
    oracle(data);
});
