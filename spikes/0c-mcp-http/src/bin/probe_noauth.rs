// Throwaway probe — NOT part of the 0c deliverable.
//
// Question: does rmcp 3.3.0's streamable-HTTP *client* transport work against
// a real remote MCP server at all, independent of GitHub auth?
//
// 0c has two unproven variables stacked: (a) the transport, (b) whether
// GitHub's endpoint accepts the auth header as constructed. If Muffin runs
// 0c with a token and it fails, we cannot tell which one broke. This probe
// removes (a) from the equation by hitting a public, no-auth MCP endpoint.
//
// Runs against https://mcp.deepwiki.com/mcp — verified by curl to accept
// streamable HTTP with no credentials.

use rmcp::{
    model::CallToolRequestParams,
    transport::StreamableHttpClientTransport,
    transport::streamable_http_client::StreamableHttpClientTransportConfig,
    ServiceExt,
};

fn main() -> anyhow::Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async_main())
}

async fn async_main() -> anyhow::Result<()> {
    println!("== probe: rmcp streamable-HTTP client vs no-auth remote MCP ==");

    let config =
        StreamableHttpClientTransportConfig::with_uri("https://mcp.deepwiki.com/mcp");
    let transport = StreamableHttpClientTransport::from_config(config);

    let client = ().serve(transport).await?;
    println!("[1] initialized — transport connected, handshake completed");

    let tools = client.list_all_tools().await?;
    println!("[2] listed {} tools:", tools.len());
    for t in &tools {
        println!("    - {}", t.name);
    }

    // ask_question is read-only (it queries a public wiki). No side effects.
    let result = client
        .call_tool(
            CallToolRequestParams::new("ask_question").with_arguments(
                serde_json::json!({
                    "repoName": "modelcontextprotocol/servers",
                    "question": "What transports does MCP define?"
                })
                .as_object()
                .expect("literal is an object")
                .clone(),
            ),
        )
        .await?;

    println!("[3] call_tool(ask_question) result:");
    for block in &result.content {
        match block {
            rmcp::model::ContentBlock::Text(t) => {
                let s: String = t.text.chars().take(400).collect();
                println!("    text: {}", s)
            }
            other => println!("    (non-text block: {:?})", other),
        }
    }
    println!("    is_error: {:?}", result.is_error);

    println!("\n== probe complete: remote streamable-HTTP transport WORKS (no auth involved) ==");

    client.cancel().await?;
    Ok(())
}
