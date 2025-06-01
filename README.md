# projectfiles

A Rust implementation of the Model Context Protocol (MCP) server with support for both stdio and SSE transports.

## Features

- **Multiple Transports**: Support for both stdio and Server-Sent Events (SSE) transports
- **Intelligent Logging**: TTY-aware logging with ANSI colors for terminals, plain text for files/pipes
- **Extensible Tool System**: Easy-to-use trait-based system for adding new tools
- **Built-in Tools**:
  - Calculator: Evaluate mathematical expressions
  - File operations: Read, write, and list files
  - System info: Get OS and environment information
  - Time utilities: Get current time and Unix timestamps
- **Type-safe**: Leverages Rust's type system for safe protocol implementation
- **Async/await**: Built on Tokio for efficient async I/O

## Installation

```bash
cargo install --path crates/projectfiles-bin
```

## Usage

### Running with stdio transport

```bash
projectfiles stdio
```

### Running with SSE transport

```bash
projectfiles sse --port 3000
```

### Environment Variables

- `RUST_LOG`: Set logging level (e.g., `RUST_LOG=debug`)
- `NO_COLOR`: Disable colors in terminal output (e.g., `NO_COLOR=1`)

### Logging

The application uses intelligent TTY detection:
- **Terminal**: Colored output for interactive use
- **Files/Pipes**: Plain text without ANSI codes for log aggregation
- **All logs**: Output to stderr to preserve MCP stdin/stdout protocol

See [LOGGING.md](LOGGING.md) for detailed logging documentation.

## Architecture

The project is organized as a Rust workspace with the following crates:

- **[projectfiles-core](crates/projectfiles-core/README.md)**: Core library with protocol implementation, server logic, and built-in tools
- **[projectfiles-bin](crates/projectfiles-bin/README.md)**: Binary application providing the CLI interface
- **xtask**: Development automation tasks

## Development

### Building

```bash
cargo build
```

### Running tests

```bash
cargo test
```

### Adding new tools

To add a new tool, implement the `ToolHandler` trait:

```rust
use async_trait::async_trait;
use projectfiles_core::{ToolHandler, ToolContent, Result};
use serde_json::Value;

pub struct MyTool;

#[async_trait]
impl ToolHandler for MyTool {
    fn name(&self) -> &str {
        "my_tool"
    }
    
    fn description(&self) -> &str {
        "Description of my tool"
    }
    
    fn input_schema(&self) -> Value {
        // JSON Schema for tool inputs
    }
    
    async fn execute(&self, arguments: Option<Value>) -> Result<Vec<ToolContent>> {
        // Tool implementation
    }
}
```

Then register it in the server:

```rust
server.register_tool(MyTool);
```

## Testing with mcp-discovery

Use mcp-discovery to explore and test the server's capabilities:

```bash
# For stdio transport
mcp-discovery stdio -- projectfiles stdio

# For SSE transport
mcp-discovery sse http://localhost:3000
```

## Specification

See [SPECIFICATION.md](SPECIFICATION.md) for protocol details.

## License

MIT