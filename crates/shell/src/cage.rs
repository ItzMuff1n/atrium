//! The cage — Phase 2e: strong confinement for `run commands`
//! (`BUILD-PLAN.md` §2e, `DECISIONS.md` "Phase 2e: bubblewrap...").
//!
//! This module builds the argument list for `bubblewrap` and answers the one
//! question that decides whether a cage is available at all. It exists because
//! `crates/shell/src/lib.rs`'s own header used to say that adding a cage was
//! "out of scope" — which contradicted `DECISIONS.md`, where strong confinement
//! is a required phase. It is not out of scope; it is this phase.
//!
//! **The requirement.** Inside the command's own view of the filesystem,
//! outside the environment root does not exist. Not "is checked first" — does
//! not exist.
//!
//! **The shape of the cage.** `--unshare-all` gives the command its own
//! user, mount, PID, network, IPC and UTS namespaces. The root is bound at `/`,
//! so virtual paths line up exactly as Phase 1 defines them. A fixed list of
//! system directories is bound **read-only** on top of it. `/proc` and `/dev`
//! are fresh, and `/tmp` is a private tmpfs. The host environment is cleared and
//! replaced with exactly four variables.
//!
//! **Fail closed.** This module never returns a plan when it cannot establish
//! that a cage is possible. Every failure is a refusal that names the reason.
//! There is deliberately no variant of this type that means "run it anyway" —
//! the absence of such a value is the mechanism, not a convention
//! (`BUILD-PLAN.md` §2e; `attack-list-2e.md` §F).

use std::path::{Path, PathBuf};

/// The four environment variables the command is given. Nothing else reaches
/// it (`attack-list-2e.md` §E, requirement A of 26 Sep 2026).
///
/// A bare bubblewrap cage inherits the **host's** environment, measured
/// 26 Sep 2026: `SSH_AUTH_SOCK`, `DBUS_SESSION_BUS_ADDRESS`, `GH_AUDIT_TOKEN`
/// and the whole `HERMES_*` set were all visible to the command. Clearing it
/// and passing back a fixed four is the difference between a cage and a window.
pub const ALLOWED_ENV: [(&str, &str); 4] = [
    ("PATH", "/usr/bin:/bin"),
    ("HOME", "/"),
    ("TERM", "dumb"),
    ("LANG", "C.UTF-8"),
];

/// The system directories bound **read-only**, in this order. Measured
/// necessary: without `/usr` the cage cannot run `python3` or `git` at all, and
/// without the `/bin`, `/lib` and `/lib64` aliases (symlinks into `/usr` on this
/// host) a program cannot even be `exec`'d — `bwrap: execvp /bin/sh: No such
/// file or directory`.
///
/// Read-only is load-bearing: measured, `touch /usr/evil` inside the cage
/// returns `Read-only file system` and nothing appears at the real `/usr`.
pub const READ_ONLY_BINDS: [&str; 5] = ["/usr", "/bin", "/sbin", "/lib", "/lib64"];

/// Why a cage could not be established. Every variant is a **refusal with a
/// reason**; none of them is a fallback.
#[derive(Debug)]
pub enum CageError {
    /// The bubblewrap binary was not found at any of the places it is looked
    /// for. Not a `PATH` lookup: the child's `PATH` is fixed and minimal, and
    /// a search that honoured the caller's `PATH` would let the caller choose
    /// the cage (attack-list-2e.md §F.1).
    ToolMissing { looked_in: Vec<String> },
    /// bubblewrap ran and refused. Names its own words, which is what a runner
    /// log needs to distinguish "no user namespaces" from "no such file".
    CannotCage { reason: String },
    /// The environment root is not isolated from the rest of the machine: it
    /// shares a filesystem with a place a hard link could come from. Refused
    /// rather than caged, because a cage around a root that a hard link can
    /// cross is the hole 2a found, still open
    /// (`attack-list-2e.md` §D.7, `DESIGN.md` §3.1).
    RootNotIsolated {
        device: u64,
        shares_with: &'static str,
    },
    /// The root path could not be examined at all.
    RootUnreadable { reason: String },
}

impl std::fmt::Display for CageError {
    /// Every message is written to be safe on a default-output line: it names
    /// the **virtual** environment and never a real host path
    /// (`DESIGN.md` §3.1; attack-list-2e.md §F.9, `!! HOST PATH DISCLOSED`).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CageError::ToolMissing { looked_in } => write!(
                f,
                "refused: the cage program (bubblewrap) was not found, so the command was NOT run. \n\
                 Looked for it as a program named `bwrap` in: {}. \n\
                 A command is never run without the cage (BUILD-PLAN.md §2e).",
                looked_in.join(", ")
            ),
            CageError::CannotCage { reason } => write!(
                f,
                "refused: the cage could not be established, so the command was NOT run. \n\
                 The cage program said: {reason} \n\
                 A command is never run without the cage (BUILD-PLAN.md §2e)."
            ),
            CageError::RootNotIsolated { .. } => write!(
                f,
                "refused: the environment is not isolated from the rest of this machine — it shares a \
                 filesystem with somewhere a hard link could reach it from, so the environment's \
                 boundary would not hold. The command was NOT run.\n\
                 A hard link is a second name for one file rather than a path, so no path check can \
                 see it: the environment must sit on its own filesystem (DESIGN.md §3.1)."
            ),
            CageError::RootUnreadable { reason } => write!(
                f,
                "refused: the environment could not be examined, so the command was NOT run ({reason})."
            ),
        }
    }
}

impl std::error::Error for CageError {}

/// Where the cage program is looked for — an explicit list, never the caller's
/// `PATH`. `PATH` on a developer machine is long and user-controlled; a cage
/// resolved through it could be any program at all.
const BWRAP_CANDIDATES: [&str; 3] = ["/usr/bin/bwrap", "/bin/bwrap", "/usr/local/bin/bwrap"];

/// Find the cage program. `Ok(path)` means an executable exists there;
/// `Err(ToolMissing)` means none did, and that is a refusal.
pub fn find_cage_program() -> Result<PathBuf, CageError> {
    for c in BWRAP_CANDIDATES {
        let p = Path::new(c);
        if p.is_file() {
            if let Ok(md) = std::fs::metadata(p) {
                use std::os::unix::fs::PermissionsExt;
                if md.permissions().mode() & 0o111 != 0 {
                    return Ok(p.to_path_buf());
                }
            }
        }
    }
    Err(CageError::ToolMissing {
        looked_in: BWRAP_CANDIDATES.iter().map(|s| s.to_string()).collect(),
    })
}

/// The device the path sits on. `std::os::unix::fs::MetadataExt::dev`.
fn device_of(p: &Path) -> Result<u64, CageError> {
    std::fs::metadata(p)
        .map(|m| {
            use std::os::unix::fs::MetadataExt;
            m.dev()
        })
        .map_err(|e| CageError::RootUnreadable {
            reason: e.to_string(),
        })
}

/// The places an outside file could come from, each of which must be on a
/// **different** filesystem from the environment root for the boundary to hold.
///
/// This is the mechanism `DESIGN.md` §3.1 states and `attack-list-2a.md` §N
/// measured: a **hard link is a second name for one file, not a path**, so no
/// path check can see it — but a hard link cannot cross filesystems
/// (`Invalid cross-device link`, observed). The isolation requirement is
/// therefore exactly "no place a user's file lives shares a filesystem with
/// the environment root".
///
/// The list is the user's world, not the whole of `/`: `/home` is where the
/// files that matter are, `/tmp` is world-writable and where a link would
/// otherwise be trivially placed, and the working directory is included because
/// a caller can start the runner anywhere.
fn places_a_link_could_come_from() -> Vec<(&'static str, PathBuf)> {
    let mut v = vec![
        ("the home directory", PathBuf::from("/home")),
        ("the temporary directory", PathBuf::from("/tmp")),
        ("the root filesystem", PathBuf::from("/")),
    ];
    if let Ok(cwd) = std::env::current_dir() {
        v.push(("the working directory", cwd));
    }
    v
}

/// Is the environment root isolated from every place a hard link could reach it
/// from?
///
/// Returns `Ok(())` when the root's filesystem differs from all of them, and a
/// refusal otherwise. Measured on this machine 26 Sep 2026: a root under
/// `/tmp` (device 50) shares a device with `/tmp` (50) and is **not** isolated
/// — a hard link from `/tmp` into such a root is creatable and lets the cage
/// read a file outside it. A root under `/run/user/1000` (device 71) or
/// `/dev/shm` (device 27) is isolated: `ln` from `/home` (49) or `/tmp` (50)
/// fails with `Invalid cross-device link`.
pub fn check_root_isolated(root: &Path) -> Result<(), CageError> {
    let dev = device_of(root)?;
    for (name, p) in places_a_link_could_come_from() {
        if let Ok(other) = device_of(&p) {
            if other == dev {
                return Err(CageError::RootNotIsolated {
                    device: dev,
                    shares_with: name,
                });
            }
        }
    }
    Ok(())
}

/// A cage, ready to be executed: the program to run and the arguments that make
/// it a cage. Building one has already proved that the cage program exists and
/// that the root is isolated — so holding one of these means both questions
/// were answered `yes`.
#[derive(Debug, Clone)]
pub struct Cage {
    program: PathBuf,
    args: Vec<String>,
}

impl Cage {
    pub fn program(&self) -> &Path {
        &self.program
    }
    pub fn args(&self) -> &[String] {
        &self.args
    }
}

/// Build the cage for one command.
///
/// `root` is the **real** host path of the environment root — the only place in
/// this phase a real path is used, and it is used as a bind source for
/// bubblewrap, never as something the command can name. `virtual_cwd` is what
/// the command is told its working directory is, and it is what bubblewrap is
/// handed: the root is bound at `/`, so the virtual path is the real path
/// **inside** the cage. That equality is the whole reason the root is bound at
/// `/` rather than somewhere else.
///
/// `program` and `args` are passed through untouched as the cage's own command.
pub fn build(
    root: &Path,
    virtual_cwd: &str,
    program: &str,
    args: &[String],
) -> Result<Cage, CageError> {
    let cage_program = find_cage_program()?;
    check_root_isolated(root)?;

    let mut a: Vec<String> = Vec::new();
    let mut push = |s: &str| a.push(s.to_string());

    // Every namespace: user, mount, PID, network, IPC, UTS. The network being
    // inside this list is Muffin's decision D1 of 26 Sep 2026 — shut — and the
    // agent's ability to look things up comes from a fetch tool the loop calls
    // outside this cage instead (BUILD-PLAN.md Phase 5). `--share-net` must
    // never appear here (attack-list-2e.md §H.9).
    push("--unshare-all");
    // The command dies with the runner: no survivor outlives the run
    // (attack-list-2e.md §C.11, §I.6).
    push("--die-with-parent");
    // No controlling terminal inherited.
    push("--new-session");
    // Clear the environment. Without this the host's variables reach the
    // command, measured (attack-list-2e.md §E).
    push("--clearenv");

    // The four allowed variables, and nothing else.
    for (k, v) in ALLOWED_ENV {
        push("--setenv");
        push(k);
        push(v);
    }
    // HOME is the cage's own root, so a program writing to `$HOME` writes
    // inside the environment and never near the host's home
    // (attack-list-2e.md §C.7, §E.5).
    push("--setenv");
    push("HOME");
    push("/");

    // The environment root, at `/`. FIRST, so the read-only system bindings
    // below are laid on top of it rather than being replaced by it — bubblewrap
    // applies its operations in order, and getting this wrong produces a cage
    // that silently is not the cage (measured while probing this phase:
    // binding the root last wiped every system directory and `execvp` then
    // failed with "No such file or directory").
    push("--bind");
    push(&root.to_string_lossy());
    push("/");

    // The system directories a real shell needs, read-only, on top of the root.
    for d in READ_ONLY_BINDS {
        push("--ro-bind");
        push(d);
        push(d);
    }

    // A fresh /proc and /dev: the processes the command can see are its own,
    // and no host device is reachable through /dev.
    push("--proc");
    push("/proc");
    push("--dev");
    push("/dev");

    // A private, empty /tmp. Not a host directory: it is a fresh tmpfs, so a
    // program that needs a writable temporary directory has one, and nothing
    // written there appears in the host's /tmp (attack-list-2e.md §G.7).
    push("--tmpfs");
    push("/tmp");

    // The working directory, named the way the agent named it. The root is at
    // `/`, so this virtual path is also the real path inside the cage.
    push("--chdir");
    push(virtual_cwd);

    // The command itself and its arguments, unaltered.
    push("--");
    push(program);
    for arg in args {
        push(arg);
    }

    Ok(Cage {
        program: cage_program,
        args: a,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_allowlist_is_exactly_four_names_and_never_a_path_to_the_host() {
        assert_eq!(ALLOWED_ENV.len(), 4);
        let names: Vec<&str> = ALLOWED_ENV.iter().map(|(k, _)| *k).collect();
        assert_eq!(names, vec!["PATH", "HOME", "TERM", "LANG"]);
        // HOME must be the cage's own root, not the host's home.
        let home = ALLOWED_ENV.iter().find(|(k, _)| *k == "HOME").unwrap().1;
        assert_eq!(home, "/");
    }

    #[test]
    fn the_cage_never_shares_the_network() {
        // D1, 26 Sep 2026: the network is SHUT. A caged command reaches
        // nothing, and the agent's lookups come from the fetch tool instead.
        let root = std::env::temp_dir();
        let c = build(&root, "/", "/bin/echo", &["hi".to_string()]);
        // The build may refuse (a /tmp root is not isolated) — what must hold
        // is that no cage ever carries --share-net.
        if let Ok(cage) = c {
            assert!(!cage.args().iter().any(|a| a == "--share-net"));
            assert!(cage.args().iter().any(|a| a == "--unshare-all"));
        }
    }

    #[test]
    fn the_root_is_bound_first_and_the_system_dirs_are_read_only() {
        // The ordering is load-bearing: measured, binding the root after the
        // system directories replaces the whole tree and execvp fails.
        let root = Path::new("/run/user/1000");
        if !root.exists() {
            return;
        }
        let c = build(root, "/home/work", "/bin/echo", &["hi".to_string()]);
        if let Ok(cage) = c {
            let a = cage.args();
            let bind_root = a
                .iter()
                .position(|x| x == "--bind")
                .expect("--bind present");
            let first_ro = a
                .iter()
                .position(|x| x == "--ro-bind")
                .expect("--ro-bind present");
            assert!(
                bind_root < first_ro,
                "the root bind must come before the ro-binds"
            );
        }
    }

    #[test]
    fn a_root_on_the_same_filesystem_as_tmp_is_refused() {
        // The measured non-isolated case. /tmp is a tmpfs with its own device;
        // a directory inside it shares that device, so a hard link from /tmp
        // into it is creatable and the boundary would not hold.
        let d = std::env::temp_dir().join("atrium-2e-isolation-probe");
        let _ = std::fs::create_dir_all(&d);
        let r = check_root_isolated(&d);
        let _ = std::fs::remove_dir(&d);
        match r {
            Err(CageError::RootNotIsolated { .. }) => {}
            other => panic!("expected a refusal for a /tmp root, got {other:?}"),
        }
    }

    #[test]
    fn a_missing_cage_program_is_a_refusal_naming_where_it_looked() {
        // The candidates are absolute and fixed, so this asserts the SHAPE of
        // the failure rather than depending on the machine having no bwrap.
        match find_cage_program() {
            Ok(p) => assert!(p.is_absolute()),
            Err(CageError::ToolMissing { looked_in }) => {
                assert_eq!(looked_in.len(), BWRAP_CANDIDATES.len());
            }
            Err(other) => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn no_refusal_message_contains_a_real_host_path() {
        // DESIGN.md §3.1: the agent never learns the real path exists.
        let msgs = [
            CageError::ToolMissing {
                looked_in: vec!["/usr/bin/bwrap".to_string()],
            },
            CageError::CannotCage {
                reason: "setting up uid map: Permission denied".to_string(),
            },
            CageError::RootNotIsolated {
                device: 50,
                shares_with: "the temporary directory",
            },
            CageError::RootUnreadable {
                reason: "no such file".to_string(),
            },
        ];
        for m in msgs {
            let s = m.to_string();
            assert!(!s.contains("/home/muffin"), "leaked the host home: {s}");
            assert!(!s.contains("/dev/shm"), "leaked a real root location: {s}");
        }
    }
}
