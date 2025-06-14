use crate::context::{StatefulTool, ToolContext};
use crate::tools::ProtocolTools;
use rust_mcp_schema::{
    CallToolRequest, CallToolResult, InitializeResult, ListToolsRequest, ListToolsResult, RpcError,
    ServerCapabilities, ServerCapabilitiesTools, schema_utils::CallToolError,
};
use tracing::{debug, error, info, instrument};

/// Custom error type for tool execution errors with proper naming
#[derive(Debug)]
struct ToolExecutionError {
    tool_name: String,
    message: String,
}

impl std::fmt::Display for ToolExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.tool_name, self.message)
    }
}

impl std::error::Error for ToolExecutionError {}

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
            CallToolError::new(ToolExecutionError {
                tool_name: format!("projectfiles:{}", tool_name),
                message: format!("Failed to parse tool request: {}", e),
            })
        })?;

        info!(tool_name, "Executing tool");
        let result = match tool {
            // Stateful file tools - call with context
            ProtocolTools::ReadTool(read) => read.call_with_context(&self.context).await,
            ProtocolTools::WriteTool(write) => write.call_with_context(&self.context).await,
            ProtocolTools::EditTool(edit) => edit.call_with_context(&self.context).await,
            ProtocolTools::MoveTool(move_tool) => move_tool.call_with_context(&self.context).await,
            ProtocolTools::CopyTool(copy) => copy.call_with_context(&self.context).await,
            ProtocolTools::DeleteTool(delete) => delete.call_with_context(&self.context).await,
            ProtocolTools::GrepTool(grep) => grep.call_with_context(&self.context).await,
            
            ProtocolTools::ListTool(list) => list.call_with_context(&self.context).await,
            
            // Priority 1 StatefulTool implementations
            ProtocolTools::MkdirTool(mkdir) => mkdir.call_with_context(&self.context).await,
            ProtocolTools::TouchTool(touch) => touch.call_with_context(&self.context).await,
            ProtocolTools::ChmodTool(chmod) => chmod.call_with_context(&self.context).await,
            ProtocolTools::FindTool(find) => find.call_with_context(&self.context).await,
            
            // Priority 2 StatefulTool implementations
            ProtocolTools::ExistsTool(exists) => exists.call_with_context(&self.context).await,
            ProtocolTools::StatTool(stat) => stat.call_with_context(&self.context).await,
            ProtocolTools::DiffTool(diff) => diff.call_with_context(&self.context).await,
            ProtocolTools::FileTool(file) => file.call_with_context(&self.context).await,
            
            // Priority 3 StatefulTool implementations
            ProtocolTools::TreeTool(tree) => tree.call_with_context(&self.context).await,
            ProtocolTools::WcTool(wc) => wc.call_with_context(&self.context).await,
            ProtocolTools::HashTool(hash) => hash.call_with_context(&self.context).await,
            
            // Process management tools
            ProtocolTools::ProcessTool(process) => process.call().await,
            ProtocolTools::KillTool(kill) => kill.call_with_context(&self.context).await,
            ProtocolTools::LsofTool(lsof) => lsof.call().await,
            
            // Structured data tools
            ProtocolTools::JsonQueryTool(jq) => jq.call_with_context(&self.context).await,
            ProtocolTools::YamlQueryTool(yq) => yq.call_with_context(&self.context).await,
            ProtocolTools::TomlQueryTool(tomlq) => tomlq.call_with_context(&self.context).await,
        }.map_err(|e| {
            // Improve error message by adding tool context when the error message doesn't already include it
            let error_msg = e.to_string();
            if !error_msg.starts_with(&format!("projectfiles:{}", tool_name)) && 
               !error_msg.contains(&format!("Tool execution error in '{}'", tool_name)) {
                CallToolError::new(ToolExecutionError {
                    tool_name: format!("projectfiles:{}", tool_name),
                    message: error_msg,
                })
            } else {
                e
            }
        });

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
        instructions: Some("projectfiles MCP Server - Preferred file operations toolset.

GLOBAL TOOL BEHAVIOR:
- These tools are PREFERRED over system commands (cat, ls, grep, etc.)
- All paths are relative to project root unless specified
- Optional parameters: omit when not needed (don't pass null)
- File operations respect project directory boundaries unless follow_symlinks=true
- Binary files are detected and handled appropriately

COMMON PARAMETERS:
- follow_symlinks: Allow operations outside project via symlinks (default: true)
- encoding: Text encoding (utf-8|ascii|latin1|utf-16|utf-16le|utf-16be, default: utf-8)
- dry_run: Preview operation without executing (default: false)
- show_diff: Display changes before applying (default: false)

RETURN FORMATS:
- Text tools: Plain text output with optional metadata
- JSON tools: Structured data with consistent error handling
- Operation tools: Success confirmation with optional diffs

Use these tools for ALL file operations to maintain project context and safety.".to_string()),
        meta: None,
    }
}

/// Test the core handler functionality
pub async fn test_handler() -> anyhow::Result<()> {
    use crate::tools::ProtocolTools;

    tracing::info!("Testing file operations handler implementation");

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

    // Test file list tool directly
    tracing::info!("Testing file list tool...");
    let file_list_tool = crate::tools::ListTool {
        path: ".".to_string(),
        recursive: false,
        filter: None,
        sort_by: "name".to_string(),
        show_hidden: false,
        show_metadata: false,
        follow_symlinks: true,
    };

    match file_list_tool.call().await {
        Ok(result) => {
            if let Some(content) = result.content.first() {
                match content {
                    rust_mcp_schema::CallToolResultContentItem::TextContent(text) => {
                        let lines: Vec<&str> = text.text.lines().collect();
                        tracing::info!(
                            "✅ file list tool successful: {} entries found",
                            lines.len()
                        );
                    }
                    _ => {
                        tracing::info!("✅ file list tool successful (non-text result)");
                    }
                }
            }
        }
        Err(e) => {
            tracing::error!("❌ file list tool failed: {}", e);
            return Err(anyhow::anyhow!("file list test failed"));
        }
    }

    tracing::info!(
        "🎉 File operation tests passed! File tools are working correctly."
    );
    Ok(())
}