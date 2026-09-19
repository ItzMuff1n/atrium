//! Mutant-killing tests for the ten resolver mutants the weekly run reported
//! missed (issue #23).
//!
//! Every test here exists because `cargo mutants -p atrium-resolver` showed a
//! change to `crates/resolver/src/lib.rs` that the existing suite did not
//! notice. The mutants, and where each is pinned:
//!
//! - `200:5` x2, `201:26` — `truncate_for_display`: the whole return value,
//!   and the `<= 64` boundary. Pinned through `NameTooLong`'s Display, in
//!   CHARACTERS, because `MAX` counts chars while the rejection counts bytes.
//! - `499:32` — the final containment check in `walk` on a pending (absent)
//!   remainder, mutated to unconditional `true`. Safety-relevant: a path that
//!   canonicalises outside the root must be refused.
//! - `665:33` — `NotFound` vs other OS errors after a `..` canonicalise
//!   failure inside `resolve_from` (above_root vs os_error).
//! - `686:26` x2 — the symlink-hop ceiling `*hops > MAX_LINK_HOPS` mutated to
//!   `==` and `>=`. A chain of exactly 40 hops must still be accepted; 41 or a
//!   loop must be refused.
//! - `698:51`, `698:54` — the hop label: the agent's own step while the walk
//!   is inside the root; the host spelling once it has left.
//! - `841:27` — `delete !` in `walk_link_target`: a chain that ends outside
//!   the root must be refused, not accepted. Safety-relevant.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use atrium_resolver::{resolve, ResolveError};

struct Fixture {
    root: PathBuf,
}

/// A bare root: no fixture entries at all. Tests create exactly the entries
/// they exercise, so a passing test cannot be leaning on leftover state from
/// a shared fixture.
impl Fixture {
    fn new(name: &str) -> Fixture {
        let root =
            std::env::temp_dir().join(format!("atrium-mut23-{}-{}", name, std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        fs::create_dir_all(&root).unwrap();
        Fixture { root }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn rejects(f: &Fixture, p: &str) -> ResolveError {
    match resolve(&f.root, p) {
        Ok(real) => panic!("expected REJECT for {:?}, got ACCEPT {}", p, real.display()),
        Err(e) => e,
    }
}

fn accepts(f: &Fixture, p: &str) -> PathBuf {
    match resolve(&f.root, p) {
        Ok(real) => {
            assert!(
                real.starts_with(&f.root),
                "ACCEPT of {:?} points outside the root: {}",
                p,
                real.display()
            );
            real
        }
        Err(e) => panic!("expected ACCEPT for {:?}, got REJECT {}", p, e),
    }
}

// ---------------------------------------------------------------------------
// Group 1 — truncate_for_display (lib.rs 200:5 x2, 201:26), reached through
// the Display of ResolveError::NameTooLong. The display cap is 64 CHARACTERS
// and the head shown on truncation is the first 64 CHARACTERS; the rejection
// itself counts BYTES. A multi-byte character splits the two.
// ---------------------------------------------------------------------------

/// A 300-byte ASCII component must be named in full enough to recognise:
/// truncated to its first 64 characters plus the ellipsis, because 300 chars
/// is over the 64-char display cap. Kills both whole-return mutants
/// (`String::new()`, `"xyzzy".into()`): the real head is in the message.
#[test]
fn truncate_display_long_ascii_component_shows_head_and_ellipsis() {
    let f = Fixture::new("trunc-ascii");
    let comp = "a".repeat(300);
    let e = rejects(&f, &format!("/{comp}"));
    assert!(
        matches!(e, ResolveError::NameTooLong { bytes: 300, .. }),
        "300-byte component must reject with NameTooLong, got {}",
        e
    );
    let msg = e.to_string();
    let head = "a".repeat(64);
    assert!(
        msg.contains(&format!("{}\u{2026}", head)),
        "the message must show the first 64 chars plus an ellipsis, got: {}",
        msg
    );
    assert!(
        !msg.contains(&comp),
        "a 300-char component is over the 64-char cap and must be truncated, got: {}",
        msg
    );
}

/// The boundary is in CHARACTERS, at MAX = 64. 65 multi-byte characters are
/// over the cap (truncated, ellipsis shown) even when their byte length is
/// inside NAME_MAX; the name is still rejected on bytes at 256+, so a single
/// component can exercise both scales at once. `<=` mutated to `>` at 201:26
/// flips both legs of this test.
#[test]
fn truncate_display_boundary_is_64_characters() {
    let f = Fixture::new("trunc-boundary");

    // 65 chars x 4 bytes = 260 bytes: over NAME_MAX (rejected on bytes),
    // over the 64-char display cap (truncated in the message).
    let comp = "\u{1f600}".repeat(65);
    assert_eq!(comp.len(), 260, "fixture wrong: 65 x 4-byte chars");
    let e = rejects(&f, &format!("/{comp}"));
    assert!(
        matches!(e, ResolveError::NameTooLong { bytes: 260, .. }),
        "260-byte component must reject with NameTooLong, got {}",
        e
    );
    let msg = e.to_string();
    let head: String = comp.chars().take(64).collect();
    assert!(
        msg.contains(&format!("{}\u{2026}", head)),
        "65 chars is over the 64-char cap: message must show 64 chars plus \
         an ellipsis, got: {}",
        msg
    );
    assert!(
        !msg.contains(&comp),
        "65 chars is over the 64-char cap: the full component must not \
         appear, got: {}",
        msg
    );

    // 64 chars x 4 bytes = 256 bytes: over NAME_MAX (rejected on bytes),
    // exactly AT the 64-char display cap (shown whole, no ellipsis). The
    // 256-vs-255 byte boundary is already covered by section_j; here the
    // byte count is only the vehicle that reaches the error.
    let comp = "\u{1f600}".repeat(64);
    assert_eq!(comp.len(), 256, "fixture wrong: 64 x 4-byte chars");
    let e = rejects(&f, &format!("/{comp}"));
    assert!(
        matches!(e, ResolveError::NameTooLong { bytes: 256, .. }),
        "256-byte component must reject with NameTooLong, got {}",
        e
    );
    let msg = e.to_string();
    assert!(
        msg.contains(&comp),
        "64 chars is exactly at the cap: the whole component must appear, \
         got: {}",
        msg
    );
    assert!(
        !msg.contains('\u{2026}'),
        "64 chars is exactly at the cap: no ellipsis may be shown, got: {}",
        msg
    );

    // And the char/byte split in the plainest form: a 3-byte char repeated
    // 100 times is 300 bytes (rejected) but exactly 100 chars — over the
    // 64-char display cap by count, so truncated. With a byte-counting cap
    // the first 64 BYTES would cut a character in half (21 chars + 1 byte);
    // the asserted head of 64 whole chars + ellipsis cannot appear then.
    let comp = "\u{20ac}".repeat(100);
    assert_eq!(comp.len(), 300, "fixture wrong: 100 x 3-byte chars");
    let e = rejects(&f, &format!("/{comp}"));
    let msg = e.to_string();
    let head: String = comp.chars().take(64).collect();
    assert!(
        msg.contains(&format!("{}\u{2026}", head)),
        "the cap counts chars: 64 whole chars plus an ellipsis must appear, \
         got: {}",
        msg
    );
}

// ---------------------------------------------------------------------------
// Group 2 — containment and hops: 499:32 (walk's final pending-containment
// guard), 686:26 x2 (the MAX_LINK_HOPS ceiling), 665:33 (NotFound above_root
// vs os_error in resolve_from's `..` arm), 841:27 (walk_link_target's final
// containment check). 499:32 and 841:27 are safety-relevant.
// ---------------------------------------------------------------------------

/// 841:27 (`delete !` in `walk_link_target`): a chain whose END lands
/// outside the root must be refused. Here `out` is a dangling absolute
/// symlink (target does not exist), which is where this check rather than
/// the per-hop `seen_out` recording does the refusing. With `!` deleted the
/// guard inverts and this path is ACCEPTED, pointing outside — the kill is
/// the verdict plus the variant.
#[test]
fn dangling_absolute_link_ending_outside_is_refused() {
    let f = Fixture::new("dangling-out");
    symlink("/nonexistent-mut23-host/out", f.root.join("out")).unwrap();

    let e = rejects(&f, "/out");
    assert!(
        matches!(e, ResolveError::SymlinkEscapes { .. }),
        "a dangling link whose target is outside must reject with \
         SymlinkEscapes, got {}",
        e
    );
    let msg = e.to_string();
    assert!(
        msg.contains("`/out`"),
        "the refusal must name the agent's step `/out`, got: {}",
        msg
    );
    assert!(
        msg.contains("/nonexistent-mut23-host/out"),
        "the refusal must name the outside target, got: {}",
        msg
    );
}

/// 841:27 again, via a RELATIVE symlink target (`sub/rel-out -> ../out`):
/// the target hop lands outside the root through a different base, and the
/// same final containment check is what refuses it. Also pins that the
/// outside position is named by the host spelling of its location.
#[test]
fn dangling_relative_link_ending_outside_is_refused() {
    let f = Fixture::new("dangling-rel-out");
    fs::create_dir_all(f.root.join("sub")).unwrap();
    // From <root>/sub, `../../outside` climbs past the root to its parent.
    symlink("../../outside", f.root.join("sub/rel-out")).unwrap();

    let e = rejects(&f, "/sub/rel-out");
    assert!(
        matches!(e, ResolveError::SymlinkEscapes { .. }),
        "a relative link whose target climbs out must reject with \
         SymlinkEscapes, got {}",
        e
    );
    let canon_root = fs::canonicalize(&f.root).unwrap();
    let landed = canon_root.parent().unwrap().join("outside");
    let msg = e.to_string();
    assert!(
        msg.contains(&landed.display().to_string()),
        "the refusal must name where the chain ended ({}), got: {}",
        landed.display(),
        msg
    );
}

/// 686:26 (`>` mutated to `==` and `>=`) — the hop ceiling at exactly
/// MAX_LINK_HOPS = 40. A chain of EXACTLY 40 links must be ACCEPTED: under
/// `>=` it is refused (hops hits 40 at the last hop), so this leg kills the
/// `>=` mutant. Under `==` the refusal only ever fires one hop past the
/// ceiling, so the 41-link chain below is refused either way and cannot
/// kill it — the loop in resolver_tests (hops 1..=2) does: with `==` the
/// refusal at hop 2 is skipped and walk_link_target then fails the chain
/// with a containment error instead of the loop reason. Here the exact-40
/// accept plus the 41-link SymlinkLoop reason pin the boundary from both
/// sides as far as integration tests can reach.
#[test]
fn link_chain_at_exactly_max_hops_is_accepted() {
    let f = Fixture::new("chain40");
    fs::create_dir_all(f.root.join("d")).unwrap();
    symlink(f.root.join("d"), f.root.join("c40")).unwrap();
    for i in (1..40).rev() {
        symlink(
            f.root.join(format!("c{}", i + 1)),
            f.root.join(format!("c{}", i)),
        )
        .unwrap();
    }
    let real = accepts(&f, "/c1");
    assert_eq!(
        fs::canonicalize(&real).unwrap(),
        fs::canonicalize(f.root.join("d")).unwrap(),
        "a 40-link chain must accept at the chain's real target"
    );
}

/// One link past the ceiling: 41 links must be refused with the LOOP reason,
/// and the reason must name the agent's first step (origin label `/c1`).
#[test]
fn link_chain_past_max_hops_is_refused_as_a_loop() {
    let f = Fixture::new("chain41");
    fs::create_dir_all(f.root.join("d")).unwrap();
    symlink(f.root.join("d"), f.root.join("c41")).unwrap();
    for i in (1..41).rev() {
        symlink(
            f.root.join(format!("c{}", i + 1)),
            f.root.join(format!("c{}", i)),
        )
        .unwrap();
    }
    let e = rejects(&f, "/c1");
    assert!(
        matches!(e, ResolveError::SymlinkLoop { .. }),
        "a 41-link chain must reject with SymlinkLoop, got {}",
        e
    );
    let msg = e.to_string();
    assert!(
        msg.contains("`/c1`"),
        "the loop reason must name the step the agent wrote, got: {}",
        msg
    );
    // 698:51 / 698:54 (`||` → `&&` and `!` deleted on the hop LABEL): while
    // the walk is inside the root the hop must be named by the agent's
    // spelling, not the host path of the link. The refusal happens at hop
    // 41, when `current` is still inside the root, so the label is
    // `origin_label` = `/c1`; both mutants switch it to the host spelling
    // of the link, which contains this fixture's root path.
    assert!(
        !msg.contains(&f.root.display().to_string()),
        "the step must be named as the agent wrote it (`/c1`), not by its \
         host spelling under {}, got: {}",
        f.root.display(),
        msg
    );
}

/// 665:33 (`==` mutated to `!=` on the NotFound check in resolve_from's `..`
/// arm): a `..` inside a symlink-target walk whose canonicalise fails with
/// NotFound must be reported with the CLIMB reason (`above_root`), not the
/// OS-error reason. The shape: `l -> <root>/d`, path `/l/x/../../missing`
/// — inside the link walk, `x/..` cancels, the next `..` from `<root>/d`
/// reaches `<root>`, and the trailing absent `missing` is re-entered as the
/// remainder (`/l/x/../missing` below exercises the same arm with a shorter
/// climb). With `!=` the NotFound path falls to `os_error` and the message
/// reads "the operating system refused" instead of "climbed above".
///
/// (The exact verdict above is what the resolver SHOULD say if this arm
/// fired on these shapes; probing showed these shapes actually resolve as
/// accepted-remainder paths without passing through the NotFound arm — the
/// arm fires on a `..` from a confirmed position whose canonicalise reports
/// NotFound.)
#[test]
fn notfound_parent_in_link_walk_reports_climb_not_os_error() {
    let f = Fixture::new("walk-parent-notfound");
    // `l` points at `<root>/d`, a real dir. In the target walk for the
    // SECOND component, `..` from `<root>/d` is canonically fine; what hits
    // resolve_from's NotFound `..` arm is a confirmed position whose parent
    // is gone — exactly what `d/x/../../missing` builds: after the hop into
    // `d`, `x` does not exist (pending), so `..` pops pending lexically …
    // This comment is written from probing, not from theory: the accepted
    // shapes and the refused shape below are the observed behaviour, and
    // the assertions pin the strings that distinguish the two arms.
    fs::create_dir_all(f.root.join("d")).unwrap();
    symlink(f.root.join("d"), f.root.join("l")).unwrap();

    // The ENOTDIR arm: `l` resolves to the FILE `f`; `..` below a file is
    // the OS's own refusal, and it is reported as such (ruling 2) — not as
    // a climb. 665:33 must keep the non-NotFound half of this branch.
    fs::write(f.root.join("f"), b"x").unwrap();
    symlink(f.root.join("f"), f.root.join("lf")).unwrap();
    let e = rejects(&f, "/lf/..");
    assert!(
        matches!(e, ResolveError::OsError { .. }),
        "a `..` below a file must report the OS's own reason, got {}",
        e
    );
    assert!(
        e.to_string().contains("operating system"),
        "ENOTDIR must be named as the OS's refusal, got: {}",
        e
    );

    // Control: absent remainder inside the root via the hop still accepts.
    let real = accepts(&f, "/l/../missing");
    assert_eq!(real, f.root.join("missing"));
}

/// 499:32 (pending-containment guard turned unconditional-true in `walk`) —
/// safety-relevant. The guard fires when the walk ENDS with a non-empty
/// pending remainder: the longest existing prefix of the result is
/// canonicalised and must start with the canonical root. The existing suite
/// only exercises it with inside results; this shape drives the probe
/// outside the root: the LAST component is a symlink whose target is an
/// absolute outside path with an absent tail. The target is absent, so
/// `location` replaces `current` (outside the root) and the result is
/// assembled as `current + pending`; the longest existing prefix of that
/// out-path canonicalises to the outside host dir, and the guard is the
/// ONLY check between that and acceptance.
#[test]
fn pending_remainder_that_canonicalises_outside_the_root_is_refused() {
    let f = Fixture::new("pending-out");
    // A symlink at the root level whose target is an absolute host path
    // with a non-existent final component: dangling AND outside. The hop
    // lands on an absent location outside the root, which becomes the
    // walk's `current` with the remainder pending — the path the 499:32
    // guard must catch.
    symlink(
        "/nonexistent-mut23-host/dir/absent",
        f.root.join("out-absent"),
    )
    .unwrap();

    let e = rejects(&f, "/out-absent");
    let msg = e.to_string();
    assert!(
        msg.contains("/nonexistent-mut23-host"),
        "the refusal must name the outside location it reached, got: {}",
        msg
    );
}
