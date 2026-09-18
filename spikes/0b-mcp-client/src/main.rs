// Phase 0b spike — Rust MCP client against the official `everything`
// reference server over stdio.
//
// BUILD-PLAN 0b: list its tools, call one, print the result.
// Stop-and-rethink condition: if this fails in ways not quickly fixable,
// the backend language decision changes (see DECISIONS.md §Backend).
//
// Uses the SDK's own documented pattern (rust-sdk README "Build a Client"):
// TokioChildProcess spawning `npx -y @modelcontextprotocol/server-everything`,
// then list_all_tools() and call_tool().

use rmcp::model::CallToolRequestParams;
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use rmcp::ServiceExt;
use tokio::process::Command;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("== Phase 0b spike: rmcp client vs everything server ==\n");

    // 1. Spawn the everything server as a child process over stdio.
    let transport = TokioChildProcess::new(Command::new("npx").configure(|cmd| {
        cmd.arg("-y").arg("@modelcontextprotocol/server-everything");
    }))?;

    let client = ().serve(transport).await?;
    println!("[1] initialized: client connected to everything server");

    // 2. List its tools.
    let tools = client.list_all_tools().await?;
    println!("[2] listed {} tools:", tools.len());
    for tool in &tools {
        println!("    - {}", tool.name);
    }

    // 3. Call one: `echo`, the everything server's most basic tool.
    let params = CallToolRequestParams::new("echo").with_arguments(
        serde_json::json!({ "message": "hello from atrium 0b spike" })
            .as_object()
            .expect("literal is always an object")
            .clone(),
    );
    let result = client.call_tool(params).await?;
    println!("\n[3] call_tool(echo) result:");
    // Print the raw content items so we see exactly what the server returned.
    for content in &result.content {
        match content {
            rmcp::model::ContentBlock::Text(text_content) => {
                println!("    text: {}", text_content.text);
            }
            other => println!("    (non-text content: {other:?})"),
        }
    }
    // Don't guess the is_error shape — print it verbatim.
    println!("    is_error: {:?}", result.is_error);

    println!("\n== spike complete: rmcp works. 0b core question answered. ==");
    Ok(())
}