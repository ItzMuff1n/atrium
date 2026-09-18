//! Property tests for the sandbox path resolver (Phase 1 + 1b).
//!
//! These are NOT more attack-list cases. The attack lists check inputs somebody
//! thought of. This file checks rules that must hold for *any* input, by
//! generating random ones per run and shrinking any failure to a minimal case.
//!
//! The contract under test is `atrium_resolver::resolve(root, virtual_path)`.
//! For every generated case a fresh temp tree is built (directories, files and
//! symlinks — chains, loops, dangling targets, pointing both inside and outside
//! the root), then five properties are asserted:
//!
//!   1. it never panics;
//!   2. every `Ok` result, after canonicalising whatever part of it exists,
//!      lies inside the canonical root;
//!   3. the verdict is deterministic: the same input resolved twice agrees;
//!   4. it never changes the filesystem: a snapshot of the tree (paths, types,
//!      symlink targets) is identical before and after;
//!   5. relative, empty and NUL-containing inputs are always `Err`.
//!
//! Properties 1-4 need the same built tree, so they share one test; property 5
//! needs no tree and is its own test.
//!
//! Failures use plain `assert!`, which panics — proptest catches the panic and
//! shrinks it to a minimal case, which is the behaviour we want.
//!
//! Nothing here edits the resolver, and nothing here writes outside a temp dir.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use atrium_resolver::resolve;
use proptest::prelude::*;

/// A component of 298 bytes. Over NAME_MAX (255), so the resolver's own length
/// rule must reject it whether or not anything by that name exists.
const LONG_NAME: &str =
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\
aaaaaaaaaaaaaaaaaaaaaaaaa";

/// How many cases each property test runs. Overridable with PROP_CASES so the
/// number can be tuned without editing the file.
const DEFAULT_CASES: u32 = 256;

fn cases() -> u32 {
    std::env::var("PROP_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_CASES)
}

/// Names the OS will accept as a single component: non-empty, no separator, no
/// NUL, at most 255 bytes, and not a bare `.` or `..`. Anything else cannot be
/// created and the builder skips it — such names still reach the resolver
/// inside virtual paths, they just cannot exist on disk.
fn creatable(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\0')
        && name.len() <= 255
        && name != "."
        && name != ".."
}

/// Where a generated symlink points.
#[derive(Debug, Clone)]
enum LinkTarget {
    /// A file that exists in the root, reached relatively.
    InsideFile,
    /// A directory that exists in the root.
    InsideDir,
    /// A file inside a subdirectory of the root.
    SubFile,
    /// A real file outside the root, reached by `../`.
    OutsideRelative,
    /// A real file outside the root, reached absolutely.
    OutsideAbsolute,
    /// `/etc/passwd` — absolute, outside the root, and definitely exists.
    EtcPasswd,
    /// A target that does not exist: a dangling symlink.
    Dangling,
    /// Another symlink in the same tree, by index. An index equal to its own is
    /// a self-loop; forming a cycle with an earlier link is a loop; otherwise
    /// it is a chain.
    LinkRef(usize),
}

/// A description of the tree to build. Kept small and flat so proptest can
/// shrink it meaningfully.
#[derive(Debug, Clone)]
struct TreeSpec {
    /// Subdirectories of the root.
    dirs: Vec<String>,
    /// Files directly in the root.
    root_files: Vec<String>,
    /// Files inside the first subdirectory (created only if that dir exists).
    sub_files: Vec<String>,
    /// Symlinks in the root: (name, target).
    links: Vec<(String, LinkTarget)>,
}

impl TreeSpec {
    /// Every name that could appear in a virtual path for this tree.
    fn all_names(&self) -> Vec<String> {
        let mut v = Vec::new();
        v.extend(self.dirs.iter().cloned());
        v.extend(self.root_files.iter().cloned());
        v.extend(self.sub_files.iter().cloned());
        v.extend(self.links.iter().map(|(n, _)| n.clone()));
        v.sort();
        v.dedup();
        v
    }
}

fn arb_name() -> impl Strategy<Value = String> {
    prop_oneof![
        4 => "[a-z]{1,8}",
        3 => "[a-zA-Z0-9_.-]{1,10}",
        2 => proptest::sample::select(vec![
            "café".to_string(),
            "日本語".to_string(),
            "Ω".to_string(),
            "🚀".to_string(),
            "naïve".to_string(),
            "..x".to_string(),
            ".hidden".to_string(),
            "a b".to_string(),
        ]),
        // Deliberately long: around and over the 255-byte component limit.
        1 => "[a-z]{250,260}",
    ]
}

fn arb_tree() -> impl Strategy<Value = TreeSpec> {
    let dirs = prop::collection::vec(arb_name(), 0..3);
    let root_files = prop::collection::vec(arb_name(), 0..3);
    let sub_files = prop::collection::vec(arb_name(), 0..3);
    let links = prop::collection::vec(
        (arb_name(), 0usize..8).prop_map(|(n, k)| {
            let target = match k {
                0 => LinkTarget::InsideFile,
                1 => LinkTarget::InsideDir,
                2 => LinkTarget::SubFile,
                3 => LinkTarget::OutsideRelative,
                4 => LinkTarget::OutsideAbsolute,
                5 => LinkTarget::EtcPasswd,
                6 => LinkTarget::Dangling,
                _ => LinkTarget::LinkRef(usize::MAX), // resolved to a real index below
            };
            (n, target)
        }),
        0..4,
    );
    (dirs, root_files, sub_files, links).prop_map(|(dirs, root_files, sub_files, mut links)| {
        // Replace the sentinel with a real index so chains, loops and
        // self-loops all appear.
        let n = links.len();
        for (i, (_, t)) in links.iter_mut().enumerate() {
            if let LinkTarget::LinkRef(j) = t {
                if *j == usize::MAX {
                    let pick = if n == 0 { i } else { (i * 3 + 1) % n };
                    *t = LinkTarget::LinkRef(pick);
                }
            }
        }
        TreeSpec {
            dirs,
            root_files,
            sub_files,
            links,
        }
    })
}

/// A batch of generated virtual paths, packed into one string separated by
/// newlines so the strategy value stays a single `String`. Absolute by
/// construction, with `..`, `.`, `//`, long components, unicode and names that
/// match existing entries.
fn arb_batch(names: Vec<String>) -> impl Strategy<Value = String> {
    // `sample::select` panics on an empty collection, and a tree may legitimately
    // have no entries at all. Fall back to fixed names so the empty tree still
    // gets paths drawn against plausible spellings.
    let names = if names.is_empty() {
        vec![
            "etc".to_string(),
            "passwd".to_string(),
            "a".to_string(),
            "日本語".to_string(),
        ]
    } else {
        names
    };
    let piece = prop_oneof![
        3 => proptest::sample::select(names),
        2 => Just("..".to_string()),
        2 => Just(".".to_string()),
        1 => Just(String::new()), // an empty piece, which yields `//`
        1 => proptest::sample::select(vec![
            LONG_NAME.to_string(),
            "日本語".to_string(),
            "🚀".to_string(),
            "café".to_string(),
            "Ω".to_string(),
            "etc".to_string(),
            "passwd".to_string(),
        ]),
    ];
    prop::collection::vec(prop::collection::vec(piece, 0..6), 1..4).prop_map(|path_piece_lists| {
        let mut out: Vec<String> = Vec::new();
        for pieces in path_piece_lists {
            let mut s = String::from("/");
            for (i, p) in pieces.iter().enumerate() {
                if i > 0 {
                    s.push('/');
                    // Sometimes emit a repeated separator: `a//b`.
                    if i % 3 == 2 {
                        s.push('/');
                    }
                }
                s.push_str(p);
            }
            out.push(s);
        }
        out.join("\n")
    })
}

fn split_paths(packed: &str) -> Vec<String> {
    packed.split('\n').map(|s| s.to_string()).collect()
}

// ---------------------------------------------------------------------------
// temp-dir plumbing (std only — the resolver has no test helper for this)
// ---------------------------------------------------------------------------

static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct TempBase(PathBuf);

impl TempBase {
    fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!("atrium-prop-{}-{}", std::process::id(), n));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).expect("create temp base");
        TempBase(p)
    }
    fn root(&self) -> PathBuf {
        self.0.join("root")
    }
    fn outside(&self) -> PathBuf {
        self.0.join("outside")
    }
}

impl Drop for TempBase {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Build the described tree, returning the root path.
fn build_tree(base: &TempBase, spec: &TreeSpec) -> PathBuf {
    let root = base.root();
    let outside = base.outside();
    fs::create_dir_all(&root).expect("create root");
    fs::create_dir_all(&outside).expect("create outside");
    fs::write(outside.join("secret.txt"), b"outside-secret").expect("write outside file");

    // Directories.
    let mut made_dirs: Vec<String> = Vec::new();
    for d in &spec.dirs {
        if !creatable(d) {
            continue;
        }
        if fs::create_dir(root.join(d)).is_ok() {
            made_dirs.push(d.clone());
        }
    }

    // Files in the root.
    let mut made_root_files: Vec<String> = Vec::new();
    for f in &spec.root_files {
        if !creatable(f) {
            continue;
        }
        if fs::write(root.join(f), b"x").is_ok() {
            made_root_files.push(f.clone());
        }
    }

    // Files inside the first subdirectory, when there is one.
    let mut made_sub_files: Vec<String> = Vec::new();
    if let Some(first) = made_dirs.first() {
        for f in &spec.sub_files {
            if !creatable(f) {
                continue;
            }
            if fs::write(root.join(first).join(f), b"y").is_ok() {
                made_sub_files.push(f.clone());
            }
        }
    }

    // Symlinks, in order, so a later link can point at an earlier one. A link
    // whose target cannot be spelled is skipped; its slot is kept as empty so
    // the indices stay aligned with the spec.
    let mut made_links: Vec<String> = Vec::new();
    for (i, (name, target)) in spec.links.iter().enumerate() {
        if !creatable(name) {
            made_links.push(String::new());
            continue;
        }
        let dest = match target {
            LinkTarget::InsideFile => made_root_files.first().map(|f| format!("./{}", f)),
            LinkTarget::InsideDir => made_dirs.first().map(|d| format!("./{}", d)),
            LinkTarget::SubFile => made_dirs
                .first()
                .and_then(|d| made_sub_files.first().map(|f| format!("./{}/{}", d, f))),
            LinkTarget::OutsideRelative => Some("../outside/secret.txt".to_string()),
            LinkTarget::OutsideAbsolute => {
                Some(outside.join("secret.txt").to_string_lossy().into_owned())
            }
            LinkTarget::EtcPasswd => Some("/etc/passwd".to_string()),
            LinkTarget::Dangling => Some(format!("./does-not-exist-{}", i)),
            LinkTarget::LinkRef(j) => {
                if *j == i {
                    // Self-loop: point at our own name.
                    Some(format!("./{}", name))
                } else {
                    made_links
                        .get(*j)
                        .filter(|s| !s.is_empty())
                        .map(|n| format!("./{}", n))
                }
            }
        };
        match dest {
            Some(d) if std::os::unix::fs::symlink(&d, root.join(name)).is_ok() => {
                made_links.push(name.clone());
            }
            _ => made_links.push(String::new()),
        }
    }

    root
}

/// Every path under `dir` with its type and, for symlinks, its target. Sorted,
/// so two snapshots compare exactly. Symlinks are never followed, so a loop
/// cannot make this recurse forever.
fn snapshot(dir: &Path) -> Vec<(String, String)> {
    fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, String)>) {
        let mut entries: Vec<_> = match fs::read_dir(dir) {
            Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
            Err(_) => return,
        };
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let p = e.path();
            let rel = p
                .strip_prefix(base)
                .unwrap_or(&p)
                .to_string_lossy()
                .into_owned();
            let md = match fs::symlink_metadata(&p) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let ft = md.file_type();
            if ft.is_symlink() {
                let t = fs::read_link(&p)
                    .map(|t| t.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| "<unreadable>".to_string());
                out.push((rel, format!("symlink -> {}", t)));
            } else if ft.is_dir() {
                out.push((rel, "dir".to_string()));
                walk(&p, base, out);
            } else {
                out.push((rel, format!("file {} bytes", md.len())));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out
}

/// The deepest ancestor of `p` that exists, canonicalised. This is
/// "canonicalise what exists": Phase 1b accepts a path that does not exist yet,
/// so the part that does exist is the part that can be checked.
fn deepest_existing_canonical(p: &Path) -> Option<(PathBuf, PathBuf)> {
    let mut cur = p.to_path_buf();
    loop {
        if let Ok(c) = fs::canonicalize(&cur) {
            return Some((cur.clone(), c));
        }
        if !cur.pop() {
            return None;
        }
    }
}

/// Run `f`, returning `Err(message)` if it panics instead of propagating. The
/// panic hook is silenced and restored so a caught panic does not spam output.
fn no_panic<F: FnOnce() -> R, R>(f: F) -> Result<R, String> {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    std::panic::set_hook(prev);
    out.map_err(|e| {
        if let Some(s) = e.downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = e.downcast_ref::<String>() {
            s.clone()
        } else {
            "<non-string panic>".to_string()
        }
    })
}

// ---------------------------------------------------------------------------
// Properties 1-4: one tree per case, every invariant checked on it
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(cases()))]

    /// Properties 1, 2, 3 and 4 — they share the built tree, so building it once
    /// per case is what keeps the file fast.
    #[test]
    fn prop_resolver_invariants(case in arb_tree().prop_flat_map(|s| {
        let names = s.all_names();
        (Just(s), arb_batch(names))
    })) {
        let (spec, packed) = case;
        let base = TempBase::new();
        let root = build_tree(&base, &spec);
        let paths = split_paths(&packed);
        let canon_root = fs::canonicalize(&root).expect("canonicalise root");

        let before = snapshot(&root);

        let mut first_verdicts: Vec<Result<PathBuf, String>> = Vec::new();
        let mut second_verdicts: Vec<Result<PathBuf, String>> = Vec::new();

        for p in &paths {
            // --- property 1: never panics -------------------------------
            let first = match no_panic(|| resolve(&root, p)) {
                Ok(r) => r,
                Err(msg) => panic!(
                    "PROPERTY 1 (never panics) VIOLATED\n  path: {:?}\n  panic: {}\n  tree: {:#?}",
                    p, msg, spec
                ),
            };
            let second = match no_panic(|| resolve(&root, p)) {
                Ok(r) => r,
                Err(msg) => panic!(
                    "PROPERTY 1 (never panics) VIOLATED on the second call\n  path: {:?}\n  panic: {}\n  tree: {:#?}",
                    p, msg, spec
                ),
            };

            // --- property 2: an Ok result lies inside the root ----------
            if let Ok(ref real) = first {
                if let Some((existing_part, canon)) = deepest_existing_canonical(real) {
                    assert!(
                        canon.starts_with(&canon_root),
                        concat!(
                            "PROPERTY 2 (an Ok result lies inside the root) VIOLATED\n",
                            "  virtual path:   {:?}\n",
                            "  returned:       {}\n",
                            "  existing part:  {}\n",
                            "  canonicalised:  {}\n",
                            "  canonical root: {}\n",
                            "  tree: {:#?}"
                        ),
                        p,
                        real.display(),
                        existing_part.display(),
                        canon.display(),
                        canon_root.display(),
                        spec
                    );
                }
            }

            first_verdicts.push(first.map_err(|e| format!("{:?}", e)));
            second_verdicts.push(second.map_err(|e| format!("{:?}", e)));
        }

        // --- property 3: the verdict is deterministic -------------------
        assert!(
            first_verdicts == second_verdicts,
            "PROPERTY 3 (deterministic) VIOLATED\n  first:  {:?}\n  second: {:?}\n  paths: {:?}\n  tree: {:#?}",
            first_verdicts,
            second_verdicts,
            paths,
            spec
        );

        // --- property 4: the filesystem is untouched --------------------
        let after = snapshot(&root);
        assert!(
            before == after,
            "PROPERTY 4 (the filesystem is unchanged) VIOLATED\n  before: {:?}\n  after: {:?}\n  paths: {:?}\n  tree: {:#?}",
            before,
            after,
            paths,
            spec
        );
    }

    /// Property 5: relative, empty and NUL-containing inputs are always `Err`.
    /// No tree is needed — these are rejected before any filesystem call.
    #[test]
    fn prop_lexical_inputs_rejected(
        junk in prop_oneof![
            3 => "[a-zA-Z0-9_./-]{1,12}",
            2 => proptest::sample::select(vec![
                "relative/path".to_string(),
                "a".to_string(),
                "./x".to_string(),
                "../x".to_string(),
                "..".to_string(),
                ".".to_string(),
            ]),
        ],
        nul_at in 0usize..6,
    ) {
        let base = TempBase::new();
        let root = base.root();
        fs::create_dir_all(&root).expect("create root");

        // Relative, non-empty and NUL-free: must be Err.
        if !junk.is_empty() && !junk.contains('\0') && !Path::new(&junk).is_absolute() {
            let r = resolve(&root, &junk);
            assert!(
                r.is_err(),
                "PROPERTY 5 (a relative input is Err) VIOLATED\n  input: {:?}\n  got: {:?}",
                junk,
                r
            );
        }

        // Empty: always Err.
        let r = resolve(&root, "");
        assert!(
            r.is_err(),
            "PROPERTY 5 (an empty input is Err) VIOLATED\n  got: {:?}",
            r
        );

        // NUL anywhere in an otherwise absolute path: always Err.
        let mut n = junk.replace('\0', "");
        let pos = nul_at.min(n.len());
        n.insert(pos, '\0');
        let nul_path = format!("/{}", n);
        let r = resolve(&root, &nul_path);
        assert!(
            r.is_err(),
            "PROPERTY 5 (a NUL-containing input is Err) VIOLATED\n  input: {:?}\n  got: {:?}",
            nul_path,
            r
        );
    }
}
