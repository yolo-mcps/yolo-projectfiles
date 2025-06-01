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
        instructions: Some(format!("{} MCP Server with tools support", name)),
        meta: None,
    }
}