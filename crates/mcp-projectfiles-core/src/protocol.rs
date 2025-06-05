// Re-export all rust-mcp-schema types
pub use rust_mcp_schema::{
    // Core protocol types
    CallToolRequest,
    CallToolResult,
    CallToolResultContentItem,
    ClientCapabilities,
    ImageContent,

    // Utility types
    Implementation,
    InitializeRequest,
    InitializeRequestParams,
    InitializeResult,
    ListToolsRequest,
    ListToolsResult,
    PingRequest,

    RequestId,

    Result as SchemaResult,

    // Error types
    RpcError,
    // Schema and capability types
    ServerCapabilities,
    ServerCapabilitiesTools,
    TextContent,
    Tool,
    ToolInputSchema,
    // Schema utilities
    schema_utils::{
        CallToolError, ClientMessage, MessageFromServer, NotificationFromClient, RequestFromClient,
        ServerMessage,
    },
};

// Constants
pub const PROTOCOL_VERSION: &str = "2025-03-26";

// Helper function to create server info
pub fn create_server_info(name: &str, version: &str) -> Implementation {
    Implementation {
        name: name.to_string(),
        version: version.to_string(),
    }
}

// Helper function to create server capabilities with tools
pub fn create_server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        tools: Some(ServerCapabilitiesTools {
            list_changed: Some(true),
        }),
        completions: None,
        experimental: None,
        logging: None,
        prompts: None,
        resources: None,
    }
}

// Helper function to create InitializeResult
pub fn create_initialize_result(name: &str, version: &str) -> InitializeResult {
    InitializeResult {
        protocol_version: PROTOCOL_VERSION.to_string(),
        capabilities: create_server_capabilities(),
        server_info: create_server_info(name, version),
        instructions: Some(format!(
            "{} MCP Server - Preferred file operations toolset.

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

Use these tools for ALL file operations within the project to maintain project context and safety.",
            name
        )),
        meta: None,
    }
}

