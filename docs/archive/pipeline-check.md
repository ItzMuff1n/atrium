# pipeline-check.md — how work actually flows through this setup

**Date:** 2026-09-11, ~20:10 local (+0300)
**Method:** direct observation. Every claim below is labelled **[observed]** (I ran it and read the output) or **[inferred]** (reasoned from source code, not executed). Nothing here is taken from documentation.
**Scope:** read-only. No project file was modified. Two throwaway probe crates were created under `/tmp/Nexus project/` to test hook firing (see §7, artefacts).

---

## 1. config.yaml — model block and delegation block

`~/.hermes/config.yaml`, lines 1–4 and 144–149, verbatim:

```yaml
model:
  default: deepseek-v4.1-flash
  provider: ollama-cloud
  base_url: https://ollama.com/v1
```

```yaml
delegation:
  model: kimi-k3
  provider: ollama-cloud
  base_url: https://ollama.com/v1
  max_iterations: 50
  child_timeout_seconds: 900
```

No other `model:`/`delegation:` keys exist. The file is 268 lines / 7,068 bytes, `_config_version: 33`.

### Which model is serving THIS session

**`deepseek-v4.1-flash` on provider `ollama-cloud`.** Three independent observations:

1. **[observed]** Live API-call log lines, the most recent five:

```
2026-09-11 19:58:51,522 INFO [20260910_225440_65e902] agent.conversation_loop: API call #226: model=deepseek-v4.1-flash provider=ollama-cloud ...
2026-09-11 20:01:28,180 INFO [20260910_225440_65e902] agent.conversation_loop: API call #230: model=deepseek-v4.1-flash provider=ollama-cloud in=174263 out=7057 ...
```

2. **[observed]** `sessions` row for this session (`~/.hermes/state.db`):

```
id = 20260910_225440_65e902
model = deepseek-v4.1-flash
```

3. **[observed]** `session_model_usage` row for this session, task = `''` (the real turn path, not title/compression/approval):

```
model = deepseek-v4.1-flash
billing_provider = ollama-cloud
api_call_count = 169
last_seen = 1789145233.49  →  2026-09-11 19:47 local
```

### Session override in state.db

**Yes — an explicit `model` key exists in this session's `model_config`.** Full value, verbatim:

```json
{"max_iterations": 150, "reasoning_config": {"enabled": true, "effort": "medium"}, "max_tokens": null, "model": "deepseek-v4.1-flash", "provider": "ollama-cloud", "base_url": "https://ollama.com/v1", "api_mode": "chat_completions"}
```

**Contradiction flag — it does not currently matter, but it will.** That `"model"` key is a per-session pin. Right now it matches `config.yaml`'s `model.default`, so there is no visible divergence. But the pin wins: if you change `model.default` to something else, **this session keeps running `deepseek-v4.1-flash`** until the session is replaced. The earlier conclusion in this project's notes — that a session-level `model` key overrides `config.yaml model.default` — is consistent with what I see here; I did not re-test it by changing the config, because that would violate "change nothing".

Also **[observed]**: `session_model_usage` for this one session contains rows for **four different models**, because this session id has been carried across restarts and model changes:

| model | task | api_call_count |
|---|---|---|
| `glm-5.3` | `''` | 130 |
| `glm-5.3-flash` | `''` | 69 |
| `deepseek-v4.1-flash` | `''` | 169 |
| plus `title_generation`, `compression`, `approval` rows for several of these |

So "which model served session X" is only answerable per-row, not per-session. The current answer is `deepseek-v4.1-flash`, from the live log line and the row with the latest `last_seen`.

---

## 2. What actually happens when I call `delegate_task`

**[observed]** I dispatched one delegation during this audit (`deleg_b24b9c54`). Facts:

**It runs in the background, in this same process, on a thread.**
- `tools/async_delegation.py` imports `ThreadPoolExecutor` / `DaemonThreadPoolExecutor`. `_build_child_agent` constructs a second `AIAgent` object **in the parent process** (`from run_agent import AIAgent` inside `_build_child_agent`).
- The dispatch response said `"mode": "background"` and returned a `delegation_id` immediately.
- A row was written to the `async_delegations` table: `state=completed`, `delivery_state=delivered`, `delivery_attempts=1`, `owner_pid`, `task_json`, `event_json`, `result_json`.
- So: **not a subprocess.** Same OS process, same user, same filesystem, new thread, new `AIAgent`, fresh iteration budget.

**Which model receives it:** `kimi-k3` — the `delegation.model` value. **[observed]** in three places: the child's own session row (`sessions.model = kimi-k3`), the child's billing row (`session_model_usage.model = kimi-k3`), and the child's system-prompt footer, which literally ends:

```
Conversation started: Friday, September 11, 2026
Model: kimi-k3
Provider: custom
Platform: subagent
```

**Contradiction flag.** The child's provider is recorded as **`custom`**, not `ollama-cloud`, even though `delegation.provider: ollama-cloud`. The child's `base_url` is `https://ollama.com/v1`, which is right. Its `model_config` contains only `{"max_iterations": 50, "reasoning_config": {...}, "max_tokens": null, "_delegate_from": "20260910_225440_65e902"}` — **no provider, no base_url**. I cannot tell from outside whether `provider: custom` is a harmless label for "direct API at an explicit base_url" or a silent misconfiguration. The model actually served was `kimi-k3` and the call succeeded, so it works. Flagging it because the label and the config disagree.

**What context it gets:** exactly the `goal` string, plus the `context` string, plus a workspace hint. **[observed]** — I recovered the child's full system prompt from `state.db` (`system_prompts` table, via the child session's `system_prompt_hash`). The child's prompt is 21,784 chars. Its opening lines are the standard Hermes core prompt ("You are Hermes Agent, an intelligent AI assistant created by Nous Research…"), then `YOUR TASK:` / `CONTEXT:`, then a closing block:

```
Complete this task using the tools available to you. When finished, provide a clear, concise summary of:
- What you did
- What you found or accomplished
- Any files you created or modified
- Any issues encountered

Important workspace rule: Never assume a repository lives at /workspace/... ...
Keep your final summary tight: ...
```

**Whether it can see this conversation: no. [observed]** Confirmed four ways:
- The child's first and only `user` message is the goal string, verbatim — 341 chars. Nothing else.
- The child's system prompt contains **zero** occurrences of `MEMORY (your personal notes)`, `USER PROFILE`, `AGENT-RULES`, `WORKING-WITH-MUFFIN`, or `Nexus project`. (Counts: 0, 0, 0, 0, 0.)
- The child `AIAgent` is constructed with `skip_context_files=True, skip_memory=True, ephemeral_system_prompt=child_prompt, quiet_mode=True, platform="subagent"`.
- The child's `printenv` shows a distinct session identity: `HERMES_SESSION_ID=20260911_195315_c8d63c`, `HERMES_SESSION_SOURCE=tui`, `HERMES_DELEGATED_CHILD_CONTEXT=1`, and — importantly — `HERMES_TUI_RESUME=20260910_225440_65e902`, i.e. it *inherits the env of my session* but not my conversation.

So: a stranger with tools and my environment variables. Not a participant in this chat.

**How the result comes back:** as a **new message in this conversation**, asynchronously, minutes later — not as a return value of my tool call. **[observed]** The dispatch returned immediately; the child ran 39.7 s; its result re-entered as a new user-role message. The raw record also carries a live transcript path: `/home/muffin/.hermes/cache/delegation/live/deleg_b24b9c54/task-0.log`, which I tailed *while it ran* — so the transcript is observable in real time, before the result arrives.

**How the result is assembled — and where it goes wrong.** **[observed]** The value returned to me is `result_json.results[0].summary`, and that summary is **the child's last assistant message**, not its best one. In this run:

- The child produced a complete, correct answer to every question I asked it. It is in `state.db`, message id 32933, ~2,900 chars, labelled `A1:` through `C10:`, with the raw tool output pasted.
- A "verification nudge" then fired at the child (twice).
- The child replied to the nudge twice, and **the last of those replies is what I received as the summary**:

```
Same blocker as before, stated concretely:

**Verification is not possible in this session, by design:**
...
This is a delegated info-gathering probe, not a code change to be made build-clean. The intentionally broken file should be left as-is or cleaned up by the parent agent that orchestrated the probe.
```

The delegation was recorded `status=completed`, `exit_reason=completed`, `api_calls=5`. **Nothing in the result flags that the summary is a nudge-response rather than the answer.** The real answer was sitting in the transcript and in the DB the whole time. This is the single most important practical finding in this report — see §6, item 1.

---

## 3. Does a subagent inherit `atrium` and `muffin-style`, and the hook?

### Skills: YES — both, and all 99 of them.

**[observed]** The child's system prompt, recovered from `system_prompts` (hash `a87ea12f…`), contains `<available_skills>` … `</available_skills>` with **99** skill index lines — the same count as my own prompt (I read my own prompt from `system_prompts` hash `0178d65c…` and counted 99). Both skills are present. Verbatim from the child's prompt:

```
    - atrium: Rules for building Atrium. Read before any Atrium work.
    - muffin-style: How to work with Muffin. Read before replying to him.
```

And the child successfully loaded one — **[observed]** its tool trace shows `skill_view(name='muffin-style')`, result 11,560 bytes, and its reported first five lines verbatim:

```
---
name: muffin-style
description: "How to work with Muffin. Read before replying to him."
version: 1.0.0
platforms: [linux]
```

**But note what is inherited: the index only.** The child gets skill *names and one-line descriptions* in its prompt, exactly like I do. It gets **no skill bodies** unless it calls `skill_view` itself. My `context` string did not tell it to load `atrium`; it loaded `muffin-style` only because I asked it to. A child that never thinks to call `skill_view` runs with none of the rules.

### The hook: NO — and neither do I. See §5.

**[observed]** The child wrote a `.rs` file into a crate that has a `Cargo.toml`. The hook's own conditions were satisfied. The hook did not run. Same result for my own writes. This is not a subagent-vs-parent difference; the hook is not live in this process at all.

---

## 4. Tools: subagent vs me

**[inferred, from source]** The child inherits my toolsets, then two subtractions happen in `_build_child_agent` / `_strip_blocked_tools` / `_blocked_toolsets_for_role`:

`DELEGATE_BLOCKED_TOOLS` (verbatim from `tools/delegate_tool.py`):

```python
DELEGATE_BLOCKED_TOOLS = frozenset(
    [
        "delegate_task",  # no recursive delegation
        "clarify",        # no user interaction
        "memory",         # no writes to shared MEMORY.md
        "send_message",   # no cross-platform side effects
        "cronjob",        # no scheduling more work in the parent's name
    ]
)
```

Plus the composite toolset `delegation` is stripped outright, and `kanban` is added to the deny list.

So the child **lacks**: `delegate_task`, `clarify`, `memory`, `send_message`, `cronjob`.
I have all five.

**[observed, partial]** The child's tool trace shows it used `skill_view`, `write_file`, and `terminal` successfully. **[inferred]** It keeps everything else I have: `file` (`read_file`/`write_file`/`patch`/`search_files`), `terminal`, `skills`, `web`, `browser`, `code_execution` (`execute_code`), `computer_use`, `todo`, `session_search`, `vision`, `tts`, `bfl`, and the `context_engine` (LCM) tools. I did not enumerate the child's tool schema directly — that is the one claim in this section I could not close by observation, so treat the "keeps everything else" line as inferred.

**Filesystem write access: identical. [observed]** The child wrote to `/tmp/Nexus project/hookprobe/src/main.rs` and it landed (63 bytes, `verified: true`). Same OS user (`muffin`), same process, no sandbox, no path restriction, no capability difference. **[inferred]** There is no boundary here at all: a subagent can write anywhere I can.

**Two behavioural differences that matter more than the tool list:**

- **`clarify` is blocked** — a child cannot ask you anything. Ambiguity is resolved by guessing or by stalling, silently.
- **Dangerous commands are auto-denied by default.** **[observed]** `delegation.subagent_auto_approve` is not set in `config.yaml`; the default is `False`, which installs `_subagent_auto_deny` into every subagent worker thread — chosen specifically because a worker thread cannot reach the TUI's approval prompt and would otherwise deadlock on `input()`. The child sees a refusal it can "recover from". The refusal is logged as a `WARNING`. If the child does not report it, you never learn that its command was blocked.

---

## 5. Where `atrium-check.sh` fires — and where it does not

**Short answer: it fires nowhere in this session — not on my writes, not on a subagent's writes. It only fires when I run it by hand.**

### The decisive test

**[observed]** I built a throwaway crate and wrote a deliberately broken `.rs` file into it, twice, by two different agents:

| step | what happened | did `cargo check` run? |
|---|---|---|
| I `write_file` → `/tmp/Nexus project/hookprobe/src/probe_mine.rs` | file written | **No** — no `target/`, no `Cargo.lock` |
| subagent `write_file` → `/tmp/Nexus project/hookprobe/src/main.rs` | file written (63 bytes) | **No** — child's own `ls -d target` returned `No such file or directory`, exit 2 |
| I `write_file` → `/tmp/Nexus project/hookprobe2/src/main.rs` | file written (53 bytes) | **No** — no `target/` |
| I piped a synthetic payload straight into the script: `printf '{"hook_event_name":"post_tool_call","tool_name":"write_file","tool_input":{"path":"/tmp/Nexus project/hookprobe2/src/main.rs"},...}' \| bash ~/.hermes/agent-hooks/atrium-check.sh` | script ran | **Yes** — `target/` appeared, and it printed the compiler error |

The script is correct and works. Verbatim, its output on the synthetic payload:

```json
{"context":"cargo check failed:\n    Checking hookprobe2 v0.1.0 (/tmp/Nexus project/hookprobe2)\nsrc/main.rs:1:26: error[E0308]: mismatched types: expected `i32`, found `&str`\nerror: could not compile `hookprobe2` (bin \"hookprobe2\") due to 1 previous error"}
```

Exit 0. It found the crate, ran `cargo check`, and produced exactly the right diagnostic. The hook **does not fire because it is never wired up in the process that serves this session** — not because the script is broken.

### Why it is not wired up

**[observed]** `register_from_config` is called from exactly three places: `hermes_cli/main.py:10884`, `cli.py:1052`, and `gateway/run.py:11161`. It is called from **nowhere** in `tui_gateway/` — I grepped the whole package for `register_from_config`, `shell_hook`, and `accept_hooks`: zero hits. The TUI's Python side (`python -m tui_gateway.entry`) does plugin discovery but never registers shell hooks.

**[observed]** The log corroborates this. `agent.log` contains only two registration lines all day:

```
2026-09-11 17:12:50,831 INFO agent.shell_hooks: shell hook registered: post_tool_call -> ~/.hermes/agent-hooks/atrium-check.sh (matcher=write_file|patch, timeout=60s, fail_closed=False)
2026-09-11 19:22:39,926 INFO agent.shell_hooks: shell hook registered: post_tool_call -> ~/.hermes/agent-hooks/atrium-check.sh (matcher=write_file|patch, timeout=60s, fail_closed=False)
```

And the process that hosts my session — `python -m tui_gateway.entry`, pid 1600923, started 17:12:53 — logged its own `Plugin discovery complete: 61 found, 52 enabled` at **17:12:55,355 with no registration line following it**. Compare the process that started 17:12:48, whose discovery at 17:12:50,831 *is* followed by the registration. Same discovery, different code path, one of them skips the hook.

**[inferred]** Subagents do not make this better or worse: children are constructed **in this same process** on a worker thread, and the plugin manager's hook registry is process-global. If the hook were registered here, it would fire for both my writes and the child's. Since it fires for neither, the fault is upstream of delegation entirely.

**Two consequences worth naming:**

- `hermes hooks doctor` says everything is fine. **[observed]** Verbatim:

```
Checking 1 configured shell hook(s)...

  [post_tool_call] ~/.hermes/agent-hooks/atrium-check.sh
      ✓ script exists and is executable
      ✓ allowlisted (approved 2026-09-10T22:03:13.261890Z)
      ✓ script unchanged since approval
      ✓ produced valid JSON on synthetic payload (exit=0, 0.005s)

All shell hooks look healthy.
```

It checks the config, the file, the allowlist, and a synthetic payload. It does **not** check whether the running process registered the hook. "All shell hooks look healthy" is true and useless.

- The `hermes hooks doctor` smoke test is not a full-fidelity test either. **[observed]** Its synthetic `post_tool_call` payload uses `tool_name: "terminal"` and `args: {"command": "echo hello"}` — which does not match the `write_file|patch` matcher, and carries no `path`, so the script short-circuits at line 4 and prints `{}` in 0.005 s. A green tick there means "the script parsed a payload", not "the script did its job".

### And even if it did fire, its output would be thrown away

**[observed]** `model_tools._emit_post_tool_call_hook` calls `invoke_hook("post_tool_call", …)` and **ignores the returned list entirely**:

```python
        invoke_hook(
            "post_tool_call",
            tool_name=function_name,
            ...
        )
    except Exception as _hook_err:
        logger.debug("post_tool_call hook error: %s", _hook_err)
```

There is no aggregator for `post_tool_call` anywhere in the tree — I grepped for every `invoke_hook` call site and for any function that reads its results. Only two events have their return values consumed:
- `pre_llm_call` — `agent/turn_context.py:1097`, which reads `{"context": ...}` and appends it to the user message.
- `transform_tool_result` — `model_tools.py:1496`, which accepts a **plain string** and replaces the tool result.
- `pre_tool_call` — `hermes_cli/plugins.resolve_pre_tool_block`, reads `{"action": "block"}`.

`post_tool_call` is an **observer** event. `{"context": ...}` is the `pre_llm_call` shape. The script emits `{"context": ...}` on `post_tool_call` — the right payload for an event it is not registered on, and the wrong event for the payload it emits. **Three separate mismatches: wrong event for the payload shape, output discarded by the emitter, and not registered in this process.** Any one of them alone would make the hook a no-op.

**What the subagent mistook for the hook.** **[observed]** The child's `write_file` result contained a lint block:

```
"lint": {"status": "error", "output": "Diff in /tmp/Nexus project/hookprobe/src/main.rs:1:\n-fn main() { let x: i32 = \"not an integer\"; ..."}
```

That is `write_file`'s own built-in syntax check — **rustfmt formatting advice**, produced by the file tool, not by `cargo check` and not by the hook. The child read it as evidence a hook fired ("the write_file hook that fired is a formatter (rustfmt diff only)"). It is not a hook at all. Anything that reformats Rust will produce this, whether or not any hook exists.

---

## 6. Every point where this pipeline can fail silently

This is the section that matters. Each item: what silently does not happen, and why nothing reports it.

**1. A subagent's answer is replaced by its last message — and the result is still marked `completed`.**
The parent receives `results[0].summary` = the child's final assistant message. If anything nudges the child after it has answered — the verification nudge, an auto-continue, a compaction — the nudge response *becomes* the answer. **[observed]** in this very audit: a correct, complete 2,900-char report was produced and discarded; I received a paragraph explaining why the child would not run a verification. `status=completed`, `exit_reason=completed`, `api_calls=5`. No error, no warning, no marker. The real answer survives only in `~/.hermes/cache/delegation/live/<id>/task-N.log` and in the child's session row in `state.db`. **Anyone reading only the summary gets a confident, plausible, wrong-to-useless report.** The `output_schema` contract the delegation skill recommends does not protect against this, because the schema is applied to the final answer — the nudge response — not to the answer you wanted.

**2. The verification hook cannot fire in this session, and no check notices.**
Covered in §5. `hermes hooks doctor` reports healthy. `hermes hooks list` reports the hook as `✓ allowed`. The config is correct, the allowlist is correct, the script is correct, the script is executable, the script works when run by hand. The one thing that is wrong — the process never registered it — is invisible from every tool that exists to check hooks. You would only ever find it by noticing that a compile error you expected to see never appeared.

**3. Even when it fires, the compile errors never reach the agent.**
`_emit_post_tool_call_hook` discards return values. So the script's whole purpose — putting `cargo check` output in front of the model — cannot work on this event. If the hook were registered tomorrow, it would run `cargo check` (slow), print correct JSON, and change nothing. The failure mode would be *worse* than today, because the side effect (a `target/` directory, a `Cargo.lock`) would make it look like something happened.

**4. Every hook failure is swallowed, at a log level nobody reads.**
- `_emit_post_tool_call_hook` wraps everything in `try/except` → `logger.debug`. Default log level is INFO; `agent.log` records INFO and above. A debug line is dropped.
- `plugins.invoke_hook` wraps each callback in `try/except` → `logger.warning`, then continues.
- Shell hooks **fail open** on every non-blocking event. `_BLOCKING_EVENTS = frozenset({"pre_tool_call"})` — `post_tool_call` is not in it. So a spawn error, a timeout, or unparseable stdout produces a warning and **contributes nothing**. `fail_closed: true` on a `post_tool_call` hook is explicitly ignored, with a warning, at parse time.
Net effect: the hook can be absent, crash, time out, or return garbage, and the write proceeds as if everything is fine.

**5. A malformed payload is indistinguishable from "not my file".**
The script's first action is `path=$(echo "$payload" | jq -r '.tool_input.path // empty')`. If `jq` is missing, or the payload is not JSON, or the field is absent, `path` is empty — and the next line, `[[ "$path" != *"/Nexus project/"* ]]`, is then **true**, so the script prints `{}` and exits 0. Empty path, wrong field name, and "file is outside the project" all produce the identical silent no-op. There is no way to tell them apart from the output.

**6. The hook cannot see writes that do not go through `write_file` or `patch`.**
The matcher is `write_file|patch`. A `.rs` file written by `terminal` (`cat > src/main.rs`, `cargo new`, a heredoc, an editor, a `git checkout`) is completely uncovered. So is a file changed by a subagent's `execute_code`. The hook's coverage is "writes made through two specific tools", not "changes to the code".

**7. The runtime ignores the approval's script hash, so an approved hook can be silently replaced.**
`register_from_config` checks only `_is_allowlisted(event, command)` — the event and the command string. It does **not** compare `script_mtime_at_approval` against the current mtime. `hermes hooks doctor` *does* compare it and prints a warning — but the runtime does not care. **[observed]** the allowlist entry stores `"script_mtime_at_approval": "2026-09-10T21:59:46.143531Z"`; nothing at runtime reads it. A hook you reviewed and approved as a read-only compile check can be replaced with anything and will run on your next `write_file`, with no prompt and no log line distinguishing it.

**8. A subagent cannot ask, and nothing records that it needed to.**
`clarify` is stripped. When a child hits ambiguity it guesses or stalls. The result arrives as a normal completed delegation. You cannot tell from the summary whether it had a question.

**9. A subagent's blocked command looks like a normal completed delegation.**
`subagent_auto_approve` defaults to false, so dangerous commands are auto-denied in the worker thread. The child receives a refusal and is expected to "recover". The only trace is a `WARNING` in `agent.log`. If the child works around it or gives up quietly, the delegation still reports `completed`.

**10. Delegation delivery can sit at `pending` with no notification.**
Completion delivery is claimed either by the TUI or by the gateway's `_async_delegation_watcher` (which runs inside `hermes-gateway.service`, a separate process, and polls every 2 s). **[observed]** the `async_delegations` table carries `delivery_state`, `delivery_attempts`, `owner_pid`, `owner_started_at`, `delivery_claim`, `delivery_claimed_at` — machinery for exactly this problem. If neither the TUI nor the gateway claims a completed row, it stays `pending`. The child's work is not lost (it is in `state.db`), but nothing tells you it finished. You would have to query the table to find out.

**11. Skills are inherited as names only; preferences are not inherited at all.**
**[observed]** The child gets all 99 skill *names* and *one-line descriptions* — and zero memory, zero user profile, zero project context. `WORKING-WITH-MUFFIN.md`, `AGENT-RULES.md`, and the standing Atrium routing rules are absent from its prompt. It is not told to read the `atrium` skill. A subagent therefore behaves like a competent stranger who has never met you, unless the briefing text you paste says otherwise. Nothing in the result indicates which rules it did or did not follow.

**12. `output_schema` is validated, but nothing validates that the schema describes the truth.**
A child can satisfy every required field with `what_was_not_done: "nothing"` and `what_is_uncertain: "none"`. The schema forces shape, not honesty. **[observed]** in this run the child's summary was a non-answer and it still reported `status=completed` — the schema did not fire because I did not pass one, and passing one would not have caught it either.

**13. Tool availability is checked by an unenforced convention.**
The delegation skill's own note is confirmed by the source: the model has **no `toolsets` argument**. Any constraint you write into `goal`/`context` ("use the terminal tool only") is a request to a language model, not a boundary. **[observed]** `event_json.toolsets` for this delegation is `null` — the call cannot express a toolset restriction at all. Enforcement is limited to the fixed blocklist in item 4/§4. Anything else is honour-system.

**14. The child's provider is recorded as `custom` while the config says `ollama-cloud`.**
§2. It worked, so this is not a live failure — but the label and the config disagree, and I could not determine from outside which is authoritative. If a future delegation *did* mis-route, this is the field you would be looking at, and it does not currently mean what it says.

**15. One more, structural: the whole chain is checked by tools that check the wrong thing.**
`hermes hooks doctor` → checks config/allowlist/payload, not the live registry.
`hermes hooks list` → checks config, not the live registry.
`write_file`'s lint block → looks like a build check, is a formatter.
The delegation `summary` → looks like the child's answer, is its last message.
The delegation `status=completed` → means "the child stopped", not "the child succeeded".
Each of these is a green signal that is green for a reason other than the one you want. That pattern, more than any single bug, is the finding.

---

## 7. Probe artefacts left on disk (disclose / decide)

Created for this audit, all outside the project, none of them part of Atrium:

- `/tmp/Nexus project/hookprobe/` — `Cargo.toml`, `src/main.rs` (child's write), `src/probe_mine.rs` (my write), plus `target/` and `Cargo.lock` from my manual script run
- `/tmp/Nexus project/hookprobe2/` — `Cargo.toml`, `src/main.rs`, plus `target/` and `Cargo.lock` from my manual script run
- Delegation `deleg_b24b9c54` — session `20260911_195315_c8d63c` in `state.db`; transcript at `/home/muffin/.hermes/cache/delegation/live/deleg_b24b9c54/task-0.log`
- The report file you are reading

No existing file under `/home/muffin/Desktop/Nexus project/` was modified or deleted — the only thing written there is this report. Nothing in `~/.hermes/config.yaml`, the allowlist, or the hook script was touched. Say the word and I will delete the two `/tmp` crates.
