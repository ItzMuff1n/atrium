# Hermes recon — REPORT ONLY (v0.20.0)

**Snapshot taken 10 Sep 2026** (session `20260910_225440_65e902`), against
Hermes Agent v0.20.0 on this machine. It is a record of what was observed that
day, not a live reference.

**The model values in §1 and §2 are stale and contradict this project's current
state.** They say `model.default: glm-5.3` with a `glm-5.3-flash` session
override; the project moved to `deepseek-v4.1-flash` (delegation `kimi-k3`) on
11 Sep 2026. **Live config values live in `STATUS.md` and nowhere else** —
per the project's "one fact, one home" rule (HANDOFF §6). Do not read a model id
out of this file; read it from STATUS.md or check the live config.

Kept as written because the method and the findings about *where* values live
(state.db, not config.yaml; precedence rules) are still correct and still
useful. Only the values moved.

*(Banner added 12 Sep 2026 during a consistency sweep. The file previously had
no date at all, so its claims read as current.)*

## 1. Exact `model:` block from ~/.hermes/config.yaml (verbatim, lines 1–4, as of 10 Sep 2026)

```yaml
model:
  default: glm-5.3
  provider: ollama-cloud
  base_url: https://ollama.com/v1
```

## 2. Runtime override: glm-5.3-flash while the file says glm-5.3 (10 Sep 2026)

Yes. The runtime model of this chat is `glm-5.3-flash` (provider `ollama-cloud`); the config default is `glm-5.3`.

Where the override comes from (all observed this session):

- **Not an environment variable.** `/proc/<pid>/environ` of every running Hermes process (gateway pid 1459076; TUI pids 1459076/1464928/1469117 `hermes --continue`; tui_gateway pids 1459235/1465023) contains no model/provider override vars — only `OLLAMA_API_KEY`.
- **It is a session-persisted override in the state database.** File: `/home/muffin/.hermes/state.db`, table `sessions`, column `model` (plus `model_config`). This is a database row, not a config line — there is no line number; the coordinates are table + column + row. Observed row: session `20260910_225440_65e902`, started 2026-09-10 19:54:43 UTC, `model` = `glm-5.3-flash`, `model_config` =
  `{"max_iterations": 150, "reasoning_config": {"enabled": true, "effort": "medium"}, "max_tokens": null, "model": "glm-5.3-flash", "provider": "ollama-cloud", "base_url": "https://ollama.com/v1", "api_mode": "chat_completions"}`
  (Neighboring row `20260910_225401_f04d54` shows `model` = `glm-5.3` — a chat that did not get the override.)
- **Why it wins:** `/home/muffin/.hermes/hermes-agent/hermes_cli/model_switch.py`, line 765 — "Resolve the effective model: session override > channel > global." and "A user-issued `/model` (session override) always wins over per-channel/session-persisted configuration, which wins over the global default."
- **Not verified:** the exact `/model` invocation that created the override (the TUI picker's action left no log I found). The persisted row and the precedence rule are observed; the setting action itself is inferred, not observed.

## 3. Complete YAML frontmatter of ~/.hermes/skills/autonomous-ai-agents/hermes-agent/SKILL.md (verbatim, lines 1–13, nothing else)

```
---
name: hermes-agent
description: "Use, configure, theme, extend, and orchestrate Hermes Agent."
version: 3.1.0
author: Hermes Agent + Teknium
license: MIT
platforms: [linux, macos, windows]
metadata:
  hermes:
    tags: [hermes, setup, configuration, multi-agent, spawning, cli, gateway, themes, skins, desktop-plugins, tui-widgets, petdex, development]
    homepage: https://github.com/NousResearch/hermes-agent
    related_skills: [claude-code, codex, opencode]
---
```

## 4. Complete shipped worked example of a post_tool_call hook (verbatim, start to finish)

From `~/.hermes/hermes-agent/website/docs/user-guide/features/hooks.md`, section "1. Auto-format Python files after every write":

```
#### 1. Auto-format Python files after every write

```yaml
# ~/.hermes/config.yaml
hooks:
  post_tool_call:
    - matcher: "write_file|patch"
      command: "~/.hermes/agent-hooks/auto-format.sh"
```

```bash
#!/usr/bin/env bash
# ~/.hermes/agent-hooks/auto-format.sh
payload="$(cat -)"
path=$(echo "$payload" | jq -r '.tool_input.path // empty')
[[ "$path" == *.py ]] && command -v black >/dev/null && black "$path" 2>/dev/null
printf '{}\n'
```

The agent's in-context view of the file is **not** re-read automatically — the reformat only affects the file on disk. Subsequent `read_file` calls pick up the formatted version.
```

## 5. Full VALID_HOOKS list from hermes_cli/plugins.py (one name per line; 24 entries, set closes at line 219)

```
pre_tool_call
post_tool_call
transform_terminal_output
transform_tool_result
transform_llm_output
pre_llm_call
post_llm_call
pre_verify
pre_api_request
post_api_request
api_request_error
on_session_start
on_session_end
on_session_finalize
on_session_reset
on_skill_lifecycle
subagent_start
subagent_stop
pre_gateway_dispatch
pre_approval_request
post_approval_response
kanban_task_claimed
kanban_task_completed
kanban_task_blocked
```

## 6. Exact non-TTY flag to bypass the shell-hooks consent prompt

`--accept-hooks`

Per hooks.md, a non-TTY run needs one of: `--accept-hooks` (CLI flag), `HERMES_ACCEPT_HOOKS=1` (environment variable), or `hooks_auto_accept: true` in config.yaml — otherwise a newly added hook silently stays unregistered until its `(event, command)` pair is consented to (consent is persisted to `~/.hermes/shell-hooks-allowlist.json`).