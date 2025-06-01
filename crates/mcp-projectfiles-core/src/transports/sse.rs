use crate::handler::{CoreHandler, create_server_details};
use async_trait::async_trait;
use rust_mcp_schema::{
    CallToolRequest, CallToolResult, ListToolsRequest, ListToolsResult, RpcError,
    schema_utils::CallToolError,
};
use rust_mcp_sdk::{
    McpServer,
    mcp_server::{HyperServerOptions, ServerHandler, hyper_server},
};
use tracing::{debug, error, info, instrument};

/// SSE transport handler that wraps the core handler
pub struct SseHandler {
    core: CoreHandler,
}

impl SseHandler {
    pub fn new() -> Self {
        Self {
            core: CoreHandler::new(),
        }
    }
}

#[async_trait]
impl ServerHandler for SseHandler {
    /// Handle tool listing requests for SSE transport
    #[instrument(level = "debug", skip(self, _runtime))]
    async fn handle_list_tools_request(
        &self,
        request: ListToolsRequest,
        _runtime: &dyn McpServer,
    ) -> std::result::Result<ListToolsResult, RpcError> {
        debug!("SSE transport: handling list_tools request");
        self.core.list_tools(request).await
    }

    /// Handle tool call requests for SSE transport
    #[instrument(level = "debug", skip(self, _runtime), fields(tool_name = %request.params.name))]
    async fn handle_call_tool_request(
        &self,
        request: CallToolRequest,
        _runtime: &dyn McpServer,
    ) -> std::result::Result<CallToolResult, CallToolError> {
        debug!(tool_name = %request.params.name, "SSE transport: handling call_tool request");
        self.core.call_tool(request).await
    }
}

impl Default for SseHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Run the MCP server with SSE transport
#[instrument(level = "info", fields(port))]
pub async fn run_sse_server(port: u16) -> anyhow::Result<()> {
    info!("Initializing SSE transport handler");
    let handler = SseHandler::new();
    let server_details = create_server_details();

    info!(port, "Starting MCP server with SSE transport");

    // Create SSE server using hyper_server from rust-mcp-sdk
    let server_options = HyperServerOptions {
        host: "0.0.0.0".to_string(),
        port,
        ..Default::default()
    };

    let server = hyper_server::create_server(server_details, handler, server_options);

    info!("SSE transport endpoints:");
    info!(
        port,
        "  GET  http://0.0.0.0:{}/sse     - SSE stream (server → client)", port
    );
    info!(
        port,
        "  POST http://0.0.0.0:{}/message - HTTP messages (client → server)", port
    );
    info!(
        port,
        "Test with MCP Inspector: http://localhost:6274/?transport=sse&serverUrl=http://localhost:{}/sse",
        port
    );

    // Start and run the server
    info!("Starting SSE server");
    server.start().await.map_err(|e| {
        error!(error = %e, "SSE server startup failed");
        anyhow::anyhow!("Server error: {}", e)
    })?;

    info!("SSE server stopped");

    Ok(())
}