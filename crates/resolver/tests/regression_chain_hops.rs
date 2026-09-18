//! Ruling (18 Sep 2026): a symlink chain that leaves the root at ANY hop is
//! refused, even when it ends inside the root.
//!
//! The reason is in `docs/DECISIONS.md`: containment is checked at every
//! intermediate step, and an out-and-back chain breaks that rule — the walk
//! passes through a location that is outside the environment, on the way to
//! one that is inside. Strictness wins.
//!
//! Until this ruling the resolver followed the whole chain at once with
//! `canonicalize` and checked only where it landed, so a chain that went
//! `<root> -> <outside> -> <root>` was ACCEPTED. The shapes below are the
//! cases that change, plus the controls that must NOT change: a chain that
//! stays inside is still accepted, and a single escaping link is still
//! refused.
//!
//! Nothing here edits an existing test file. The one existing assertion that
//! this ruling flips lives in `blind_textual.rs` (`blind_22`, case 22 of the
//! blind list) and is flipped there, with the flip listed in the PR.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use atrium_resolver::{resolve, ResolveError};

struct Fixture {
    dir: PathBuf,
    root: PathBuf,
    outside: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Fixture {
        let dir =
            std::env::temp_dir().join(format!("atrium-chain-{}-{}", name, std::process::id()));
        if dir.exists() {
            fs::remove_dir_all(&dir).unwrap();
        }
        let root = dir.join("root");
        let outside = dir.join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.txt"), b"OUTSIDE").unwrap();

        Fixture { dir, root, outside }
    }

    /// A link directly under the root, absolutely targeted.
    fn link(&self, name: &str, target: &Path) {
        symlink(target, self.root.join(name)).unwrap();
    }
    /// A link at an arbitrary path under the root (parents created).
    fn link_at(&self, rel: &str, target: &Path) {
        let p = self.root.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        symlink(target, p).unwrap();
    }
    fn dir(&self, name: &str) {
        fs::create_dir_all(self.root.join(name)).unwrap();
    }
    fn file(&self, name: &str, body: &[u8]) {
        let p = self.root.join(name);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, body).unwrap();
    }

    fn canon_root(&self) -> PathBuf {
        fs::canonicalize(&self.root).unwrap()
    }

    /// The verdict must be a refusal, and the reason must name the escape —
    /// not mere absence, and not a loop where the ruling is about containment.
    fn refused(&self, path: &str) {
        match resolve(&self.root, path) {
            Err(e) => {
                let reason = e.to_string();
                assert!(
                    !reason.contains("does not exist inside the environment root"),
                    "`{}` refused by citing ABSENCE, not the chain hop: {}",
                    path,
                    reason
                );
                assert!(
                    !matches!(
                        e,
                        ResolveError::EmptyPath | ResolveError::RelativePath { .. }
                    ),
                    "`{}` refused by a lexical rule, not the chain hop: {}",
                    path,
                    reason
                );
            }
            Ok(p) => {
                let canon = fs::canonicalize(&p).unwrap_or_else(|_| p.clone());
                panic!(
                    "`{}` ACCEPTED as {} -> really {} (root {}) -- the chain leaves the root",
                    path,
                    p.display(),
                    canon.display(),
                    self.canon_root().display()
                );
            }
        }
    }

    /// Accept is the required verdict, and the result must really be inside.
    fn accepted_inside(&self, path: &str) {
        match resolve(&self.root, path) {
            Ok(p) => {
                let canon_root = self.canon_root();
                let canon = fs::canonicalize(&p).unwrap_or_else(|_| p.clone());
                assert!(
                    canon.starts_with(&canon_root),
                    "`{}` ACCEPTED as {} but really {} is outside {}",
                    path,
                    p.display(),
                    canon.display(),
                    canon_root.display()
                );
            }
            Err(e) => panic!("`{}` should be ACCEPTED, was refused: {}", path, e),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// The relay every out-and-back case needs: a link **outside** the root that
//    points back into it. This is what makes a chain end inside even though it
//    left first, and it is the whole reason the ruling exists.
fn relay(f: &Fixture) {
    symlink(&f.root, f.outside.join("relay")).unwrap();
}

// =========================================================================
// 1. out and back: <root>/back -> <outside>/relay -> <root>
// =========================================================================

#[test]
fn out_and_back_chain_is_refused() {
    let f = Fixture::new("oab");
    relay(&f);
    f.link("back", &f.outside.join("relay"));
    f.file("ok.txt", b"ok\n");

    // The chain leaves the root at its first hop, whoever it ends inside.
    f.refused("/back");
    f.refused("/back/ok.txt");
    // And under an absent prefix, so the `..` cannot hide it.
    f.refused("/gone/../back");
    f.refused("/gone/../back/ok.txt");
}

// =========================================================================
// 2. a hop out in the MIDDLE of a longer chain
//    <root>/c_start -> <root>/d1/hopout -> <outside>/relay2 -> <root>/d1/d3
// =========================================================================

#[test]
fn hop_out_in_the_middle_of_a_longer_chain_is_refused() {
    let f = Fixture::new("middle");
    f.dir("d1/d3");
    f.file("d1/d3/leaf.txt", b"leaf\n");
    symlink(f.root.join("d1/d3"), f.outside.join("relay2")).unwrap();
    f.link_at("d1/hopout", &f.outside.join("relay2"));
    f.link("c_start", &f.root.join("d1/hopout"));

    // The first hop is inside; the second leaves; the third lands inside.
    f.refused("/c_start");
    f.refused("/c_start/leaf.txt");
    // The same hop, reached as an ordinary component rather than as a link.
    f.refused("/d1/hopout");
    f.refused("/d1/hopout/leaf.txt");
}

// =========================================================================
// 3. a chain entirely inside — must still ACCEPT
// =========================================================================

#[test]
fn chain_entirely_inside_is_accepted() {
    let f = Fixture::new("inside");
    f.dir("real");
    f.file("real/notes.txt", b"notes\n");
    // real <- d <- c <- b <- a, every hop under the root.
    f.link("d", &f.root.join("real"));
    f.link("c", &f.root.join("d"));
    f.link("b", &f.root.join("c"));
    f.link("a", &f.root.join("b"));

    f.accepted_inside("/a");
    f.accepted_inside("/a/notes.txt");
    // And after an absent prefix, so the chain is reached via a `..`.
    f.accepted_inside("/gone/../a/notes.txt");

    // A relative target that stays inside is inside too, including one that
    // walks up to the root first.
    f.link("reldir", Path::new("../root/real"));
    f.accepted_inside("/reldir/notes.txt");
    // A link to the root itself: its target location IS the root, inside.
    f.dir("deep");
    f.link_at("deep/to_root", Path::new(".."));
    f.accepted_inside("/deep/to_root");
    f.accepted_inside("/deep/to_root/real/notes.txt");
}

// =========================================================================
// 4. a chain whose outside hop is DANGLING
//    <root>/dang1 -> <root>/dang2 -> <outside>/absent
// =========================================================================

#[test]
fn chain_with_a_dangling_outside_hop_is_refused() {
    let f = Fixture::new("dangout");
    // dang2 exists as a link but its target is outside and absent.
    f.link("dang2", &f.outside.join("absent"));
    f.link("dang1", &f.root.join("dang2"));

    f.refused("/dang1");
    f.refused("/dang1/anything");

    // A dangling chain whose every hop lands inside must still be ACCEPTED —
    // absence inside the root is exactly what Phase 1b accepts.
    f.link("dang_in", &f.root.join("nowhere/ghost"));
    f.accepted_inside("/dang_in");
    f.accepted_inside("/dang_in/more");
    // And a mid-chain dangling link that lands inside, reached through a `..`.
    f.link("mid2", &f.root.join("absent_mid"));
    f.link("mid1", &f.root.join("mid2"));
    f.accepted_inside("/gone/../mid1/next/inner.txt");
}

// =========================================================================
// 5. a loop that passes outside
//    <root>/loopout1 -> <outside>/loopout2 -> <root>/loopout1
// =========================================================================

#[test]
fn loop_that_passes_outside_is_refused_not_hung() {
    let f = Fixture::new("loopout");
    symlink(f.root.join("loopout1"), f.outside.join("loopout2")).unwrap();
    f.link("loopout1", &f.outside.join("loopout2"));

    // Whichever rule is named (the hop out, or the loop), it must be refused
    // and must not hang.
    f.refused("/loopout1");
    f.refused("/loopout1/anything");
}

// =========================================================================
// 6. a hop that LANDS on an ancestor of the root
//    <root>/a -> <root>/.. (the root's parent) -> back down into <root>
// =========================================================================

#[test]
fn a_hop_that_lands_on_an_ancestor_of_the_root_is_refused() {
    let f = Fixture::new("ancestor");
    f.file("ok.txt", b"ok\n");
    // The target is the root's own parent: an ancestor of the environment
    // root, and inside the host. A link to it has left the root, even though
    // the path can spell its way straight back down into the root afterwards.
    let parent = f.canon_root().parent().unwrap().to_path_buf();
    f.link("a", &parent);

    // The hop lands on the ancestor: the chain left, so refusal is required
    // even though the chain would come back inside.
    f.refused("/a");
    // And the spelling that walks back into the root is refused with it. The
    // final location here is genuinely inside the root; the departure is what
    // decides.
    f.refused("/a/root/ok.txt");
    f.refused("/a/root");
}

// =========================================================================
// 7. an absolute target spelled THROUGH ancestors that lands inside the root,
//    with no hop leaving it — must still ACCEPT
// =========================================================================

#[test]
fn an_absolute_target_spelled_through_ancestors_and_landing_inside_is_accepted() {
    let f = Fixture::new("through");
    f.dir("real");
    f.file("real/notes.txt", b"notes\n");
    // The target is written from the host root, so its spelling passes through
    // `/`, the temporary directory, and the root's parent before it reaches the
    // root. None of that is a departure: it is how the address is written, and
    // this is the ordinary shape of an absolute target pointing inside.
    let target = f.canon_root().join("real");
    f.link("through_ancestors", &target);

    f.accepted_inside("/through_ancestors");
    f.accepted_inside("/through_ancestors/notes.txt");

    // The same shape reaching a link that is itself inside the root.
    f.link("d_in", &f.canon_root().join("real"));
    f.link("via_d", &f.canon_root().join("d_in"));
    f.accepted_inside("/via_d/notes.txt");

    // And the ancestor-spelled target that lands on the root itself.
    f.link("to_root", &f.canon_root());
    f.accepted_inside("/to_root");
    f.accepted_inside("/to_root/real/notes.txt");
}

// =========================================================================
// Controls: the ruling must not refuse these
// =========================================================================

#[test]
fn the_ruling_does_not_refuse_a_chain_that_stays_inside_or_a_plain_file() {
    let f = Fixture::new("controls");
    f.dir("real");
    f.file("real/notes.txt", b"notes\n");
    f.file("plain.txt", b"plain\n");
    f.link("dirlink", &f.root.join("real"));

    f.accepted_inside("/plain.txt");
    f.accepted_inside("/real/notes.txt");
    f.accepted_inside("/dirlink/notes.txt");
    f.accepted_inside("/newfile");
    f.accepted_inside("/newdir/newfile");

    // A single escaping link is still refused, for the reason it always was.
    f.link("esc", &f.outside);
    f.refused("/esc");
    f.refused("/esc/secret.txt");
    // A `..` target that climbs out of the root is still refused.
    f.link("link_to_parent", Path::new(".."));
    f.refused("/link_to_parent");
    f.refused("/link_to_parent/secret.txt");
}

/// Whichever verdict each shape gets, the resolver must never return a
/// location outside the root, and must never panic or hang. This is the
/// property the file exists to test.
#[test]
fn no_chain_shape_in_this_file_returns_a_location_outside_the_root() {
    let f = Fixture::new("sweep");
    relay(&f);
    f.dir("d1/d3");
    f.file("d1/d3/leaf.txt", b"leaf\n");
    f.file("ok.txt", b"ok\n");
    f.file("real/notes.txt", b"notes\n");
    symlink(f.root.join("d1/d3"), f.outside.join("relay2")).unwrap();
    f.link_at("d1/hopout", &f.outside.join("relay2"));
    f.link("c_start", &f.root.join("d1/hopout"));
    f.link("back", &f.outside.join("relay"));
    f.link("d", &f.root.join("real"));
    f.link("c", &f.root.join("d"));
    f.link("b", &f.root.join("c"));
    f.link("a", &f.root.join("b"));
    f.link("dang2", &f.outside.join("absent"));
    f.link("dang1", &f.root.join("dang2"));
    symlink(f.root.join("loopout1"), f.outside.join("loopout2")).unwrap();
    f.link("loopout1", &f.outside.join("loopout2"));
    f.link("esc", &f.outside);
    let parent = f.canon_root().parent().unwrap().to_path_buf();
    f.link("a_up", &parent);

    let canon_root = f.canon_root();
    for p in [
        "/back",
        "/back/ok.txt",
        "/gone/../back/ok.txt",
        "/c_start",
        "/c_start/leaf.txt",
        "/d1/hopout",
        "/d1/hopout/leaf.txt",
        "/a",
        "/a/notes.txt",
        "/gone/../a/notes.txt",
        "/dang1",
        "/dang1/anything",
        "/loopout1",
        "/loopout1/anything",
        "/esc",
        "/esc/secret.txt",
        "/ok.txt",
        "/real/notes.txt",
        "/newfile",
        "/a_up",
        "/a_up/root/ok.txt",
        "/a_up/root",
    ] {
        if let Ok(real) = resolve(&f.root, p) {
            let canon = fs::canonicalize(&real).unwrap_or_else(|_| real.clone());
            assert!(
                canon.starts_with(&canon_root),
                "`{}` returned {} which really is {} -- OUTSIDE the root {}",
                p,
                real.display(),
                canon.display(),
                canon_root.display()
            );
        }
    }
}
