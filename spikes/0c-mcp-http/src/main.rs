// Atrium Phase 0c spike — MCP over remote transport (streamable HTTP).
//
// Question (BUILD-PLAN §0c): can rmcp connect to the official GitHub MCP
// server, in READ-ONLY mode, over streamable HTTP — list tools, call one
// read-only tool, print the result?
//
// Transport: StreamableHttpClientTransport::from_uri against
// https://api.githubcopilot.com/mcp/readonly (URL-path read-only variant —
// the /readonly suffix restricts the toolset to read-only tools; no
// X-MCP-Readonly header needed).
//
// Auth: GitHub PAT from the GITHUB_PERSONAL_ACCESS_TOKEN env var.
// The token is read from the environment — never hardcoded, never printed.

use rmcp::{
    model::CallToolRequestParams,
    transport::StreamableHttpClientTransport,
    transport::streamable_http_client::StreamableHttpClientTransportConfig,
    ServiceExt,
};

fn main() -> anyhow::Result<()> {
    // Single-threaded runtime suffices; keep the spike boring.
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async_main())
}

async fn async_main() -> anyhow::Result<()> {
    println!("== Phase 0c spike: rmcp client vs GitHub MCP (remote, read-only) ==");

    let token = std::env::var("GITHUB_PERSONAL_ACCESS_TOKEN")
        .map_err(|_| anyhow::anyhow!("GITHUB_PERSONAL_ACCESS_TOKEN not set"))?;
    println!("[0] token found in environment (value never printed)");

    // The transport has a first-class auth_header field (verified in rmcp
    // 3.3.0 source, StreamableHttpClientTransportConfig) — no custom headers,
    // no direct http/reqwest deps needed.
    // struct is #[non_exhaustive]: build via with_uri, then set pub fields.
    let mut config = StreamableHttpClientTransportConfig::with_uri(
        "https://api.githubcopilot.com/mcp/readonly",
    );
    // rmcp adds the `Bearer ` prefix itself — see the doc comment on
    // StreamableHttpClientTransportConfig::auth_header in rmcp 3.3.0:
    // "A bearer token without the `Bearer ` prefix". Passing `Bearer <token>`
    // here produced `Bearer Bearer <token>` on the wire and GitHub rejected it
    // with HTTP 400 "bad request: Authorization header is badly formatted".
    config.auth_header = Some(token);

    // from_config builds the transport with rmcp's own compatible reqwest
    // client (feature transport-streamable-http-client-reqwest).
    let transport = StreamableHttpClientTransport::from_config(config);

    // Connect + initialize handshake
    let client = ().serve(transport).await?;
    println!("[1] initialized: connected to GitHub MCP remote server (read-only)");

    // List tools (paginated — list_all_tools walks the cursor)
    let tools = client.list_all_tools().await?;
    println!("[2] listed {} tools:", tools.len());
    for t in &tools {
        println!("    - {}", t.name);
    }

    // Call one read-only tool: get_me — the identity of the authenticated user.
    // Zero side effects; also proves the token is valid.
    let result = client
        .call_tool(CallToolRequestParams::new("get_me").with_arguments(
            serde_json::json!({}).as_object().expect("literal is always an object").clone(),
        ))
        .await?;

    println!("[3] call_tool(get_me) result:");
    for block in &result.content {
        match block {
            rmcp::model::ContentBlock::Text(text_content) => {
                println!("    text: {}", text_content.text)
            }
            other => println!("    (non-text block: {:?})", other),
        }
    }
    println!("    is_error: {:?}", result.is_error);

    println!("\n== spike complete: remote MCP over streamable HTTP works. 0c answered. ==");

    client.cancel().await?;
    Ok(())
}