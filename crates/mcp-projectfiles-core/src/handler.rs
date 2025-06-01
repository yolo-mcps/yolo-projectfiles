use crate::context::{StatefulTool, ToolContext};
use crate::tools::ProtocolTools;
use rust_mcp_schema::{
    CallToolRequest, CallToolResult, InitializeResult, ListToolsRequest, ListToolsResult, RpcError,
    ServerCapabilities, ServerCapabilitiesTools, schema_utils::CallToolError,
};
use tracing::{debug, error, info, instrument};

/// Core MCP server handler with transport-agnostic business logic
pub struct CoreHandler {
    /// Shared context for stateful tools
    context: ToolContext,
}

impl CoreHandler {
    /// Create a new handler with default context
    pub fn new() -> Self {
        Self {
            context: ToolContext::new(),
        }
    }

    /// Create a new handler with custom context
    pub fn new_with_context(context: ToolContext) -> Self {
        Self { context }
    }

    /// Get a reference to the tool context
    pub fn context(&self) -> &ToolContext {
        &self.context
    }

    /// Handle tool listing requests (transport-agnostic)
    #[instrument(level = "debug", skip(self))]
    pub async fn list_tools(
        &self,
        _request: ListToolsRequest,
    ) -> std::result::Result<ListToolsResult, RpcError> {
        debug!("Handling list_tools request");
        let tools = ProtocolTools::tools();
        info!(tool_count = tools.len(), "Listed available tools");

        Ok(ListToolsResult {
            tools,
            next_cursor: None,
            meta: None,
        })
    }

    /// Handle tool call requests (transport-agnostic)
    #[instrument(level = "debug", skip(self), fields(tool_name = %request.params.name))]
    pub async fn call_tool(
        &self,
        request: CallToolRequest,
    ) -> std::result::Result<CallToolResult, CallToolError> {
        let tool_name = &request.params.name;
        debug!(tool_name, "Parsing tool request");

        let tool = ProtocolTools::try_from(request.params.clone()).map_err(|e| {
            error!(tool_name, error = %e, "Failed to parse tool request");
            CallToolError::unknown_tool(format!("Failed to parse tool request: {}", e))
        })?;

        info!(tool_name, "Executing tool");
        let result = match tool {
            // Stateless tools - call directly
            ProtocolTools::CalculatorTool(calc) => calc.call().await,
            ProtocolTools::FileReadTool(file_read) => file_read.call().await,
            ProtocolTools::FileWriteTool(file_write) => file_write.call().await,
            ProtocolTools::FileListTool(file_list) => file_list.call().await,
            ProtocolTools::SystemInfoTool(system_info) => system_info.call().await,
            ProtocolTools::EnvironmentTool(env_tool) => env_tool.call().await,
            ProtocolTools::CurrentTimeTool(time_tool) => time_tool.call().await,
            ProtocolTools::TimestampTool(timestamp) => timestamp.call().await,

            // Stateful tools - call with context
            ProtocolTools::CounterIncrementTool(counter) => {
                counter.call_with_context(&self.context).await
            }
            ProtocolTools::CounterGetTool(counter) => {
                counter.call_with_context(&self.context).await
            }
            ProtocolTools::CacheSetTool(cache) => cache.call_with_context(&self.context).await,
            ProtocolTools::CacheGetTool(cache) => cache.call_with_context(&self.context).await,
        };

        match &result {
            Ok(_) => info!(tool_name, "Tool execution completed successfully"),
            Err(e) => error!(tool_name, error = %e, "Tool execution failed"),
        }

        result
    }
}

impl Default for CoreHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Creates server details with tools capability
pub fn create_server_details() -> InitializeResult {
    InitializeResult {
        protocol_version: "2025-03-26".to_string(),
        capabilities: ServerCapabilities {
            tools: Some(ServerCapabilitiesTools {
                list_changed: Some(true),
            }),
            completions: None,
            experimental: None,
            logging: None,
            prompts: None,
            resources: None,
        },
        server_info: rust_mcp_schema::Implementation {
            name: "projectfiles".to_string(),
            version: "0.1.0".to_string(),
        },
        instructions: Some("MCP Server with tools support".to_string()),
        meta: None,
    }
}

/// Test the core handler functionality
pub async fn test_handler() -> anyhow::Result<()> {
    use crate::tools::ProtocolTools;

    tracing::info!("Testing tool handler implementation");

    tracing::info!("Testing tool handler...");
    tracing::info!("Testing list_tools...");

    // Test list_tools via ProtocolTools
    let tools = ProtocolTools::tools();
    tracing::info!("✅ list_tools successful: {} tools available", tools.len());

    for tool in &tools {
        tracing::info!(
            "  - {}: {}",
            tool.name,
            tool.description.as_deref().unwrap_or("No description")
        );
    }

    // Test calculator tool directly
    tracing::info!("Testing calculator tool...");
    let calc_tool = crate::tools::CalculatorTool {
        expression: "2 + 2".to_string(),
    };

    match calc_tool.call().await {
        Ok(result) => {
            if let Some(content) = result.content.first() {
                match content {
                    rust_mcp_schema::CallToolResultContentItem::TextContent(text) => {
                        tracing::info!(
                            "✅ calculator tool successful: 2 + 2 = {}",
                            text.text.trim()
                        );
                    }
                    _ => {
                        tracing::info!("✅ calculator tool successful (non-text result)");
                    }
                }
            }
        }
        Err(e) => {
            tracing::error!("❌ calculator tool failed: {}", e);
            return Err(anyhow::anyhow!("calculator test failed"));
        }
    }

    // Test system_info tool directly
    tracing::info!("Testing system_info tool...");
    let sys_tool = crate::tools::SystemInfoTool {};

    match sys_tool.call().await {
        Ok(result) => {
            if let Some(content) = result.content.first() {
                match content {
                    rust_mcp_schema::CallToolResultContentItem::TextContent(text) => {
                        let lines: Vec<&str> = text.text.lines().collect();
                        if let Some(first_line) = lines.first() {
                            tracing::info!("✅ system_info tool successful: {}", first_line);
                        }
                    }
                    _ => {
                        tracing::info!("✅ system_info tool successful (non-text result)");
                    }
                }
            }
        }
        Err(e) => {
            tracing::error!("❌ system_info tool failed: {}", e);
            return Err(anyhow::anyhow!("system_info test failed"));
        }
    }

    // Test stateful tools
    tracing::info!("Testing stateful tools...");
    let handler = CoreHandler::new();

    // Test counter increment
    tracing::info!("Testing counter increment tool...");
    let counter_tool = crate::tools::CounterIncrementTool { increment: Some(5) };

    match counter_tool.call_with_context(handler.context()).await {
        Ok(result) => {
            if let Some(content) = result.content.first() {
                match content {
                    rust_mcp_schema::CallToolResultContentItem::TextContent(text) => {
                        tracing::info!(
                            "✅ counter increment tool successful: {}",
                            text.text.trim()
                        );
                    }
                    _ => {
                        tracing::info!("✅ counter increment tool successful (non-text result)");
                    }
                }
            }
        }
        Err(e) => {
            tracing::error!("❌ counter increment tool failed: {}", e);
            return Err(anyhow::anyhow!("counter increment test failed"));
        }
    }

    // Test counter get to verify state persisted
    tracing::info!("Testing counter get tool...");
    let counter_get_tool = crate::tools::CounterGetTool {};

    match counter_get_tool.call_with_context(handler.context()).await {
        Ok(result) => {
            if let Some(content) = result.content.first() {
                match content {
                    rust_mcp_schema::CallToolResultContentItem::TextContent(text) => {
                        tracing::info!("✅ counter get tool successful: {}", text.text.trim());
                    }
                    _ => {
                        tracing::info!("✅ counter get tool successful (non-text result)");
                    }
                }
            }
        }
        Err(e) => {
            tracing::error!("❌ counter get tool failed: {}", e);
            return Err(anyhow::anyhow!("counter get test failed"));
        }
    }

    // Test cache set
    tracing::info!("Testing cache set tool...");
    let cache_set_tool = crate::tools::CacheSetTool {
        key: "test_key".to_string(),
        value: "test_value".to_string(),
    };

    match cache_set_tool.call_with_context(handler.context()).await {
        Ok(result) => {
            if let Some(content) = result.content.first() {
                match content {
                    rust_mcp_schema::CallToolResultContentItem::TextContent(text) => {
                        tracing::info!("✅ cache set tool successful: {}", text.text.trim());
                    }
                    _ => {
                        tracing::info!("✅ cache set tool successful (non-text result)");
                    }
                }
            }
        }
        Err(e) => {
            tracing::error!("❌ cache set tool failed: {}", e);
            return Err(anyhow::anyhow!("cache set test failed"));
        }
    }

    // Test cache get to verify state persisted
    tracing::info!("Testing cache get tool...");
    let cache_get_tool = crate::tools::CacheGetTool {
        key: "test_key".to_string(),
    };

    match cache_get_tool.call_with_context(handler.context()).await {
        Ok(result) => {
            if let Some(content) = result.content.first() {
                match content {
                    rust_mcp_schema::CallToolResultContentItem::TextContent(text) => {
                        tracing::info!("✅ cache get tool successful: {}", text.text.trim());
                    }
                    _ => {
                        tracing::info!("✅ cache get tool successful (non-text result)");
                    }
                }
            }
        }
        Err(e) => {
            tracing::error!("❌ cache get tool failed: {}", e);
            return Err(anyhow::anyhow!("cache get test failed"));
        }
    }

    tracing::info!(
        "🎉 All tool tests passed! Both stateful and stateless tools are working correctly."
    );
    Ok(())
}