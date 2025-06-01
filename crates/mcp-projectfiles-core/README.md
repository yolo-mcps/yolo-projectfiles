# projectfiles-core

Core library for the Model Context Protocol (MCP) server implementation using rust-mcp-sdk.

[Parent](../../README.md) | [Specification](SPECIFICATION.md)

## Features

- Built on rust-mcp-sdk and rust-mcp-schema for full MCP compliance
- Modular transport handlers (stdio/SSE) with shared business logic
- Tool system using rust-mcp-sdk macros and patterns
- Clean architecture ready for template-based code generation
- Built-in tool implementations for common operations

## Architecture

### Modular Design

- **CoreHandler**: Transport-agnostic business logic
- **StdioHandler**: Stdio transport using ServerHandler trait
- **SseHandler**: SSE transport using ServerHandler trait
- **ProtocolTools**: Tool collection using rust-mcp-sdk patterns

### Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
projectfiles-core = "0.1.0"
```

### Creating a stdio server

```rust
use projectfiles_core::run_stdio_server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run_stdio_server().await
}
```

### Creating an SSE server

```rust
use projectfiles_core::run_sse_server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run_sse_server(3000).await
}
```

### Using specific handlers

```rust
use projectfiles_core::{StdioHandler, SseHandler, CoreHandler};
use rust_mcp_sdk::{server_runtime, StdioTransport, TransportOptions};

// Stdio handler with custom configuration
let handler = StdioHandler::new();
let transport = StdioTransport::new(TransportOptions::default())?;
let server = server_runtime::create_server(server_details, transport, handler);
server.start().await?;

// SSE handler with custom port
let handler = SseHandler::new();
let server_options = HyperServerOptions {
    host: "0.0.0.0".to_string(),
    port: 8080,
    ..Default::default()
};
let server = hyper_server::create_server(server_details, handler, server_options);
server.start().await?;
```

## Built-in Tools

All tools implemented using rust-mcp-sdk patterns:

- **calculator**: Evaluate mathematical expressions
- **file_read**: Read text file contents
- **file_write**: Write content to a text file
- **file_list**: List files in a directory
- **system_info**: Get OS and architecture information
- **environment**: Read environment variables
- **current_time**: Get current date and time
- **timestamp**: Get Unix timestamp

## Transport Support

- **Stdio**: Full support via rust-mcp-sdk StdioTransport
- **SSE**: Full support via rust-mcp-sdk hyper_server
- **Shared Logic**: Same tools available on both transports

## License

MIT