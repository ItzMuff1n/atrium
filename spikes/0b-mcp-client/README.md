# Spike 0b — the MCP client

**Question (BUILD-PLAN §0b):** can a Rust program, using the official Rust
MCP SDK (rmcp), connect to the official `everything` reference server over
stdio, list its tools, call one, and print the result?

**Stop-and-rethink condition:** if the Rust MCP SDK fails in ways not quickly
fixable, the backend language decision reopens (DECISIONS.md §Backend).
**Result: NOT TRIGGERED.**

**Approach:** `rmcp` 3.3.0 (latest stable, released 2026-09-10) with features
`client`, `transport-child-process`, `transport-io`. The SDK's own README
pattern: `TokioChildProcess` spawning
`npx -y @modelcontextprotocol/server-everything`, then `list_all_tools()`,
then `call_tool()`.

## Observed results (this session, run by the agent)

`cargo run` — first-run output, verbatim:

```
Starting default (STDIO) server...
[1] initialized: client connected to everything server
[2] listed 13 tools:
    - echo, get-annotated-message, get-env, get-resource-links,
      get-resource-reference, get-structured-content, get-sum,
      get-tiny-image, gzip-file-as-resource, toggle-simulated-logging,
      toggle-subscriber-updates, trigger-long-running-operation,
      simulate-research-query
[3] call_tool(echo) result:
    text: Echo: hello from atrium 0b spike
    is_error: None
```

Exit code 0. The child npx process printed its own startup line — that's
the server talking, confirming a real child process over real stdio.

## API findings (rmcp 3.3.0, learned the hard way via rustc)

- The `client` cargo feature is **required** ( RoleClient is feature-gated;
  error is `no RoleClient in the root` without it).
- The content enum is `rmcp::model::ContentBlock` (variants: `Text`,
  `Image`, `Audio`, `Resource`, `ResourceLink`).
- `CallToolRequestParams::new(name).with_arguments(JsonObject)` — plural,
  takes a `serde_json::Map`, no builder `with_argument` singular form.
- `CallToolResult.is_error` is `Option<bool>` — printed `None` on success.

## Rough edges encountered (relevant to DECISIONS.md's "expect rough edges")

Three compile errors before first clean build, all API-surface drift between
docs/older examples and rmcp 3.3.0, all fixed within one build cycle, none
conceptual. The compiler messages were accurate and pointed at the right
builder. Verdict on "rough edges": mild, navigable, not a threat to the
backend choice.

## Verdict: VALIDATED (agent-observed; pending user confirmation)

Transport over stdio to a real reference server works; tool listing and
invocation work. This was the riskiest dependency in the stack decision.

## Recommendation for the real build

- Pin rmcp version explicitly in the real project (API surface is moving —
  the enum renamed `Content`→`ContentBlock` within recent memory).
- Reuse the exact feature set: `client`, `transport-child-process`,
  `transport-io` (+ `transport-streamable-http-client-reqwest` for 0c).
- npx spawn means node is a runtime dependency for MCP servers installed
  via npm — decide later whether to vendor servers or accept the dependency.