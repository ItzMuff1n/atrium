# Spike 0c — MCP over remote transport (repaired, blocked on credential)

**Question (BUILD-PLAN §0c):** connect to GitHub MCP in **read-only mode**
over **streamable HTTP**; list tools; call one read-only tool; print the result.

**Status:** built, compiles clean, **one latent bug found and fixed 11 Sep 2026
(see below) — it would have failed at the first network call even with a valid
token.** Remote endpoint confirmed reachable (401 without auth). Still blocked
only on a GitHub PAT. Paused by user decision (Sep 2026): "sort a token later,
proceed to something else meanwhile."

## Latent bug found and fixed 11 Sep 2026 (the important part)

The crate as first written had **no TLS backend in its dependency tree at all**.
Every `https://` request died at the transport layer before reaching the network.

- Cause: `rmcp`'s feature `transport-streamable-http-client-reqwest` enables
  `__reqwest` (the `reqwest` *crate*) but **not** any TLS feature. TLS is a
  separate opt-in: `reqwest = ["__reqwest", "reqwest?/rustls"]`, or
  `reqwest-native-tls`, or `reqwest-tls-no-provider`.
- Symptom: `Error: Send message error Transport [...] error: Client error: error
  sending request for url (https://…), when send initialize request` — with no
  mention of TLS. `curl` to the same URL returned HTTP 200, which is what made
  the cause findable: the fault was local, not the server.
- Evidence (observed): `grep -E '^name = "(rustls|native-tls|openssl)"'
  Cargo.lock` returned nothing before the fix; after adding the `reqwest`
  feature it compiled `rustls 0.23.43`, `hyper-rustls 0.27.9`,
  `rustls-platform-verifier 0.7.0`.
- Fix: add `"reqwest"` to the rmcp feature list in `Cargo.toml`.

**Verified after the fix:** a throwaway no-auth probe
(`src/bin/probe_noauth.rs`) connected to `https://mcp.deepwiki.com/mcp` —
initialized, listed 3 tools, called `ask_question`, got text back,
`is_error: Some(false)`, exit 0. So rmcp 3.3.0's streamable-HTTP client
transport works against a real remote MCP server. Observed, not inferred.

This also means the two 0c unknowns are now separated: the transport is proven,
and the only thing left untested is whether GitHub accepts the auth header as
constructed — which is exactly what the token run shows.

**Carry this forward:** any future rmcp HTTP client needs an explicit TLS
feature. Compiles clean, fails at request time, error does not say "TLS".

**Second gotcha, found the same day:** adding a second binary
(`src/bin/probe_noauth.rs`) silently broke bare `cargo run` for the real spike —
`error: cargo run could not determine which binary to run`. Fixed with
`default-run = "mcp-http-spike"` in `[package]`. Any extra binary added to a
crate that relied on a lone `main.rs` needs this key, or every documented
`cargo run` command in this project breaks.

## What was built (observed)

- `Cargo.toml`: `rmcp = "3"` features `client`, `transport-streamable-http-client-reqwest`; `anyhow`, `serde_json`, `tokio`.
- `src/main.rs`: connects to `https://api.githubcopilot.com/mcp/readonly`
  (read-only enforced server-side via the `/readonly` URL path — verified in
  GitHub's docs/remote-server.md), auth via the transport's first-class
  `auth_header` config field, token read from env
  `GITHUB_PERSONAL_ACCESS_TOKEN` (never hardcoded, never printed).
  Plans: `list_all_tools()`, then call `get_me` (read-only, zero side
  effects, also proves the token works), print content blocks + `is_error`.
- `cargo build` → exit 0. `cargo run` without a token → clean refusal:
  `Error: GITHUB_PERSONAL_ACCESS_TOKEN not set`, exit 1, no hang, nothing
  invented.

## How to resume (when a token exists)

```
cd "spikes/0c-mcp-http"
export GITHUB_PERSONAL_ACCESS_TOKEN=<token>     # fine-grained PAT, no scopes
cargo run
```

Expected: `[1] initialized` → tool list → `[3] call_tool(get_me)` with your
GitHub login in the text block → `is_error: None`. That output is the 0c
verification gate.

A fine-grained PAT with **no repository access scopes** is sufficient: the
toolset is already read-only by URL, and `get_me` only reports identity.

**If it fails with `error sending request for url`:** check the TLS feature
first (above). That failure mode looks like a network problem and is not one.

## Findings (real, reusable)

- **Dual-crate trap:** adding a direct `reqwest`/`http` dependency compiled a
  *second, incompatible* copy of those crates — rmcp 3.3.0 internally uses
  reqwest **0.13.5**, so a direct `reqwest = "0.12"` produced
  `reqwest::Client: StreamableHttpClient is not satisfied` (7 errors). Fix:
  drop the direct dep; use `StreamableHttpClientTransport::from_config(config)`
  which builds rmcp's own client, and pass auth via
  `config.auth_header = Some("Bearer …")` (a plain `String` field — no http
  crate types needed at all).
- rmcp HTTP transport API verified against installed crate source
  (streamable_http_client.rs): `with_uri`, `custom_headers(HashMap<HeaderName,
  HeaderValue>)`, `auth_header: Option<String>` pub field; config struct is
  `#[non_exhaustive]` — build with `with_uri`, then set pub fields.
- Remote server research (github.com/github/github-mcp-server README +
  docs/remote-server.md, cached in `~/.hermes/cache/web/`): URL-path
  variants `/readonly`, `/insiders`, `/x/{toolset}/readonly` exist; header
  equivalents `X-MCP-Readonly`, `X-MCP-Toolsets`. Local Docker server
  (ghcr.io/github/github-mcp-server) needs the same PAT — it is not an
  auth-free alternative.

## Not done / uncertain

- The actual remote round-trip: not run, zero network calls made to GitHub.
  Everything above is compile-verified, not connection-verified.
- Nothing else uncertain; the only unknown is whether the token flow works,
  which is exactly what the paused run would show.