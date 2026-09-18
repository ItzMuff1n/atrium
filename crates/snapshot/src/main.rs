//! `atrium-snapshot` — the Phase 2c command line.
//!
//! Exit codes: `0` success, `1` refusal, `2` usage error.
//!
//! ```
//! atrium-snapshot snapshot create  --root <dir> --store <dir> --label <name>
//! atrium-snapshot snapshot list    --store <dir>
//! atrium-snapshot snapshot restore --root <dir> --store <dir> --id <name>
//! atrium-snapshot demo
//! atrium-snapshot fixtures --root <dir> [--outside <dir>]
//! ```
//!
//! **This tool prints real host paths in its refusals**, unlike `atrium-fileops`
//! and `atrium-shell`, which are required to avoid naming the sandbox's real
//! location. The difference is deliberate: those two are driven by an agent and
//! their output can reach it; this one is driven by the user and its output must
//! tell them *which directory* was refused
//! (`DECISIONS.md` — "Snapshots are a plain copy, not copy-on-write").

use std::path::PathBuf;
use std::process::ExitCode;

use atrium_snapshot::{create, demo, list, restore, Label, SnapshotError};

const USAGE: &str = "\
atrium-snapshot — snapshot and restore of the environment root (Phase 2c)

USAGE:
  atrium-snapshot snapshot create  --root <dir> --store <dir> --label <name>
  atrium-snapshot snapshot list    --store <dir>
  atrium-snapshot snapshot restore --root <dir> --store <dir> --id <name>
  atrium-snapshot demo
  atrium-snapshot fixtures --root <dir> [--outside <dir>]

EXIT CODES:
  0  success
  1  refusal (the request was understood and declined; nothing was written)
  2  usage error (the arguments are wrong)

The snapshot store must be OUTSIDE the environment root. A store inside the root
is refused: a command running inside the environment could delete it, and a save
point the boss can delete is not a save point.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            // The refusal, on stderr, with its own specific reason.
            eprintln!("atrium-snapshot: {e}");
            ExitCode::from(e.exit_code() as u8)
        }
    }
}

fn run(args: &[String]) -> Result<u8, SnapshotError> {
    let mut it = args.iter();
    let first = match it.next() {
        Some(a) => a.as_str(),
        None => {
            eprintln!("{USAGE}");
            return Ok(2);
        }
    };

    match first {
        "snapshot" => {
            let sub = it.next().ok_or(SnapshotError::BadUsage {
                detail: "snapshot needs create, list or restore".to_string(),
            })?;
            match sub.as_str() {
                "create" => cmd_create(&args[2..]).map(|_| 0),
                "list" => cmd_list(&args[2..]).map(|_| 0),
                "restore" => cmd_restore(&args[2..]).map(|_| 0),
                other => Err(SnapshotError::BadUsage {
                    detail: format!(
                        "unknown snapshot subcommand {other:?}; expected create, list or restore"
                    ),
                }),
            }
        }
        "demo" => demo().map(|_| 0),
        "fixtures" => cmd_fixtures(&args[1..]).map(|_| 0),
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(0)
        }
        other => Err(SnapshotError::BadUsage {
            detail: format!("unknown command {other:?}; run with --help"),
        }),
    }
}

/// The flags this CLI understands, parsed by hand: no argument-parsing crate is
/// allowed (the project is std-only plus path references to its own crates).
struct Flags {
    map: Vec<(String, String)>,
}

impl Flags {
    fn parse(args: &[String], allowed: &[&str]) -> Result<Flags, SnapshotError> {
        let mut map = Vec::new();
        let mut i = 0;
        while i < args.len() {
            let a = &args[i];
            if !a.starts_with("--") {
                return Err(SnapshotError::BadUsage {
                    detail: format!("unexpected argument {a:?}; expected a flag"),
                });
            }
            let name = a.trim_start_matches("--").to_string();
            if !allowed.contains(&name.as_str()) {
                return Err(SnapshotError::BadUsage {
                    detail: format!(
                        "unknown flag --{name}; this command takes {}",
                        allowed
                            .iter()
                            .map(|s| format!("--{s}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                });
            }
            let value = args.get(i + 1).ok_or_else(|| SnapshotError::BadUsage {
                detail: format!("--{name} needs a value"),
            })?;
            if value.starts_with("--") {
                return Err(SnapshotError::BadUsage {
                    detail: format!("--{name} needs a value, but was followed by {value:?}"),
                });
            }
            map.push((name, value.clone()));
            i += 2;
        }
        Ok(Flags { map })
    }

    fn get(&self, name: &str) -> Option<&str> {
        self.map
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    fn require(&self, name: &str) -> Result<&str, SnapshotError> {
        self.get(name).ok_or(SnapshotError::MissingFlag {
            flag: match name {
                "root" => "--root",
                "store" => "--store",
                "label" => "--label",
                "id" => "--id",
                _ => "--<flag>",
            },
        })
    }
}

fn cmd_create(args: &[String]) -> Result<(), SnapshotError> {
    let f = Flags::parse(args, &["root", "store", "label"])?;
    let root = PathBuf::from(f.require("root")?);
    let store = PathBuf::from(f.require("store")?);
    let label = Label::parse(f.require("label")?)?;

    create(&root, &store, &label)?;
    println!("snapshot created: {label}");
    Ok(())
}

fn cmd_list(args: &[String]) -> Result<(), SnapshotError> {
    let f = Flags::parse(args, &["store"])?;
    let store = PathBuf::from(f.require("store")?);

    let found = list(&store)?;
    if found.is_empty() {
        // Not an error: an empty store is a normal state (attack-list-2c.md §J.7).
        println!("no snapshots");
        return Ok(());
    }
    println!(
        "{:<24} {:>6} {:>5} {:>8}  ORIGIN ROOT",
        "LABEL", "FILES", "DIRS", "SYMLINKS"
    );
    for s in found {
        println!(
            "{:<24} {:>6} {:>5} {:>8}  {}",
            s.label, s.files, s.dirs, s.symlinks, s.origin_root
        );
    }
    Ok(())
}

fn cmd_restore(args: &[String]) -> Result<(), SnapshotError> {
    let f = Flags::parse(args, &["root", "store", "id"])?;
    let root = PathBuf::from(f.require("root")?);
    let store = PathBuf::from(f.require("store")?);
    let label = Label::parse(f.require("id")?)?;

    restore(&root, &store, &label)?;
    println!("restored: {label}");
    Ok(())
}

fn cmd_fixtures(args: &[String]) -> Result<(), SnapshotError> {
    let f = Flags::parse(args, &["root", "outside"])?;
    let root = PathBuf::from(f.require("root")?);
    // The outside directory is where `link-escape` points. It defaults to a
    // sibling of the root's name so the harness can pass it explicitly; the
    // important property is that it is a directory the harness owns and not
    // `/etc`, so a followed symlink is both harmless and *visible*.
    let outside = match f.get("outside") {
        Some(o) => PathBuf::from(o),
        None => {
            let mut p = root.clone();
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "atrium-2c".to_string());
            p.set_file_name(format!("{name}-outside"));
            p
        }
    };

    atrium_snapshot::build_fixtures(&root, &outside)?;
    println!("fixtures built");
    println!("  root:    {}", root.display());
    println!("  outside: {}", outside.display());
    Ok(())
}
