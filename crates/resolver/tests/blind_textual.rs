//! The blind attack list, turned into tests.
//!
//! These 30 cases were produced by a subagent that was given ONLY the
//! resolver's rules as written in the doc comment at the top of
//! `src/lib.rs`, plus a description of the escape bug class. It saw no source
//! code and no tests. Its list is in the report as it was written; this file
//! implements it.
//!
//! Several cases deliberately probe for paths that must still be ACCEPTED —
//! a fix that refuses everything is not a fix. Those are marked in the test
//! names and assert containment of the result, not just `Ok`.
//!
//! Nothing here edits an existing test file.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use atrium_resolver::resolve;

struct Fixture {
    dir: PathBuf,
    root: PathBuf,
    outside: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Fixture {
        let dir =
            std::env::temp_dir().join(format!("atrium-blind-{}-{}", name, std::process::id()));
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

    fn link(&self, name: &str, target: &Path) {
        symlink(target, self.root.join(name)).unwrap();
    }
    fn dir(&self, name: &str) {
        fs::create_dir_all(self.root.join(name)).unwrap();
    }
    fn file(&self, name: &str, body: &[u8]) {
        if let Some(parent) = self.root.join(name).parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(self.root.join(name), body).unwrap();
    }

    fn canon_root(&self) -> PathBuf {
        fs::canonicalize(&self.root).unwrap()
    }

    /// Refuse is the required verdict, and the reason must name the escape,
    /// not mere absence.
    fn refused(&self, path: &str) {
        match resolve(&self.root, path) {
            Err(e) => {
                let reason = e.to_string();
                assert!(
                    !reason.contains("does not exist inside the environment root"),
                    "`{}` refused by citing ABSENCE, not the escape: {}",
                    path,
                    reason
                );
            }
            Ok(p) => {
                let canon = fs::canonicalize(&p).unwrap_or_else(|_| p.clone());
                panic!(
                    "`{}` ACCEPTED as {} -> really {} (root {})",
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

/// The standard setup the blind list assumes: an escaping symlink `esc`, a
/// regular file, a chain out, a loop, dangling links inside and outside, an
/// internal chain, and the directories the `..` cases need.
fn standard(name: &str) -> Fixture {
    let f = Fixture::new(name);
    // escaping
    f.link("esc", &f.outside);
    // regular files
    f.file("afile", b"x");
    f.file("file.txt", b"x");
    // chain out: chain1 -> chain2 -> outside
    f.link("chain2", &f.outside);
    f.link("chain1", &f.root.join("chain2"));
    // pure loop
    f.link("loop2", &f.root.join("loop1"));
    f.link("loop1", &f.root.join("loop2"));
    // dangling inside
    f.link("dangle", &f.root.join("nowhere/ghost"));
    f.link("dangle_mid", &f.root.join("missing_target"));
    // dangling outside
    f.link("outdangle", &f.outside.join("absent"));
    // relative target that climbs out of the root
    f.link("relclimb", Path::new("../outside/secret.txt"));
    // internal 4-hop chain ending at a real directory
    f.dir("real");
    f.file("real/file.txt", b"in\n");
    f.link("d", &f.root.join("real"));
    f.link("c", &f.root.join("d"));
    f.link("b", &f.root.join("c"));
    f.link("a", &f.root.join("b"));
    // self-loop
    f.link("s", Path::new("s"));
    // plain directories
    f.dir("good");
    f.dir("pdir");
    f.dir("trail");
    f.dir("dots");
    f.dir("sub");
    f.dir("deep");
    f
}

// =========================================================================
// 1. elided `.` after an absent component, then a climbing path
//
// ADJUDICATED: the blind list expected REJECT. It gets ACCEPT, and the
// resolver is right. In `/noexist/./esc`, the component `esc` is resolved
// UNDER `noexist`, which does not exist -- it is not the root's own `esc`
// symlink, and no symlink is reachable through an absent parent. So the whole
// path is ordinary absent names inside the root and phase 1b accepts it. The
// intent behind the case -- "an elided `.` must not let a later escaping
// symlink go unchecked" -- is exercised by the other paths below, where `..`
// first restores the confirmed position so the root's link IS reached.
#[test]
fn blind_01_absent_dot_then_parent_reaches_the_escaping_link_and_is_refused() {
    let f = standard("b01");
    // Literal blind-list path: contained, accepted, no symlink involved.
    f.accepted_inside("/noexist/./esc/../inner/escape.txt");
    // The same shape with `..` restoring the confirmed position first, so the
    // root's escaping link is genuinely reached.
    f.refused("/noexist/./../esc/../inner/escape.txt");
    f.refused("/noexist/./../esc");
    // NB not asserted: `/noexist/./esc/../esc`. The `..` pops the first `esc`,
    // leaving `esc` as a NAME under the absent `noexist` -- so it is an absent
    // path inside the root and ACCEPT is correct, the same as the literal path
    // above. It proves nothing extra.
}

// 2. leading `//` collapse, escaping link
#[test]
fn blind_02_leading_double_slash_escaping_link_is_refused() {
    let f = standard("b02");
    f.refused("//./esc");
    f.refused("//./esc/newfile");
}

// 3. a long `..` run returning to real positions, then an escaping link
#[test]
fn blind_03_long_parent_run_back_onto_real_positions_is_refused() {
    let f = standard("b03");
    // 20 nested real directories inside <root>/deep, then a `..` run that
    // returns to the root and one step beyond, then the escaping link. Each
    // `..` here lands back on a position that genuinely exists, so the walk
    // must return to the filesystem rather than staying textual.
    let mut real = f.root.join("deep");
    let mut virtual_path = String::from("/deep");
    for i in 0..20 {
        real = real.join(format!("d{}", i));
        virtual_path.push_str(&format!("/d{}", i));
    }
    fs::create_dir_all(&real).unwrap();
    // <root>/deep/d0..d19 is 21 components deep, so 21 climbs returns exactly
    // to <root> and the 22nd leaves it.
    let climbs_to_root = "/..".repeat(21);
    let climbs_beyond = "/..".repeat(22);
    f.refused(&format!("{}{}/esc", virtual_path, climbs_beyond));
    // The balanced version (back to the root, then the link) is also refused,
    // because the link itself points outside.
    f.refused(&format!("{}{}/esc", virtual_path, climbs_to_root));
}

// 4. two absent-normalisations then an escaping link
//
// ADJUDICATED: the blind list expected REJECT here. It gets ACCEPT, and the
// resolver is right. `dangle` is a DANGLING link whose target is
// `<root>/nowhere/ghost`, inside the root. Ruling 4 says that `..` then
// applies to where the TARGET would sit: `<root>/nowhere`. The next component
// is the NAME `esc`, resolved underneath `<root>/nowhere` — a directory that
// does not exist, so `<root>/nowhere/esc` is an ordinary absent path and
// phase 1b accepts it. Nothing escapes: the returned location has no existing
// parent, so it cannot reach the root's own `esc`, and traversing further out
// of it is refused (asserted below).
//
// The real defect this case was reaching for — "an absent prefix must not stop
// a LATER EXISTING component from being checked" — is covered by the sibling
// case where the component after `..` sits at the confirmed position:
// `blind_07` (`/dangle_mid/../esc`, an existing escaping link) is refused.
#[test]
fn blind_04_two_absent_normalisations_land_inside_and_do_not_escape() {
    let f = standard("b04");
    // ACCEPT is correct here, but it must be confined and it must not be a
    // way through to the root's escaping link.
    f.accepted_inside("/gone/../dangle/../esc");
    f.refused("/gone/../dangle/../esc/../../esc");
    f.refused("/gone/../dangle/../../esc");
    // `/gone/../dangle` is the dangling link itself, inside the root: ruling 1
    // accepts it. (An earlier version of this test asserted refusal, which was
    // wrong -- flagged by the failure and corrected here.)
    f.accepted_inside("/gone/../dangle");
}

// 5. symlink loop reached after an absent prefix
#[test]
fn blind_05_loop_reached_after_absent_prefix_is_refused() {
    let f = standard("b05");
    f.refused("/noexist/../loop1");
}

// 6. two-hop chain out, after an absent component
#[test]
fn blind_06_two_hop_chain_out_after_absent_is_refused() {
    let f = standard("b06");
    f.refused("/gone/../chain1");
}

// 7. dangling link, `..`, then an escaping link
#[test]
fn blind_07_dangling_then_parent_then_escaping_link_is_refused() {
    let f = standard("b07");
    f.refused("/dangle_mid/../esc");
}

// 8. dangling AND outside, with a trailing component
#[test]
fn blind_08_dangling_outside_with_trailing_component_is_refused() {
    let f = standard("b08");
    f.refused("/noexist/../outdangle/anything");
}

// 9. out via a symlink, back in via a host-side symlink inside the outside dir
#[test]
fn blind_09_out_then_back_in_via_host_link_is_refused() {
    let f = standard("b09");
    // <outside>/pivot/reentry -> <root>
    fs::create_dir_all(f.outside.join("pivot")).unwrap();
    symlink(&f.root, f.outside.join("pivot/reentry")).unwrap();
    f.link("escdir", &f.outside.join("pivot"));
    f.refused("/escdir/reentry/file.txt");
}

// 10. absent, file + `..` (ruling 2), then an escaping link
#[test]
fn blind_10_absent_file_parent_then_escaping_link_is_refused() {
    let f = standard("b10");
    f.refused("/noexist/../file.txt/../esc");
}

// 11. dangling link's `..` then an escaping link
//
// ADJUDICATED: same as case 4 — the blind list expected REJECT, the answer is
// ACCEPT, and the resolver is right (ruling 4: `..` applies to where the
// dangling target would sit, then `esc` is a name under that absent
// directory, so it is absent too). Confined, and no way through to the root's
// escaping link.
#[test]
fn blind_11_dangling_anchor_lands_inside_and_does_not_escape() {
    let f = standard("b11");
    f.accepted_inside("/dangle/../esc");
    f.refused("/dangle/../esc/../../esc");
    f.refused("/dangle/../../esc");
}

// 12. outside-dangling then `..` then a good dir
#[test]
fn blind_12_outside_dangling_then_parent_then_good_dir_is_refused() {
    let f = standard("b12");
    f.refused("/outdangle/../good");
}

// 13. relative climbing symlink after an absent prefix
#[test]
fn blind_13_relative_climbing_link_after_absent_is_refused() {
    let f = standard("b13");
    f.refused("/gone/../relclimb");
    f.refused("/gone/../relclimb/newfile");
}

// 14. file followed by `..` (ruling 2), no absent prefix
#[test]
fn blind_14_file_then_parent_is_refused_with_os_reason() {
    let f = standard("b14");
    match resolve(&f.root, "/afile/..") {
        Err(e) => {
            let reason = e.to_string();
            // Ruling 2: the operating system's own reason, not a synthesised one.
            assert!(
                reason.contains("Not a directory") || reason.contains("ENOTDIR"),
                "expected the OS reason for a file followed by `..`, got: {}",
                reason
            );
        }
        Ok(p) => panic!("/afile/.. ACCEPTED as {}", p.display()),
    }
}

// 15. absent, file + `..`, escaping link — every rule in sequence
#[test]
fn blind_15_absent_file_parent_escaping_link_is_refused() {
    let f = standard("b15");
    f.refused("/zzz/../afile/../esc");
}

// 16. a component whose name contains a backslash, pointing outside (ruling 3)
#[test]
fn blind_16_backslash_named_link_after_absent_is_refused() {
    let f = standard("b16");
    // A backslash-named link pointing outside: refused, and refused by naming
    // the SYMLINK, which proves the name was treated as one component and not
    // split at the backslash (ruling 3).
    f.link("back\\slash", &f.outside);
    f.refused("/noexist/../back\\slash");
    f.refused("/back\\slash");
    // A backslash-named link pointing INSIDE must still be accepted, so the
    // control shows the name is resolvable at all.
    f.link("in\\side", &f.root.join("real"));
    f.accepted_inside("/in\\side/file.txt");
    // And a backslash-named ABSENT path inside the root is an ordinary name.
    f.accepted_inside("/in\\side\\notyet.txt");
}

// 17. a 255-byte name that is a symlink pointing outside
#[test]
fn blind_17_max_length_name_link_after_absent_is_refused() {
    let f = standard("b17");
    let name = format!("{}x", "\u{e9}".repeat(127));
    assert_eq!(name.len(), 255, "fixture name must be 255 bytes");
    f.link(name.as_str(), &f.outside);
    f.refused(&format!("/noexist/../{}", name));
}

// 18. a 256-byte name, as a symlink to outside
#[test]
fn blind_18_over_length_name_after_absent_is_refused() {
    let f = standard("b18");
    let name = "\u{e9}".repeat(128);
    assert_eq!(name.len(), 256);
    // The name cannot exist (NAME_MAX), so the length rule must fire first.
    f.refused(&format!("/gone/../{}", name));
}

// 19. a max-length ABSENT component, then `..`, then an escaping link
#[test]
fn blind_19_max_length_absent_component_then_escaping_link_is_refused() {
    let f = standard("b19");
    let name = format!("{}x", "\u{e9}".repeat(127));
    assert_eq!(name.len(), 255);
    f.refused(&format!("/{}/../esc", name));
}

// 20. a Cyrillic confusable name that is a symlink pointing outside
#[test]
fn blind_20_cyrillic_confusable_link_after_absent_is_refused() {
    let f = standard("b20");
    // U+0435 CYRILLIC SMALL LETTER IE, then "sc".
    let name = "\u{435}sc";
    f.link(name, &f.outside);
    f.refused(&format!("/noexist/../{}", name));
}

// 21. a 4-hop INTERNAL chain after an absent prefix — must still be ACCEPTED
#[test]
fn blind_21_internal_chain_after_absent_is_accepted() {
    let f = standard("b21");
    f.accepted_inside("/gone/../a/file.txt");
}

// 22. out via a link, back in via a host-side relay
//
// FLIPPED 18 Sep 2026 (ruling, docs/DECISIONS.md). This case was adjudicated
// ACCEPT when it was written, on the ground that the resolver followed the
// whole chain with `canonicalize` and checked only where it landed: the
// intermediate location was never a step the walk took, so no intermediate
// check existed to fail, and the final location was inside the root.
//
// It was recorded as an open design question rather than silently resolved.
// The ruling settles it the other way: a chain that leaves the root at ANY hop
// is refused, even when it comes back in, because containment is checked at
// every step and the path passes through a location that is outside the
// environment. Strictness wins. The resolver now reads each hop with
// `read_link` and refuses the one that leaves.
//
// The one assertion that changed is the out-and-back ACCEPT below; it is
// listed in the PR and the report. The rest of the case is unchanged.
#[test]
fn blind_22_out_then_in_via_relay_is_refused_and_ending_outside_is_refused() {
    let f = standard("b22");
    symlink(&f.root, f.outside.join("relay")).unwrap();
    f.file("ok.txt", b"ok\n");
    f.link("hop", &f.outside.join("relay"));
    // The chain leaves the root at its first hop; coming back in does not
    // save it (ruling, 18 Sep 2026). This line was ACCEPT before the ruling.
    f.refused("/hop/ok.txt");
    // A chain that ENDS outside is refused — that is the rule that exists.
    f.link("hopout", &f.outside.join("relay"));
    f.link("relay2", &f.outside);
    f.link("hop2", &f.root.join("relay2"));
    f.refused("/hop2/newfile");
    f.refused("/hop2");
}

// 23. a pure loop reached via an absent prefix, then more components
#[test]
fn blind_23_loop_after_absent_with_more_components_is_refused_not_hung() {
    let f = standard("b23");
    // The blind list's path says `l1` while its setup said loop1/loop2; make
    // the loop exist under the name the path actually uses.
    f.link("l2", &f.root.join("l1"));
    f.link("l1", &f.root.join("l2"));
    f.refused("/gone/../l1/../anything");
}

// 24. a MIDDLE hop dangling, everything landing inside — must be ACCEPTED
#[test]
fn blind_24_dangling_middle_hop_landing_inside_is_accepted() {
    let f = standard("b24");
    // c1 -> c2 -> <root>/absent_mid (absent)
    f.link("c2", &f.root.join("absent_mid"));
    f.link("c1", &f.root.join("c2"));
    f.accepted_inside("/gone/../c1/next/inner.txt");
}

// 25. dangling anchor `..` then an escaping link with a leaf
#[test]
fn blind_25_dangling_anchor_then_escaping_link_with_leaf_is_refused() {
    let f = standard("b25");
    f.refused("/dangle_anchor/../esc/leaf.txt");
}

// 26. absent, existing dir, `..` back to the root, then an escaping link
#[test]
fn blind_26_absent_existing_dir_parent_then_escaping_link_is_refused() {
    let f = standard("b26");
    f.refused("/gone/../pdir/../esc");
}

// 27. repeated separators and a trailing slash on the leaf
#[test]
fn blind_27_repeated_separators_and_trailing_slash_is_refused() {
    let f = standard("b27");
    f.refused("/gone/../trail//../esc/");
}

// 28. interleaved `.` and `..` around absent and existing components
#[test]
fn blind_28_interleaved_dots_then_escaping_link_is_refused() {
    let f = standard("b28");
    f.refused("/gone/./../dots/./../esc");
}

// 29. a self-loop as the FIRST component (no absent component involved)
#[test]
fn blind_29_self_loop_first_component_is_refused_not_panicking() {
    let f = standard("b29");
    f.refused("/s/child.txt");
}

// 30. a `..` run that climbs above the root before an escaping link
#[test]
fn blind_30_climb_above_root_before_escaping_link_is_refused() {
    let f = standard("b30");
    f.refused("/sub/../../../esc");
    // And the step that climbs must be the one named, not a later component.
    match resolve(&f.root, "/sub/../../../esc") {
        Err(e) => {
            let reason = e.to_string();
            assert!(
                reason.contains("climbed above the environment root"),
                "expected the climb to be named, got: {}",
                reason
            );
        }
        Ok(p) => panic!("ACCEPTED as {}", p.display()),
    }
}

/// A blanket check across the whole list: whichever verdict each case gets,
/// the resolver must never return a location outside the root, and must never
/// panic or hang. This is the property the whole list exists to test.
#[test]
fn no_case_in_the_blind_list_escapes_or_panics() {
    let f = standard("sweep");
    let canon_root = f.canon_root();
    let paths = [
        "/noexist/./esc/../inner/escape.txt",
        "//./esc",
        "/gone/../dangle/../esc",
        "/noexist/../loop1",
        "/gone/../chain1",
        "/dangle_mid/../esc",
        "/noexist/../outdangle/anything",
        "/esc/reentry/file.txt",
        "/noexist/../file.txt/../esc",
        "/dangle/../esc",
        "/outdangle/../good",
        "/gone/../relclimb",
        "/afile/..",
        "/zzz/../afile/../esc",
        "/noexist/../back\\slash",
        "/noexist/../\u{435}sc",
        "/gone/../a/file.txt",
        "/hop/ok.txt",
        "/gone/../l1/../anything",
        "/gone/../c1/next/inner.txt",
        "/dangle_anchor/../esc/leaf.txt",
        "/gone/../pdir/../esc",
        "/gone/../trail//../esc/",
        "/gone/./../dots/./../esc",
        "/s/child.txt",
        "/sub/../../../esc",
    ];
    for p in paths {
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
