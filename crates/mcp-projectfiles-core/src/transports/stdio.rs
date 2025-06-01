use crate::handler::{CoreHandler, create_server_details};
use async_trait::async_trait;
use rust_mcp_schema::{
    CallToolRequest, CallToolResult, ListToolsRequest, ListToolsResult, RpcError,
    schema_utils::CallToolError,
};
use rust_mcp_sdk::{
    McpServer, StdioTransport, TransportOptions,
    mcp_server::{ServerHandler, server_runtime},
};
use tracing::{debug, error, info, instrument};

/// Stdio transport handler that wraps the core handler
pub struct StdioHandler {
    core: CoreHandler,
}

impl StdioHandler {
    pub fn new() -> Self {
        Self {
            core: CoreHandler::new(),
        }
    }
}

#[async_trait]
impl ServerHandler for StdioHandler {
    /// Handles requests to list available tools
    #[instrument(level = "debug", skip(self, _runtime))]
    async fn handle_list_tools_request(
        &self,
        request: ListToolsRequest,
        _runtime: &dyn McpServer,
    ) -> std::result::Result<ListToolsResult, RpcError> {
        debug!("Stdio transport: handling list_tools request");
        self.core.list_tools(request).await
    }

    /// Handles requests to call a specific tool
    #[instrument(level = "debug", skip(self, _runtime), fields(tool_name = %request.params.name))]
    async fn handle_call_tool_request(
        &self,
        request: CallToolRequest,
        _runtime: &dyn McpServer,
    ) -> std::result::Result<CallToolResult, CallToolError> {
        debug!(tool_name = %request.params.name, "Stdio transport: handling call_tool request");
        self.core.call_tool(request).await
    }
}

impl Default for StdioHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Run the MCP server with stdio transport
#[instrument(level = "info")]
pub async fn run_stdio_server() -> anyhow::Result<()> {
    info!("Initializing stdio transport handler");
    let handler = StdioHandler::new();
    let server_details = create_server_details();

    info!("Starting MCP server with stdio transport");

    // Create stdio transport
    let transport = StdioTransport::new(TransportOptions::default())
        .map_err(|e| anyhow::anyhow!("Failed to create stdio transport: {}", e))?;

    // Create and run server with stdio transport
    let server = server_runtime::create_server(server_details, transport, handler);

    // Start and run the server
    info!("Starting stdio server");
    server.start().await.map_err(|e| {
        error!(error = %e, "Stdio server startup failed");
        anyhow::anyhow!("Server error: {}", e)
    })?;

    info!("Stdio server stopped");
    Ok(())
}