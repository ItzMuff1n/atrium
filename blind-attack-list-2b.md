# Attack List: Real Command Execution Inside a Virtualized Environment

Scope under test (as declared): the runner controls (a) where a command starts — cwd resolved through the path resolver, escapes refused; (b) what it's handed — scrubbed environment, no inherited stdin; (c) capture of stdout/stderr and exit/kill/timeout status. It explicitly does NOT claim OS-level sandboxing of what the command can then reach.

Notation: write attempts use whatever the real environment folder maps to, but attacks assume we only know virtual paths like `/home/documents`.

## 1. Working-directory escapes and lies

1. Request cwd `/home/../..` or `/home/documents/../../` — expect refusal or normalization to inside the env; wrong if the command starts outside the env folder (check via `pwd` output).
2. Request cwd `/` — expect refusal; wrong if it starts at the real filesystem root.
3. Request cwd that resolves through a symlink *inside* the env pointing to a real outside path (e.g. env contains `link -> /etc`), cwd = `/home/link` — resolver must reject or resolve the real target and reject; wrong if the command runs in real `/etc`.
4. Pre-create a symlink inside the env to another path inside the env, use it as cwd — expect it works and `pwd` shows a coherent, env-consistent path; shows a leak if `pwd -P` prints the real host path of the env folder.
5. cwd given as a file, not a directory (`/home/documents/report.txt`) — expect clean refusal; watch for a crash or for silently running in the parent dir.
6. cwd path with trailing slash, double slashes, `.` components, or NUL-like edge characters (`/home//documents/./`) — expect consistent resolution, no bypass of the outside-check via weird spelling.
7. cwd spelled with different case on a case-insensitive host FS, or with Unicode lookalikes of `..` (e.g. `‥`, fullwidth dots) — resolver must not be fooled into a different real location.
8. Race: pass cwd inside the env, then between resolution and spawn swap the directory for a symlink to outside (TOCTOU) — the check and the chdir must use the same resolved fd/path; success for attacker if the process starts outside.
9. cwd is a directory that gets deleted/recreated by a prior command — expect either refusal or correct behavior on the new directory, never fallback to host cwd.
10. Relative cwd or empty cwd string — expect explicit defaulting to env root (or refusal), never the host user's home or the app's own cwd.
11. cwd passed as an absolute *host* path that happens to exist both places (e.g. `/tmp`) — expect it interpreted as the env's `/tmp`; wrong if it lands in the real `/tmp`.
12. In-command escape confirmation test: run `pwd; ls /` as the command with a legal cwd — the *command itself* can see the real FS (out of the narrow promise's scope, but record that `ls /` revealing the real root is expected per the stated design, and flag it if the docs imply otherwise).

## 2. Environment variable scrubbing

13. Run `env` (or `printenv`) — expect none of `AWS_SECRET_ACCESS_KEY`, `GITHUB_TOKEN`, `SSH_AUTH_SOCK`, `HOME` (host), `USER` (host), `PATH` (host), `GPG`/`DBUS`/`XDG` secrets; wrong if any host value appears.
14. Check specifically for `LD_PRELOAD`, `LD_LIBRARY_PATH`, `DYLD_*`, `PYTHONPATH`, `NODE_OPTIONS`, `BASH_ENV`, `ENV` — these should not pass through from host; also verify the scrub didn't *add* them.
15. Over-emptiness test: run `git --version`, `python3 -c "import os; os.getcwd()"`, `sh -c 'echo ok'` with the scrubbed env — if `PATH` is empty or minimal, ordinary commands may fail to spawn; determine whether that's intended (spec silent — flag it).
16. HOME-less behavior: run `git config --list` or `ssh -V` — many tools need `HOME`; wrong if it crashes *and* wrong if HOME silently points at the host home (leak).
17. Secret-in-content leak: put a token-looking value in an allowed env var if the API allows caller-supplied vars, then confirm it round-trips and nothing else joins it.
18. Env var with weird name/value (newline in value, `=` in name, non-UTF8 bytes, empty name) — expect spawn to refuse or pass through unchanged; watch for a panic in the runner.

## 3. stdin handling

19. Run `cat` with no arguments — expect it to exit immediately on EOF (stdin closed/null) or be refused; wrong if it hangs forever waiting on the user's terminal.
20. Run a command that reads a password prompt (`read -s x` in sh) — must not block on or leak the real TTY; also must not echo user keystrokes later typed into the app.
21. Run `python3 -c "import sys; print(sys.stdin.isatty())"` — record whether stdin is null/pipe; a TTY here is a red flag.
22. Command that reads stdin and writes it back, with runner feeding known bytes if there's an API to pass stdin — verify byte-exact round trip (or confirm stdin input is unsupported and cleanly EOF'd).

## 4. Output capture correctness

23. Binary output: `head -c 1024 /dev/urandom` to stdout — expect the capture to survive as bytes/base64 without UTF-8 mangling; wrong if invalid bytes corrupt the response, are silently dropped, or crash the runner.
24. Interleaving: a program printing alternately 1 byte to stdout and 1 stderr 10,000 times — if the API claims ordering, check the merged order; the streams must not be swapped (stderr text appearing in the stdout field).
25. Partial line without trailing newline (`printf 'no-newline'`) — must be captured fully; watch for last-chunk loss.
26. CR/terminal control sequences: output containing `\r`, ANSI colors, `\b`, NUL bytes — capture should be preserved or explicitly documented as sanitized; wrong if control bytes can forge the *runner's own* status text appended to output.
27. Huge stdout (e.g. `yes | head -c 100M`): cap must trigger; the response must truthfully indicate truncation and keep the command kill/killed status accurate; wrong if truncation is hidden or memory blows up.
28. Flood exactly at the cap boundary (cap, cap-1, cap+1 bytes) — check off-by-one and truncation flag.
29. Slow drip: program emitting 1 byte/second for longer than the time limit — timeout must still fire; output collected so far should be returned or explicitly discarded, consistently documented.
30. Stdout closed early (`exec 1>&-; echo to-stderr-only`) and stderr closed (`exec 2>&-`) — the runner must not hang or misreport.

## 5. Exit status / kill / timeout truthfulness

31. `exit 42`, `exit 0`, `exit 255` — codes must be reported exactly; `exit 256`-style wrap and negative codes checked.
32. Death by signal: `kill -SEGV $$` / `kill -KILL $$` from inside the command — must be reported as signal/killed with the signal number, never as exit 0 or as an ambiguous "error".
33. Timeout: `sleep 999` with a short limit — reported state must be an explicit *timeout*, distinguishable from a normal exit; verify the process is actually dead afterward (`ps`).
34. Timeout on a process ignoring SIGTERM (`trap '' TERM; sleep 999`) — runner must escalate to SIGKILL and still report timeout; check no zombie remains.
35. Instant-kill race: command that exits 0 in <1ms right at spawn — status must be exit 0, not timeout/spawn error.
36. Command that forks a daemon (`nohup sleep 600 & disown` or double-fork) then exits 0 — does the runner report exit 0 while a real process survives outside? Spec is silent on process-group cleanup; either behavior is defensible, but the survivor must be *documented*, otherwise it's a hole: repeated runs leak processes/memory on the host.
37. Command spawning children that hold stdout/stderr open after parent exits (`(sleep 30) &`) — runner must not hang waiting on pipe EOF past the timeout, and must not report "finished" while silently still waiting.
38. Zombie reaping: run many short commands; `ps` for zombie buildup — runner must reap children.

## 6. Refusal/success must not lie

39. Refused cwd (outside path): confirm *nothing* ran — e.g. attempt `touch /home/documents/sentinel` with an outside cwd; sentinel must not exist afterward.
40. Success claim: run `touch /home/documents/x` reported as exit 0 — verify file exists in the env (and only there, not host `/home/documents`).
41. Reported failure with side effect: command `touch x; exit 1` — file should exist and status 1 reported; wrong if failure report implies nothing happened but file exists, or status 0 with no file.
42. Spawn failure paths: nonexistent binary, non-executable file as command, directory as command — each must produce a distinct, truthful spawn-error result, not exit 127 confusion or a silent hang.

## 7. Paths, arguments, quoting

43. Is the command run via a shell or execvp? Test: command `echo $HOME` and `echo a | cat` — record which interpretation occurs; wrong if the docs say one and behavior shows the other (shell injection surface or broken pipelines).
44. Argument with spaces: `touch` on filename `a b.txt` — file must appear with the space intact, not two files.
45. Argument with `$`, backticks, `$(...)`, `*`, `;`, `&&`, `|`, newline — verify documented shell/non-shell behavior; injection is wrong if exec semantics were promised, failing to expand is wrong if shell semantics promised.
46. Argument beginning with `-` (e.g. `--help`, `-rf`) — passed through to the program, not swallowed by the runner's own arg parsing.
47. Empty-string argument, and zero arguments — must reach the program correctly (argv count check via a small script).
48. Non-UTF8 bytes in an argument and in the command name — must not panic the runner; behavior documented.
49. Argument that is an absolute *host* path (`/etc/passwd`): command `cat /etc/passwd` will succeed because commands are unsandboxed — confirm the response/docs don't claim arguments are resolved (narrow promise covers cwd only); flag if docs imply otherwise.
50. Very long argument (100KB+) and very long command line total — expect a clean error or pass-through, not truncation or crash.
51. Command name with a path: `./script.sh`, `/home/bin/tool`, `../tool` — which relative base is used must match the resolved cwd; `..` must be confined or fail.

## 8. Information leaked through reported output

52. Run `pwd -P`, `readlink /proc/self/cwd`, `mount`, `df .` — if the runner's response reveals the real host path of the env folder (in output it injects, wrappers, or error messages), that's a hole in the "agent never learns where the folder really sits" claim. (Command's *own* output doing it is the command's nature; the *runner* must not add it.)
53. Error messages from the runner (spawn failure, refusal) must use virtual paths, not host paths.
54. Timing/truncation metadata in responses must not reveal host PID, host uptime, host tmp paths (`/tmp/atrium-xyz/...`).

## 9. Concurrency, repeatability, mutable cwd

55. Two runs at once writing the same file (`sh -c 'for i in $(seq 1000); do echo a; done > f'` x2) — observe interleaving/corruption is possible; is that accepted? Spec silent — flag.
56. Same command run twice with identical inputs — results should be identical; nondeterministic status/captures indicate a race.
57. Run N commands concurrently until fd exhaustion (`ulimit -n` probes) — runner must fail cleanly, not wedge or start mis-reporting statuses.
58. cwd deleted by command A while command B is starting in it — B must fail cleanly or run in a stable fd-based cwd, never in a wrong directory.
59. Output cap shared or per-run? Two concurrent flooders — verify each gets its truthful truncated output, no cross-contamination of streams between runs.

## 10. Cross-command / system interference

60. Command A `kill`s PIDs of command B (same user, unsandboxed — easy: `pkill -f sleep` while another run sleeps) — B's reported status must truthfully show signal death caused externally, not "finished"; accepted that A *can* do this, but the status report must not lie.
61. Resource hog: one command consuming all CPU/RAM (`:(){ :|:&};:` fork bomb or `dd` of huge file) — runner and host app must remain responsive; documented limits if any. Probably out of the narrow promise — flag as known-unmitigated.
62. Command writing into the env folder's own metadata/config areas (the app's state files for the env) — can a command corrupt the virtualization layer itself? Test read/write of any dotfiles the app keeps in the env root.

## Limits of this list

- I have not seen the code, the API surface (whether caller can pass env/stdin/shell flag), or the platform (Linux assumed; macOS/Windows items like `DYLD_*`, case-insensitivity would shift).
- Items where scope is genuinely ambiguous versus the stated narrow promise: PATH/HOME minimalism (15–16), daemon survival (36), concurrency semantics (55–59), resource exhaustion (61), and host-absolute-path arguments (49). These may be explicitly out of scope for this piece; I flagged rather than assumed.
- I may be wrong that any output-ordering guarantee exists (24) or that base64-safe binary return is required (23) — both depend on the response format I couldn't see.
- Signal-to-status mapping details (32) and zombie reaping (38) can't be fully judged without knowing the spawn/wait implementation.
