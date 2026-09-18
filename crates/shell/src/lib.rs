//! atrium-shell — Phase 2b: the real shell (`BUILD-PLAN.md` §2b).
//!
//! Runs a real command (`program`, `argv`) with:
//! - its working directory resolved from a `VirtualPath` through
//!   `atrium_resolver::resolve` — the only function allowed to turn a
//!   virtual path into a real one (`AGENT-RULES.md` §6, `DESIGN.md` §3.2);
//! - a fixed, non-inherited environment (the user's shell environment on
//!   this machine holds API keys — this is a security requirement,
//!   not tidiness);
//! - stdin not connected to the user's terminal (immediate EOF);
//! - a time limit (a safety stop so the runner cannot be hung — NOT the
//!   `DESIGN.md` §7.1 kill switch, which is Phase 4 and user-facing);
//! - a cap on captured output, reported as truncation, never silent;
//! - a three-way status: exit code / terminating signal / timeout. These
//!   are not the same thing and are never collapsed into one number. A
//!   command killed by SIGKILL reported as exit 0 is the worst single
//!   outcome this phase can produce (attack-list-2b.md §C.6).
//!
//! HONEST SCOPE — stated in the README too and never overstated:
//! 2b guarantees WHERE a command starts and WHAT it is handed. It does
//! NOT confine what the command can then reach. A real command can
//! `cd /` and act on the host. That is by design: the cage is
//! `BUILD-PLAN.md` §2e, a required later phase. attack-list-2b.md §G
//! deliberately demonstrates the hole. There is no cage, bubblewrap,
//! namespace, Landlock or container here, and adding one is out of scope.

use std::fmt;
use std::path::{Path, PathBuf};

use atrium_resolver::{resolve, ResolveError};

/// The default time limit: 30 seconds. A safety stop so the runner cannot
/// be hung (attack-list-2b.md §F.1). Settable via `RunOptions`.
pub const DEFAULT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// The default cap on each captured stream: 1 MiB. Past the cap the stream
/// is still drained (so the child cannot block on a full pipe) but only the
/// first `max_output_bytes` are kept and `truncated` is set — silent
/// truncation is a failure (attack-list-2b.md §F.3).
pub const DEFAULT_MAX_OUTPUT_BYTES: usize = 1024 * 1024;

/// How long the runner waits for its two reader threads to see EOF after the
/// command itself has ended, before it gives up on them and reports what it
/// has (attack-list-2b.md §N.8).
///
/// This exists because a command's *own children* can hold its output pipes
/// open after the command exits — `sh -c '(sleep 30) &'` is the smallest
/// example, and a build tool or daemon is the realistic one. Waiting for EOF
/// means waiting for the survivor's lifetime, which is not the command's
/// lifetime and must not be the runner's. Real and observed: without this
/// grace the runner blocked for the full 30 s of a grandchild's sleep while
/// its own 2 s limit had already expired.
///
/// The reader threads are not killed; they end on their own when the last
/// holder closes the pipe. Bytes a survivor writes after `run()` has returned
/// are not the command's output.
pub const READER_GRACE: std::time::Duration = std::time::Duration::from_millis(200);

/// The PATH handed to every child. Deliberately fixed and minimal:
/// the two standard binary directories, nothing from the user's session
/// (attack-list-2b.md §D.3).
pub const CHILD_PATH: &str = "/usr/bin:/bin";

/// A directory as the agent wrote it. Always starts with `/`.
///
/// Constructing one guarantees nothing about what the path points at; it
/// only fixes the spelling class. Containment is still decided by
/// `resolve()` inside `run()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualPath(String);

impl VirtualPath {
    /// Build a virtual path from the agent's spelling. Relative paths are
    /// refused here (and again inside `resolve()`, which remains the only
    /// containment oracle per `AGENT-RULES.md` §6).
    pub fn new(path: &str) -> Result<VirtualPath, RunError> {
        if path.is_empty() {
            return Err(RunError::Rejected(ResolveError::EmptyPath));
        }
        if !path.starts_with('/') {
            return Err(RunError::Rejected(ResolveError::RelativePath {
                input: path.to_string(),
            }));
        }
        Ok(VirtualPath(path.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for VirtualPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A path that has been through the resolver and is known to be inside the
/// environment root. The only thing `spawn` may be given as a cwd.
/// Its constructor is private to this crate: the only way to get one is
/// through `resolve()`, so forgetting to resolve is a compile error.
#[derive(Debug)]
pub struct RealDir(PathBuf);

impl RealDir {
    /// The single doorway from virtual to real. Called at the top of
    /// `run()`, before anything is spawned (attack-list-2b.md §B.9:
    /// a path that fails to resolve is REFUSED and no process starts).
    /// `resolve()` answers "is this inside the root", not "is this a
    /// directory" — the directory check is this phase's own and happens
    /// here, after resolving (brief §9.4).
    fn resolve(root: &Path, path: &VirtualPath) -> Result<RealDir, RunError> {
        match resolve(root, path.as_str()) {
            Ok(p) => Ok(RealDir(p)),
            Err(e) => Err(RunError::Rejected(e)),
        }
    }

    fn as_path(&self) -> &Path {
        &self.0
    }
}

/// How the command ended. Three distinct things, never collapsed into one
/// number (attack-list-2b.md §C.6, §F.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitStatus {
    /// The command exited on its own with this code.
    Exited(i32),
    /// The command was killed by this signal (e.g. 9 for SIGKILL).
    Signalled(i32),
    /// The command was still running at the time limit and was killed by
    /// the runner. Never an exit code, never left running.
    TimedOut,
}

impl fmt::Display for ExitStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExitStatus::Exited(code) => write!(f, "exit {}", code),
            ExitStatus::Signalled(sig) => write!(f, "killed by signal {}", sig),
            ExitStatus::TimedOut => write!(f, "timed out"),
        }
    }
}

/// What a finished command produced. Bytes are bytes: exact, unaltered,
/// no trimming, no newline-adding, no lossy UTF-8 conversion, no merging
/// (attack-list-2b.md §C).
#[derive(Debug)]
pub struct Outcome {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub status: ExitStatus,
    pub truncated_stdout: bool,
    pub truncated_stderr: bool,
}

/// The limits a run obeys. Both are settable; the defaults exist so a
/// caller that does not care gets a safety stop and a capped capture
/// anyway.
#[derive(Debug, Clone, Copy)]
pub struct RunOptions {
    pub timeout: std::time::Duration,
    pub max_output_bytes: usize,
}

impl Default for RunOptions {
    fn default() -> Self {
        RunOptions {
            timeout: DEFAULT_TIMEOUT,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
        }
    }
}

/// The one error type. Carries enough to name the specific failure: which
/// virtual path, which step, which reason. Every variant names what was
/// wrong — no variant says "denied" bare. Resolver rejections are wrapped,
/// never flattened.
#[derive(Debug)]
pub enum RunError {
    /// The resolver rejected the cwd; its own reason passes through.
    Rejected(ResolveError),
    /// The cwd resolved inside the root but is not there: a command cannot
    /// start somewhere that does not exist (attack-list-2b.md §B.2).
    CwdMissing { path: String },
    /// The cwd resolved to a file, not a directory (attack-list-2b.md
    /// §B.3/§B.4 — this phase's own check, not the resolver's).
    CwdNotADirectory { path: String },
    /// The operating system refused to start the program. Names the
    /// program the caller asked for and the OS's own words.
    Spawn { program: String, reason: String },
    /// The runner's own machinery failed (a reader thread could not be
    /// joined). Names which step.
    Internal { step: &'static str, reason: String },
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunError::Rejected(e) => {
                // The resolver's own `Display` text names the offending step,
                // but for these variants it names where that step *reached* —
                // a real host path. Passing it through verbatim would disclose
                // the sandbox's location on disk (DESIGN.md §3.1;
                // attack-list-2b.md §H.1). So those variants are re-worded
                // here, from the resolver's STRUCTURED fields, to name the
                // VIRTUAL path and drop the real one. Every other variant is
                // passed through intact: its text is already virtual-path-only.
                // (Pattern copied from fileops/src/lib.rs, impl Display for
                // FileOpError — a copyable pattern, not a copyable file.)
                //
                // STANDING CONSEQUENCE: if the resolver ever grows another
                // variant whose reason names a real path, this re-wording
                // MUST be updated at the same time.
                match e {
                    ResolveError::TraversalAboveRoot { component, .. } => write!(
                        f,
                        "Rejected: the `..` step `{}` climbed above the environment root (that step left the environment; where it reached on disk is not disclosed)",
                        component
                    ),
                    ResolveError::EscapesRoot { input, .. } => write!(
                        f,
                        "Rejected: `{}` escapes the environment root (a step left the environment; where it reached on disk is not disclosed)",
                        input
                    ),
                    ResolveError::SymlinkEscapes { link, .. } => write!(
                        f,
                        "Rejected: symlink `{}` points outside the environment root (its target is outside the environment, and its location is not disclosed)",
                        link
                    ),
                    other => write!(f, "{}", other),
                }
            }
            RunError::CwdMissing { path } => write!(
                f,
                "refused: `{}` does not exist inside the environment; a command cannot start in a directory that is not there",
                path
            ),
            RunError::CwdNotADirectory { path } => write!(
                f,
                "refused: `{}` is a file, not a directory; a command can only start in a directory",
                path
            ),
            RunError::Spawn { program, reason } => write!(
                f,
                "refused: the operating system could not start `{}` ({})",
                program, reason
            ),
            RunError::Internal { step, reason } => {
                write!(f, "runner failure while {} ({})", step, reason)
            }
        }
    }
}

impl std::error::Error for RunError {}

/// Run a real command.
///
/// - `root` is the real host path of the environment root (must exist);
/// - `cwd` is the virtual path the command starts in — resolved through
///   `resolve()` before ANYTHING is spawned; a path that fails to resolve
///   is refused and no process starts (attack-list-2b.md §B.9);
/// - `program` and `args` are passed through UNTOUCHED — no shell parsing,
///   no splitting, no quoting, no resolving. They are not paths; a line of
///   shell is the caller's business (`sh -c '<line>'`), and what the
///   command then does with its own arguments is the command's business
///   (attack-list-2b.md §C.10, §J.7; brief §9.3 requirement 8);
/// - `opts` carries the time limit and the output cap.
///
/// The child gets an EMPTY environment with a small fixed set on top
/// (`PATH`, `HOME`, `TMPDIR` — the last two pointing at the environment
/// root), never the parent's environment. Its stdin is /dev/null, so a
/// command that reads stdin sees EOF immediately. stdout and stderr are
/// captured separately, each on its own reader thread, so a command
/// flooding one stream while the other is full cannot deadlock the runner
/// (attack-list-2b.md §F.5). On timeout the child is killed and then
/// reaped — no zombies, no orphans (§F.2).
///
/// Boring and obvious by deliberate choice: spawn, read both streams on
/// their own threads, poll for exit, kill and reap on timeout. No clever.
pub fn run(
    root: &Path,
    cwd: &VirtualPath,
    program: &str,
    args: &[String],
    opts: &RunOptions,
) -> Result<Outcome, RunError> {
    use std::process::{Command, Stdio};

    // 1. Resolve the cwd BEFORE anything is spawned. A refusal here means
    //    no process starts at all.
    let real = RealDir::resolve(root, cwd)?;
    // 2. The directory check is this phase's own (brief §9.4): `resolve()`
    //    answers containment, not kind. The path may be absent (accepted by
    //    the resolver) or a file.
    match std::fs::symlink_metadata(real.as_path()) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => {
            return Err(RunError::CwdNotADirectory {
                path: cwd.as_str().to_string(),
            })
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(RunError::CwdMissing {
                path: cwd.as_str().to_string(),
            })
        }
        Err(e) => {
            return Err(RunError::Internal {
                step: "checking the resolved working directory",
                reason: e.to_string(),
            })
        }
    }

    // 3. Spawn: fixed non-inherited environment, stdin at EOF, both
    //    output streams piped.
    let mut cmd = Command::new(program);
    cmd.args(args)
        .current_dir(real.as_path())
        .env_clear()
        .env("PATH", CHILD_PATH)
        .env("HOME", root)
        .env("TMPDIR", root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| RunError::Spawn {
        program: program.to_string(),
        reason: e.to_string(),
    })?;

    // 4. One reader thread per stream, from the moment the pipes exist, so
    //    neither pipe can fill while we are looking at the other (§F.5).
    //    Each drains the stream to EOF, keeping up to the cap and flagging
    //    truncation. (child.stdout/stderr are Some: we piped them above.)
    let mut child_stdout = child.stdout.take().expect("stdout was piped");
    let mut child_stderr = child.stderr.take().expect("stderr was piped");
    let cap = opts.max_output_bytes;
    let out_handle = std::thread::spawn(move || read_capped(&mut child_stdout, cap));
    let err_handle = std::thread::spawn(move || read_capped(&mut child_stderr, cap));

    // 5. Poll for exit. Boring: try_wait, sleep a slice, repeat until the
    //    deadline.
    let deadline = std::time::Instant::now() + opts.timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => {
                break match s.code() {
                    Some(code) => ExitStatus::Exited(code),
                    None => {
                        #[cfg(unix)]
                        {
                            use std::os::unix::process::ExitStatusExt;
                            match s.signal() {
                                Some(sig) => ExitStatus::Signalled(sig),
                                // A wait-status with neither code nor signal does
                                // not occur on unix, and this is now enforced
                                // rather than assumed. `try_wait` waits with
                                // WNOHANG and no WUNTRACED/WCONTINUED, so a
                                // stopped or continued child is never reported
                                // here -- probed 19 Sep 2026: a SIGSTOPped child
                                // gave `Ok(None)`, not a status. Every status that
                                // IS reported therefore carries a code or a
                                // signal. The previous fallback here returned
                                // `Exited(-1)`, a fabricated exit code that no
                                // process can produce and that `Display` would
                                // print as "exit -1" -- a lie in the one place
                                // (attack-list-2b.md §C.6) the whole point is
                                // telling a crash from a success. Unreachable is
                                // the honest answer.
                                None => unreachable!(
                                    "a wait-status on unix carries an exit code or a signal"
                                ),
                            }
                        }
                        #[cfg(not(unix))]
                        {
                            // Unreachable for the same reason, on a platform this
                            // project does not build or test: all CI runs on
                            // ubuntu-latest, and the test target uses
                            // `std::os::unix` unconditionally. Kept as a value
                            // rather than `unreachable!()` because this arm is not
                            // compiled here and so cannot be verified by running
                            // anything -- see `.cargo/mutants.toml`.
                            ExitStatus::Exited(-1)
                        }
                    }
                };
            }
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    // §F.1/§F.2: kill, then reap below — never left running.
                    let _ = child.kill();
                    break ExitStatus::TimedOut;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Err(e) => {
                return Err(RunError::Internal {
                    step: "waiting on the child",
                    reason: e.to_string(),
                })
            }
        }
    };

    // 6. Always reap. After a normal exit the status was already collected
    //    by try_wait; after a timeout this collects the killed child so no
    //    zombie or orphan is left.
    if status == ExitStatus::TimedOut {
        let _ = child.wait();
    }

    // 7. Collect both streams, but NEVER block on them past the child's own
    //    end. This is attack-list-2b.md §N.8, and the defect it catches was
    //    real and observed: a command that backgrounds something which
    //    inherits these pipes (`sh -c '(sleep 30) &'`) returns immediately,
    //    but the grandchild holds the write ends open, so a plain
    //    `join()` waits for the *grandchild* to finish — far past the time
    //    limit, for as long as the daemon lives. The runner is about the
    //    command, so it waits a short grace period for the readers to see
    //    EOF, and then reports what was captured and moves on. Bytes the
    //    grandchild would have written are not the command's output.
    //
    //    `is_finished()` first, with no wait at all, so the ordinary case
    //    (pipes already at EOF because the child exited) pays nothing.
    let mut out_handle = Some(out_handle);
    let mut err_handle = Some(err_handle);
    let grace = std::time::Instant::now() + READER_GRACE;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut truncated_stdout = false;
    let mut truncated_stderr = false;
    loop {
        if let Some(h) = out_handle.as_ref() {
            if h.is_finished() {
                let (b, t) = out_handle
                    .take()
                    .unwrap()
                    .join()
                    .map_err(|_| RunError::Internal {
                        step: "joining the stdout reader",
                        reason: "reader thread panicked".to_string(),
                    })?;
                stdout = b;
                truncated_stdout = t;
            }
        }
        if let Some(h) = err_handle.as_ref() {
            if h.is_finished() {
                let (b, t) = err_handle
                    .take()
                    .unwrap()
                    .join()
                    .map_err(|_| RunError::Internal {
                        step: "joining the stderr reader",
                        reason: "reader thread panicked".to_string(),
                    })?;
                stderr = b;
                truncated_stderr = t;
            }
        }
        if out_handle.is_none() && err_handle.is_none() {
            break;
        }
        if std::time::Instant::now() >= grace {
            // Someone still holds a pipe open — a survivor, not the command.
            // Report what has been captured; the readers are detached and
            // will end on their own when the last holder closes. §N.9: the
            // survivor is not killed, and that is recorded in README.md.
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }

    Ok(Outcome {
        stdout,
        stderr,
        status,
        truncated_stdout,
        truncated_stderr,
    })
}

/// Drain a stream to EOF, keeping the first `cap` bytes and flagging
/// truncation. The stream is ALWAYS drained to EOF — a child that has hit
/// the cap must still be able to write and exit, otherwise the cap itself
/// becomes a deadlock (attack-list-2b.md §F.3–F.6).
fn read_capped<R: std::io::Read>(r: &mut R, cap: usize) -> (Vec<u8>, bool) {
    let mut kept: Vec<u8> = Vec::new();
    let mut truncated = false;
    let mut buf = [0u8; 8192];
    loop {
        match r.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if kept.len() < cap {
                    let room = cap - kept.len();
                    kept.extend_from_slice(&buf[..n.min(room)]);
                    if n > room {
                        truncated = true;
                    }
                } else {
                    truncated = true;
                }
            }
            Err(_) => break,
        }
    }
    (kept, truncated)
}
