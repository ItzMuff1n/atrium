//! Phase 2c tests — in-process, against the library.
//!
//! The CLI harness (`hand-test-2c.sh`) covers what can be expressed as arguments
//! and observed as text. These cover what cannot: a hash against published
//! vectors, a name containing a newline, a damaged record, a socket refusing a
//! snapshot, and the inode of the root surviving a restore.
//!
//! Every test builds its own throwaway root, store and outside directory under
//! `/tmp` and removes them afterwards. `cleanup` is called at every exit point;
//! a leftover directory is a leak, not a failure, but the project's habit is to
//! leave nothing behind.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use atrium_snapshot::sha256::{hex, sha256, Sha256};
use atrium_snapshot::{
    build_fixtures, create, list, parse_record, render_record, restore, walk_tree, Entry,
    EntryKind, Label, SnapshotError,
};

// ---------------------------------------------------------------------------
// Fixture scaffolding
// ---------------------------------------------------------------------------

struct Ctx {
    base: PathBuf,
    root: PathBuf,
    store: PathBuf,
    outside: PathBuf,
}

impl Ctx {
    fn new(tag: &str) -> Ctx {
        let base = std::env::temp_dir().join(format!(
            "atrium-2c-test-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0)
        ));
        let ctx = Ctx {
            root: base.join("root"),
            store: base.join("store"),
            outside: base.join("outside"),
            base,
        };
        let _ = fs::remove_dir_all(&ctx.base);
        fs::create_dir_all(&ctx.store).unwrap();
        ctx
    }

    fn fixtures(&self) {
        build_fixtures(&self.root, &self.outside).unwrap();
    }
}

impl Drop for Ctx {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn label(s: &str) -> Label {
    Label::parse(s).unwrap()
}

fn tree_hash(p: &Path) -> String {
    // A cheap, order-independent-enough summary for comparing two tree states in
    // one assertion: every path, kind, mode, size and content hash.
    let mut entries = walk_tree(p).unwrap();
    entries.sort_by(|a, b| a.rel.cmp(&b.rel));
    let mut out = String::new();
    for e in entries {
        out.push_str(&format!(
            "{:?} {:?} {:o} {} {}\n",
            String::from_utf8_lossy(&e.rel),
            e.kind,
            e.mode,
            e.size,
            hex(&e.hash)
        ));
    }
    out
}

fn count_entries(p: &Path) -> usize {
    walk_tree(p).unwrap().len()
}

// ---------------------------------------------------------------------------
// SHA-256 — the foundation the record's integrity rests on
// ---------------------------------------------------------------------------

#[test]
fn sha256_matches_the_published_vectors() {
    // FIPS 180-4 / NIST. If any of these is wrong, §F's "a damaged snapshot is
    // never restorable" is not a guarantee, so this is checked first.
    assert_eq!(
        hex(&sha256(b"")),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        hex(&sha256(b"abc")),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        hex(&sha256(
            b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
        )),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
    assert_eq!(
        hex(&sha256(
            b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu"
        )),
        "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1"
    );
    assert_eq!(
        hex(&sha256(&vec![b'a'; 1_000_000])),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
}

#[test]
fn sha256_streaming_equals_one_shot() {
    // "Streaming" must not be an assertion; a file larger than the chunk size is
    // hashed both ways here.
    for n in [0usize, 1, 63, 64, 65, 127, 128, 129, 4096, 300_000] {
        let data: Vec<u8> = (0..n).map(|i| (i % 251) as u8).collect();
        let one_shot = sha256(&data);
        for chunk in [1usize, 7, 64, 997] {
            let mut h = Sha256::new();
            for part in data.chunks(chunk) {
                h.update(part);
            }
            assert_eq!(h.finish(), one_shot, "n={n} chunk={chunk}");
        }
    }
}

// ---------------------------------------------------------------------------
// Label — the only outside input, and where a name must not become a path
// ---------------------------------------------------------------------------

#[test]
fn label_refuses_every_way_a_name_can_be_a_path() {
    // attack-list-2c.md §G.1–G.3, §G.5–G.6
    assert!(matches!(
        Label::parse("../../outside-evil"),
        Err(SnapshotError::LabelHasSeparator { .. })
    ));
    assert!(matches!(
        Label::parse("/etc/evil"),
        Err(SnapshotError::LabelHasSeparator { .. })
    ));
    assert!(matches!(Label::parse("."), Err(SnapshotError::LabelDot)));
    assert!(matches!(
        Label::parse(".."),
        Err(SnapshotError::LabelDotDot { .. })
    ));
    assert!(matches!(Label::parse(""), Err(SnapshotError::LabelEmpty)));
    assert!(matches!(
        Label::parse("a\0b"),
        Err(SnapshotError::LabelHasNul)
    ));
    assert!(matches!(
        Label::parse("a/b"),
        Err(SnapshotError::LabelHasSeparator { .. })
    ));
}

#[test]
fn label_length_is_measured_in_bytes_not_characters() {
    // The case a character-counting implementation gets wrong: 200 Hebrew
    // characters is 400 bytes and must be refused even though 200 < 255.
    let ascii_255 = "a".repeat(255);
    assert!(
        Label::parse(&ascii_255).is_ok(),
        "255 bytes must be allowed"
    );

    let ascii_256 = "a".repeat(256);
    assert!(matches!(
        Label::parse(&ascii_256),
        Err(SnapshotError::LabelTooLong { bytes: 256 })
    ));

    let hebrew_200: String = std::iter::repeat('\u{05D0}').take(200).collect();
    assert_eq!(hebrew_200.chars().count(), 200, "200 characters");
    assert_eq!(hebrew_200.len(), 400, "400 bytes");
    match Label::parse(&hebrew_200) {
        Err(SnapshotError::LabelTooLong { bytes }) => assert_eq!(bytes, 400),
        other => panic!("200 Hebrew characters (400 bytes) must be refused, got {other:?}"),
    }
}

#[test]
fn label_refusal_names_the_specific_problem() {
    // §G.9: a bare "failed" is not acceptable.
    let sep = Label::parse("../../x").unwrap_err().to_string();
    assert!(
        sep.contains("path separator"),
        "reason should name the separator: {sep}"
    );
    let long = Label::parse(&"a".repeat(300)).unwrap_err().to_string();
    assert!(
        long.contains("300 bytes") && long.contains("255"),
        "reason should name the size and the limit: {long}"
    );
}

// ---------------------------------------------------------------------------
// The record — what makes a directory a snapshot
// ---------------------------------------------------------------------------

#[test]
fn record_round_trips_including_a_newline_in_a_name() {
    let entries = vec![
        Entry {
            rel: b"a/file.txt".to_vec(),
            kind: EntryKind::File,
            mode: 0o644,
            size: 3,
            hash: sha256(b"abc"),
        },
        // A name with a newline in it: the case that makes a line-delimited
        // format wrong, and the reason `namelen` exists.
        Entry {
            rel: b"weird/line\nbreak.txt".to_vec(),
            kind: EntryKind::File,
            mode: 0o600,
            size: 1,
            hash: sha256(b"x"),
        },
        // A name that is not valid UTF-8. Held as raw bytes throughout.
        Entry {
            rel: b"weird/\xff\xfe.bin".to_vec(),
            kind: EntryKind::File,
            mode: 0o644,
            size: 0,
            hash: sha256(b""),
        },
        Entry {
            rel: b"link".to_vec(),
            kind: EntryKind::Symlink,
            mode: 0o777,
            size: 8,
            hash: sha256(b"a/target"),
        },
    ];

    let l = label("base");
    let bytes = render_record(&l, Path::new("/tmp/the-root"), &entries);
    let parsed = parse_record(&l, &bytes).expect("the record must parse");

    assert_eq!(parsed.origin_root, "/tmp/the-root");
    assert_eq!(parsed.entries.len(), 4);
    for (a, b) in parsed.entries.iter().zip(entries.iter()) {
        assert_eq!(a, b, "entry must survive the round trip byte-exactly");
    }
}

#[test]
fn record_is_deterministic() {
    // Two records of the same tree must be byte-identical, which is what makes
    // "taking a snapshot changes nothing" a meaningful comparison.
    let a = vec![
        Entry {
            rel: b"x".to_vec(),
            kind: EntryKind::File,
            mode: 0o644,
            size: 0,
            hash: sha256(b""),
        },
        Entry {
            rel: b"y".to_vec(),
            kind: EntryKind::Dir,
            mode: 0o755,
            size: 0,
            hash: [0u8; 32],
        },
    ];
    let mut b = a.clone();
    b.reverse();
    let l = label("same");
    assert_eq!(
        render_record(&l, Path::new("/r"), &a),
        render_record(&l, Path::new("/r"), &a)
    );
}

#[test]
fn a_truncated_record_is_not_a_snapshot() {
    let entries = vec![Entry {
        rel: b"f".to_vec(),
        kind: EntryKind::File,
        mode: 0o644,
        size: 1,
        hash: sha256(b"z"),
    }];
    let l = label("cut");
    let full = render_record(&l, Path::new("/r"), &entries);

    // Chop off the completion line: exactly what a process killed at the wrong
    // moment leaves behind.
    let cut = full.len() - 8;
    assert!(parse_record(&l, &full[..cut]).is_err());

    // A record whose completion count lies is also refused.
    let mut lying = full.clone();
    let pos = String::from_utf8_lossy(&lying).find("END 1").unwrap();
    lying[pos] = b'E';
    let lying_text = String::from_utf8_lossy(&lying).replace("END 1", "END 9");
    assert!(parse_record(&l, lying_text.as_bytes()).is_err());
}

// ---------------------------------------------------------------------------
// Create — capture the whole root, and nothing else
// ---------------------------------------------------------------------------

#[test]
fn create_captures_the_fixture_tree_exactly() {
    let ctx = Ctx::new("capture");
    ctx.fixtures();

    let l = label("base");
    create(&ctx.root, &ctx.store, &l).unwrap();

    // The snapshot's tree must match the root's, entry for entry.
    let snap = ctx.store.join("base");
    assert_eq!(
        tree_hash(&snap),
        tree_hash(&ctx.root),
        "the snapshot must describe the same tree as the root"
    );

    // And it must actually contain the awkward ones, not merely agree in count.
    assert!(snap.join("weird/with space.txt").is_file());
    assert!(snap.join("weird/caf\u{e9}.txt").is_file());
    assert!(snap.join("weird/empty-file.txt").is_file());
    assert_eq!(
        fs::metadata(snap.join("weird/empty-file.txt"))
            .unwrap()
            .len(),
        0
    );
    assert!(snap.join("deep/a/b/c/d/e/leaf.txt").is_file());
    // Empty directories survive as directories.
    assert!(snap.join("home/work").is_dir());
    assert!(snap.join("home/documents/sub").is_dir());
}

#[test]
fn create_does_not_change_the_root() {
    // §A.11: photographs do not touch the subject.
    let ctx = Ctx::new("nochange");
    ctx.fixtures();
    let before = tree_hash(&ctx.root);

    create(&ctx.root, &ctx.store, &label("base")).unwrap();

    assert_eq!(
        tree_hash(&ctx.root),
        before,
        "the root must be untouched by a snapshot"
    );
}

#[test]
fn modes_survive_a_snapshot() {
    let ctx = Ctx::new("modes");
    ctx.fixtures();
    create(&ctx.root, &ctx.store, &label("base")).unwrap();

    let snap = ctx.store.join("base");
    assert_eq!(
        fs::metadata(snap.join("weird/mode-755.sh"))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777,
        0o755
    );
    assert_eq!(
        fs::metadata(snap.join("weird/mode-444.txt"))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777,
        0o444
    );
}

#[test]
fn a_duplicate_label_is_refused_and_does_not_overwrite() {
    // §B.9: silently replacing a snapshot destroys an undo while reporting success.
    let ctx = Ctx::new("dup");
    ctx.fixtures();
    let l = label("base");
    create(&ctx.root, &ctx.store, &l).unwrap();

    // Damage the snapshot, then try to re-create over it. The refusal must leave
    // the damaged one alone rather than replacing it with a clean one.
    let marker = ctx.store.join("base/home/documents/notes.txt");
    fs::write(&marker, "PROOF-NOT-OVERWRITTEN").unwrap();

    match create(&ctx.root, &ctx.store, &l) {
        Err(SnapshotError::LabelExists { .. }) => {}
        other => panic!("a duplicate label must be refused, got {other:?}"),
    }
    assert_eq!(
        fs::read_to_string(&marker).unwrap(),
        "PROOF-NOT-OVERWRITTEN",
        "the existing snapshot must not have been overwritten"
    );
}

// ---------------------------------------------------------------------------
// Symlinks — copied, never followed
// ---------------------------------------------------------------------------

#[test]
fn symlinks_are_copied_as_links_and_never_followed() {
    // §E — the section that exists because a naive recursive copy follows links,
    // which destroys the sandbox's link structure and can pull host data into the
    // store. Both failure modes are silent.
    let ctx = Ctx::new("symlink");
    ctx.fixtures();
    create(&ctx.root, &ctx.store, &label("base")).unwrap();
    let snap = ctx.store.join("base");

    // E.1 the escape link keeps its exact target string
    let escape = fs::read_link(snap.join("link-escape")).unwrap();
    assert_eq!(
        escape, ctx.outside,
        "the link's target must be preserved verbatim"
    );

    // E.2 the outside directory's contents must NOT be inside the snapshot
    let sentinel_contents = fs::read_to_string(ctx.outside.join("sentinel.txt")).unwrap();
    assert_eq!(sentinel_contents, "sentinel\n");
    let mut found_sentinel = false;
    let mut found_keep = false;
    for e in walk_tree(&snap).unwrap() {
        let name = String::from_utf8_lossy(&e.rel).to_string();
        if name.contains("sentinel") || name.contains("keep.txt") {
            found_sentinel = true;
        }
        if name == "link-escape/sub" || name.starts_with("link-escape/") {
            found_keep = true;
        }
    }
    assert!(
        !found_sentinel && !found_keep,
        "following link-escape would have copied the outside directory in"
    );

    // E.6 a link to a directory is a link, not a second copy.
    let link_md = fs::symlink_metadata(snap.join("link-docs")).unwrap();
    assert!(
        link_md.file_type().is_symlink(),
        "link-docs must stay a symlink"
    );
    // Checked structurally, NOT by `snap.join("link-docs/notes.txt").exists()`:
    // that call *follows* the link, finds the snapshot's own copy of
    // home/documents, and would report a copy where there is none. The question
    // is whether the walk recorded anything under the link's name.
    let under_link: Vec<String> = walk_tree(&snap)
        .unwrap()
        .iter()
        .map(|e| String::from_utf8_lossy(&e.rel).to_string())
        .filter(|n| n.starts_with("link-docs/"))
        .collect();
    assert!(
        under_link.is_empty(),
        "nothing may be recorded under a symlink's name: {under_link:?}"
    );
}

#[test]
fn a_dangling_symlink_survives_and_is_not_an_error() {
    // §E.4: the target does not exist and the link is still copied exactly.
    let ctx = Ctx::new("dangling");
    ctx.fixtures();
    create(&ctx.root, &ctx.store, &label("base")).unwrap();

    let snap = ctx.store.join("base");
    let md = fs::symlink_metadata(snap.join("link-broken")).unwrap();
    assert!(md.file_type().is_symlink());
    assert_eq!(
        fs::read_link(snap.join("link-broken")).unwrap(),
        Path::new("nothing-here.txt")
    );
}

// ---------------------------------------------------------------------------
// Restore — exactly back
// ---------------------------------------------------------------------------

#[test]
fn restore_puts_everything_back_and_removes_strangers() {
    // §C.1–C.5, and the plan's own line: snapshot, delete everything, restore.
    let ctx = Ctx::new("restore");
    ctx.fixtures();
    let before = tree_hash(&ctx.root);
    create(&ctx.root, &ctx.store, &label("base")).unwrap();

    // Delete everything in the environment, down to the last hidden file.
    for item in fs::read_dir(&ctx.root).unwrap() {
        let p = item.unwrap().path();
        if fs::symlink_metadata(&p).unwrap().is_dir() {
            fs::remove_dir_all(&p).unwrap();
        } else {
            fs::remove_file(&p).unwrap();
        }
    }
    assert_eq!(count_entries(&ctx.root), 0, "the environment must be empty");
    // The snapshot lives outside the environment, so emptying the environment
    // must not have touched it.
    assert!(ctx.store.join("base").is_dir());

    restore(&ctx.root, &ctx.store, &label("base")).unwrap();

    assert_eq!(
        tree_hash(&ctx.root),
        before,
        "the restored environment must match the original exactly"
    );
}

#[test]
fn restore_removes_a_file_that_was_not_in_the_snapshot() {
    let ctx = Ctx::new("stranger");
    ctx.fixtures();
    create(&ctx.root, &ctx.store, &label("base")).unwrap();

    fs::write(ctx.root.join("stranger.txt"), "not in the snapshot").unwrap();
    restore(&ctx.root, &ctx.store, &label("base")).unwrap();

    assert!(
        !ctx.root.join("stranger.txt").exists(),
        "a restore that only adds things back is not a restore"
    );
}

#[test]
fn restore_brings_back_a_replaced_symlink() {
    // §C.6
    let ctx = Ctx::new("replink");
    ctx.fixtures();
    create(&ctx.root, &ctx.store, &label("base")).unwrap();

    fs::remove_file(ctx.root.join("link-docs")).unwrap();
    fs::create_dir(ctx.root.join("link-docs")).unwrap();
    fs::write(ctx.root.join("link-docs/real-file.txt"), "was a link").unwrap();

    restore(&ctx.root, &ctx.store, &label("base")).unwrap();

    let md = fs::symlink_metadata(ctx.root.join("link-docs")).unwrap();
    assert!(
        md.file_type().is_symlink(),
        "link-docs must be a symlink again"
    );
    assert_eq!(
        fs::read_link(ctx.root.join("link-docs")).unwrap(),
        Path::new("home/documents")
    );
    // And it still works.
    assert_eq!(
        fs::read_to_string(ctx.root.join("link-docs/notes.txt")).unwrap(),
        "hi"
    );
}

#[test]
fn the_root_directory_itself_survives_a_restore() {
    // §C.11: contents are replaced, the environment itself is not deleted and
    // recreated. Anything else holding a reference to the directory keeps working.
    use std::os::unix::fs::MetadataExt;
    let ctx = Ctx::new("inode");
    ctx.fixtures();
    create(&ctx.root, &ctx.store, &label("base")).unwrap();

    let before = fs::metadata(&ctx.root).unwrap().ino();
    restore(&ctx.root, &ctx.store, &label("base")).unwrap();
    let after = fs::metadata(&ctx.root).unwrap().ino();

    assert_eq!(
        before, after,
        "the root's inode must be unchanged by a restore"
    );
}

#[test]
fn restore_is_idempotent() {
    // §C.12
    let ctx = Ctx::new("idem");
    ctx.fixtures();
    create(&ctx.root, &ctx.store, &label("base")).unwrap();
    restore(&ctx.root, &ctx.store, &label("base")).unwrap();
    let once = tree_hash(&ctx.root);
    restore(&ctx.root, &ctx.store, &label("base")).unwrap();
    assert_eq!(
        tree_hash(&ctx.root),
        once,
        "a second restore must change nothing"
    );
}

// ---------------------------------------------------------------------------
// The refusal that is the whole point: the store must be outside the root
// ---------------------------------------------------------------------------

#[test]
fn a_store_inside_the_root_is_refused() {
    let ctx = Ctx::new("inside");
    ctx.fixtures();
    let inside = ctx.root.join("snapshots");

    match create(&ctx.root, &inside, &label("base")) {
        Err(SnapshotError::StoreInsideRoot { store, root }) => {
            assert!(store.contains("snapshots") && root.contains("root"));
        }
        other => panic!("a store inside the root must be refused, got {other:?}"),
    }
}

#[test]
fn a_store_equal_to_the_root_is_refused() {
    let ctx = Ctx::new("equal");
    ctx.fixtures();
    match create(&ctx.root, &ctx.root, &label("base")) {
        Err(SnapshotError::StoreEqualsRoot { .. }) => {}
        other => panic!("store == root must be refused, got {other:?}"),
    }
}

#[test]
fn a_store_that_does_not_exist_is_refused_not_created() {
    // §B.7: a mistyped store path must fail rather than become an empty store
    // that then "works".
    let ctx = Ctx::new("missingstore");
    ctx.fixtures();
    let nope = ctx.base.join("no-such-store");
    match create(&ctx.root, &nope, &label("base")) {
        Err(SnapshotError::StoreMissing { .. }) => {}
        other => panic!("a missing store must be refused, got {other:?}"),
    }
    assert!(!nope.exists(), "the store must not have been created");
}

// ---------------------------------------------------------------------------
// The damaged-snapshot refusal, and that it leaves the environment alone
// ---------------------------------------------------------------------------

#[test]
fn a_snapshot_altered_after_creation_is_refused_and_the_root_is_untouched() {
    // §F.2 — the line that proves the completed record is load-bearing rather
    // than decorative.
    let ctx = Ctx::new("altered");
    ctx.fixtures();
    create(&ctx.root, &ctx.store, &label("base")).unwrap();

    // Change something in the root, so a restore would visibly differ.
    fs::write(ctx.root.join("home/documents/notes.txt"), "WORKING-DATA").unwrap();
    let working = tree_hash(&ctx.root);

    // Now damage the SNAPSHOT (not the root).
    fs::write(
        ctx.store.join("base/home/documents/notes.txt"),
        "TAMPERED-IN-THE-STORE",
    )
    .unwrap();

    match restore(&ctx.root, &ctx.store, &label("base")) {
        Err(SnapshotError::SnapshotAltered { detail, which, .. }) => {
            assert_eq!(which, "store", "the mismatch must be found in the snapshot");
            assert!(
                detail.contains("notes.txt"),
                "the reason should name the changed file: {detail}"
            );
        }
        other => panic!("an altered snapshot must be refused, got {other:?}"),
    }

    assert_eq!(
        tree_hash(&ctx.root),
        working,
        "the environment must be untouched when a restore is refused"
    );
}

#[test]
fn a_same_length_tamper_is_still_caught_by_the_hash() {
    // The counterpart to the optimisation above. After hashing files on their way
    // out, the cheap checks (names, kinds, sizes, modes) decide most cases without
    // opening anything — so this test exists to prove the *hash* path still fires
    // where the cheap checks cannot: bytes changed, length preserved.
    let ctx = Ctx::new("samelength");
    ctx.fixtures();
    create(&ctx.root, &ctx.store, &label("base")).unwrap();

    let target = ctx.store.join("base/home/documents/notes.txt");
    let original = fs::read(&target).unwrap();
    assert_eq!(original, b"hi");

    // Same length, different bytes. A size-only check would pass this.
    fs::write(&target, b"HI").unwrap();
    assert_eq!(fs::metadata(&target).unwrap().len(), original.len() as u64);

    match restore(&ctx.root, &ctx.store, &label("base")) {
        Err(SnapshotError::SnapshotAltered { detail, which, .. }) => {
            assert_eq!(which, "store");
            assert!(
                detail.contains("checksum"),
                "the reason must name the checksum, proving the hash path decided it: {detail}"
            );
            assert!(detail.contains("notes.txt"), "{detail}");
        }
        other => panic!("a same-length tamper must be caught by the hash, got {other:?}"),
    }
}

#[test]
fn a_missing_file_in_the_snapshot_is_refused() {
    // §F.3
    let ctx = Ctx::new("missingfile");
    ctx.fixtures();
    create(&ctx.root, &ctx.store, &label("base")).unwrap();
    fs::remove_file(ctx.store.join("base/deep/a/b/c/d/e/leaf.txt")).unwrap();

    match restore(&ctx.root, &ctx.store, &label("base")) {
        Err(SnapshotError::SnapshotAltered { detail, .. }) => {
            assert!(
                detail.contains("leaf.txt") || detail.contains("missing"),
                "{detail}"
            );
        }
        other => panic!("a snapshot missing a recorded file must be refused, got {other:?}"),
    }
}

#[test]
fn a_directory_with_no_record_is_not_a_snapshot() {
    // §F.1 — what a create process killed at the wrong moment leaves behind.
    let ctx = Ctx::new("norecord");
    ctx.fixtures();
    fs::create_dir_all(ctx.store.join("orphan/home")).unwrap();
    fs::write(ctx.store.join("orphan/home/x.txt"), "half a snapshot").unwrap();

    // Not listed...
    let found = list(&ctx.store).unwrap();
    assert!(
        found.is_empty(),
        "a directory with no completed record must not be listed as a snapshot"
    );
    // ...and not restorable.
    match restore(&ctx.root, &ctx.store, &label("orphan")) {
        Err(SnapshotError::NotASnapshot { .. }) => {}
        other => panic!("an incomplete snapshot must be refused, got {other:?}"),
    }
}

#[test]
fn a_truncated_record_is_not_restorable() {
    let ctx = Ctx::new("trunc");
    ctx.fixtures();
    create(&ctx.root, &ctx.store, &label("base")).unwrap();

    let record = ctx.store.join("base.snapshot");
    let bytes = fs::read(&record).unwrap();
    // Drop the completion line.
    let text = String::from_utf8_lossy(&bytes).to_string();
    let cut = text.rfind("END ").unwrap();
    fs::write(&record, &text.as_bytes()[..cut]).unwrap();

    assert!(
        list(&ctx.store).unwrap().is_empty(),
        "a truncated record is not a snapshot"
    );
    match restore(&ctx.root, &ctx.store, &label("base")) {
        Err(SnapshotError::NotASnapshot { .. }) | Err(SnapshotError::RecordMalformed { .. }) => {}
        other => panic!("a truncated record must be refused, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Cross-root restore
// ---------------------------------------------------------------------------

#[test]
fn restoring_into_a_different_root_is_refused() {
    // §C.8: a plausible disaster, and not a silent default.
    let ctx = Ctx::new("crossroot");
    ctx.fixtures();
    create(&ctx.root, &ctx.store, &label("base")).unwrap();

    let other = ctx.base.join("other-root");
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join("important.txt"), "this is someone else's data").unwrap();

    match restore(&other, &ctx.store, &label("base")) {
        Err(SnapshotError::DifferentRoot {
            snapshot_root,
            given_root,
        }) => {
            assert!(snapshot_root.contains("root"));
            assert!(given_root.contains("other-root"));
        }
        other => panic!("a cross-root restore must be refused, got {other:?}"),
    }
    assert!(
        other.join("important.txt").exists(),
        "the other root must be untouched"
    );
}

#[test]
fn a_bad_id_leaves_the_root_untouched() {
    // §C.9: the refusal happens before anything is deleted.
    let ctx = Ctx::new("badid");
    ctx.fixtures();
    create(&ctx.root, &ctx.store, &label("base")).unwrap();
    let before = tree_hash(&ctx.root);

    match restore(&ctx.root, &ctx.store, &label("no-such-snapshot")) {
        Err(SnapshotError::NotASnapshot { .. }) => {}
        other => panic!("an unknown id must be refused, got {other:?}"),
    }
    assert_eq!(tree_hash(&ctx.root), before, "the root must be untouched");
}

// ---------------------------------------------------------------------------
// The root's own validation
// ---------------------------------------------------------------------------

#[test]
fn a_symlink_root_is_refused() {
    use std::os::unix::fs::symlink;
    let ctx = Ctx::new("linkroot");
    ctx.fixtures();
    let link = ctx.base.join("root-link");
    symlink(&ctx.root, &link).unwrap();

    match create(&link, &ctx.store, &label("base")) {
        Err(SnapshotError::RootIsSymlink { .. }) => {}
        other => panic!("a symlink root must be refused, got {other:?}"),
    }
}

#[test]
fn the_filesystem_root_is_refused_by_name() {
    match create(Path::new("/"), Path::new("/tmp"), &label("base")) {
        Err(SnapshotError::RootIsFilesystemRoot) => {}
        other => panic!("--root / must be refused, got {other:?}"),
    }
}

#[test]
fn a_root_that_is_a_file_is_refused() {
    let ctx = Ctx::new("fileroot");
    fs::create_dir_all(&ctx.base).unwrap();
    let file = ctx.base.join("a-file");
    fs::write(&file, "not a directory").unwrap();

    match create(&file, &ctx.store, &label("base")) {
        Err(SnapshotError::RootNotDirectory { .. }) => {}
        other => panic!("a file as root must be refused, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// The boring half
// ---------------------------------------------------------------------------

#[test]
fn an_empty_root_snapshots_and_restores() {
    // §J.6: an undo mechanism that crashes on "nothing to undo" is broken at the
    // edge where it is most likely to be called.
    let ctx = Ctx::new("empty");
    fs::create_dir_all(&ctx.root).unwrap();
    create(&ctx.root, &ctx.store, &label("base")).unwrap();
    restore(&ctx.root, &ctx.store, &label("base")).unwrap();
    assert_eq!(count_entries(&ctx.root), 0);
}

#[test]
fn listing_an_empty_store_is_not_an_error() {
    // §J.7
    let ctx = Ctx::new("emptylist");
    let found = list(&ctx.store).unwrap();
    assert!(found.is_empty());
}

#[test]
fn a_file_with_a_newline_in_its_name_round_trips() {
    // §J.5: names are bytes; anything treating output as line-delimited gets this
    // wrong, and it is worth one test to prove which way this implementation went.
    let ctx = Ctx::new("newlinename");
    fs::create_dir_all(ctx.root.join("d")).unwrap();
    let weird = ctx.root.join("d/line\nbreak.txt");
    fs::write(&weird, "content").unwrap();

    create(&ctx.root, &ctx.store, &label("base")).unwrap();
    fs::remove_file(&weird).unwrap();
    restore(&ctx.root, &ctx.store, &label("base")).unwrap();

    assert!(
        weird.exists(),
        "a file whose name contains a newline must be captured and restored"
    );
    assert_eq!(fs::read_to_string(&weird).unwrap(), "content");
}

#[test]
fn restore_empties_a_read_only_directory_instead_of_failing() {
    // A REAL DEFECT, found by the blind list (`blind-attack-list-2c.md` item 26)
    // after the rest of the phase was already green.
    //
    // A directory recorded as read-only cannot have its children deleted, so
    // `clear_contents` failed on it — and it failed *after* deleting everything
    // else, leaving a half-cleared environment. Observed before the fix: restore
    // exited 1 with "Permission denied (os error 13)", the modified file was gone
    // rather than restored, and only the read-only directory survived.
    //
    // A refusal that damages the environment is the worst outcome in this phase:
    // the tool exists so that nothing is ever lost by accident. The fix makes the
    // directories writable first, and does so as a pre-flight check so that an
    // unremovable tree refuses BEFORE the first deletion.
    let ctx = Ctx::new("readonly");
    fs::create_dir_all(ctx.root.join("rodir")).unwrap();
    fs::write(ctx.root.join("rodir/f.txt"), "x\n").unwrap();
    fs::write(ctx.root.join("top.txt"), "top\n").unwrap();
    create(&ctx.root, &ctx.store, &label("base")).unwrap();

    // Make the directory unwritable, then modify something else, so the restore
    // has real work to do and the read-only directory is on its path.
    fs::set_permissions(ctx.root.join("rodir"), fs::Permissions::from_mode(0o555)).unwrap();
    fs::write(ctx.root.join("top.txt"), "MODIFIED\n").unwrap();

    restore(&ctx.root, &ctx.store, &label("base")).expect(
        "restore must cope with a read-only directory in the environment, not fail halfway",
    );

    assert_eq!(
        fs::read_to_string(ctx.root.join("top.txt")).unwrap(),
        "top\n",
        "the modified file must have been restored"
    );
    assert_eq!(
        fs::read_to_string(ctx.root.join("rodir/f.txt")).unwrap(),
        "x\n",
        "the file inside the read-only directory must be restored too"
    );
}

#[test]
fn two_snapshots_with_different_labels_are_independent() {
    // §J.2
    let ctx = Ctx::new("two");
    ctx.fixtures();
    create(&ctx.root, &ctx.store, &label("first")).unwrap();

    fs::write(ctx.root.join("home/documents/notes.txt"), "second version").unwrap();
    create(&ctx.root, &ctx.store, &label("second")).unwrap();

    restore(&ctx.root, &ctx.store, &label("first")).unwrap();
    assert_eq!(
        fs::read_to_string(ctx.root.join("home/documents/notes.txt")).unwrap(),
        "hi",
        "the older snapshot must restore its own state"
    );

    let found = list(&ctx.store).unwrap();
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].label, "first");
    assert_eq!(found[1].label, "second");
}

#[test]
fn a_special_file_refuses_the_whole_snapshot() {
    // §9.5: a socket in an environment is exactly the shape of a thing that gets
    // dropped silently, so it refuses by name instead.
    use atrium_snapshot::bind_probe_socket;
    let ctx = Ctx::new("socket");
    ctx.fixtures();
    let sock = ctx.root.join("home/work/agent.sock");
    let _listener = bind_probe_socket(&sock).expect("bind a probe socket");

    match create(&ctx.root, &ctx.store, &label("base")) {
        Err(SnapshotError::UnsupportedEntry { path, kind }) => {
            assert!(
                path.contains("agent.sock"),
                "the refusal should name the file: {path}"
            );
            assert_eq!(kind, "socket");
        }
        other => panic!("a socket must refuse the snapshot, got {other:?}"),
    }
    // And nothing that looks like a snapshot was left behind.
    assert!(list(&ctx.store).unwrap().is_empty());
}

#[test]
fn hard_links_are_restored_as_separate_files() {
    // §I.2: the data survives; the fact that two names shared one file does not.
    // Recorded rather than hidden, and asserted so the record stays accurate.
    use std::os::unix::fs::MetadataExt;
    let ctx = Ctx::new("hardlink");
    ctx.fixtures();

    // Before: the two names are the same file.
    let one = fs::metadata(ctx.root.join("hard/one.txt")).unwrap();
    let two = fs::metadata(ctx.root.join("hard/two.txt")).unwrap();
    assert_eq!(one.ino(), two.ino(), "the fixtures must start hard-linked");

    create(&ctx.root, &ctx.store, &label("base")).unwrap();
    restore(&ctx.root, &ctx.store, &label("base")).unwrap();

    assert_eq!(
        fs::read_to_string(ctx.root.join("hard/two.txt")).unwrap(),
        "linked",
        "the data must survive"
    );
    let one_after = fs::metadata(ctx.root.join("hard/one.txt")).unwrap();
    let two_after = fs::metadata(ctx.root.join("hard/two.txt")).unwrap();
    assert_ne!(
        one_after.ino(),
        two_after.ino(),
        "documented consequence (§I.2): the inode is not shared after a restore"
    );
}

#[test]
fn a_deep_tree_does_not_overflow_the_stack() {
    // The walk is iterative for this reason; a recursive one would die here.
    let ctx = Ctx::new("deep");
    let mut p = ctx.root.clone();
    for i in 0..600 {
        p = p.join(format!("d{i}"));
    }
    fs::create_dir_all(&p).unwrap();
    fs::write(p.join("leaf.txt"), "bottom").unwrap();

    create(&ctx.root, &ctx.store, &label("base")).unwrap();
    fs::remove_dir_all(ctx.root.join("d0")).unwrap();
    restore(&ctx.root, &ctx.store, &label("base")).unwrap();

    assert!(p.join("leaf.txt").exists(), "the deep tree must come back");
}

#[test]
fn exit_codes_are_what_the_readme_says() {
    // §J.8: stated and tested, not assumed.
    assert_eq!(
        SnapshotError::MissingFlag { flag: "--root" }.exit_code(),
        2,
        "usage errors are 2"
    );
    assert_eq!(
        SnapshotError::StoreInsideRoot {
            store: "s".into(),
            root: "r".into()
        }
        .exit_code(),
        1,
        "refusals are 1"
    );
}
