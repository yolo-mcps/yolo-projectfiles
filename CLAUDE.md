# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Objectives

This is an MCP Server designed for allowing an agentic coding assistant to list,
search, grep, find, read, and write files in a project's working directory. The
primary intent is to allow an LLM more access than general Read/Write tools that
might have access to files outside the current project directory.

## Development Commands

### Building and Testing

- `cargo build` - Build all workspace crates
- `cargo test` - Run all tests across the workspace
- `cargo run --bin mcp-projectfiles -- stdio` - Run the server with stdio transport
- `cargo run --bin mcp-projectfiles -- sse --port 3000` - Run the server with SSE transport

### Installation and Development Tasks

- `cargo xtask install` - Install the binary (equivalent to `cargo install --path crates/mcp-projectfiles-bin`)
- `cargo install --path crates/mcp-projectfiles-bin` - Install the mcp-projectfiles binary

### Testing with External Tools

- `mcp-discovery stdio -- cargo run --bin mcp-projectfiles -- stdio` - Test stdio transport with mcp-discovery
- `mcp-discovery sse http://localhost:3000` - Test SSE transport with mcp-discovery

### Environment Variables

- `RUST_LOG=debug` - Enable debug logging
- `NO_COLOR=1` - Disable colored terminal output

## Architecture Overview

This is a Rust workspace implementing an MCP (Model Context Protocol) server with multiple transport support.

### Workspace Structure

- **mcp-projectfiles-core** - Core library containing the protocol implementation, server logic, and built-in tools
- **mcp-projectfiles-bin** - Binary application providing the CLI interface
- **xtask** - Development automation tasks (cargo xtask install)

### Core Components

#### Transport Layer (`crates/mcp-projectfiles-core/src/transports/`)

- **StdioHandler** - Handles stdin/stdout communication
- **SseHandler** - Handles Server-Sent Events over HTTP
- Both transports use the same `CoreHandler` for business logic

#### Core Handler (`crates/mcp-projectfiles-core/src/handler.rs`)

- Transport-agnostic business logic
- Manages tool execution for both stateful and stateless tools
- Uses `ToolContext` for shared state between stateful tools

#### Tool System (`crates/mcp-projectfiles-core/src/tools/`)

- **Stateless tools**: Calculator, File operations, System info, Time utilities
- **Stateful tools**: Counter, Cache (use shared `ToolContext`)
- Tools are defined using the `rust-mcp-sdk` tool_box macro in `tools.rs`
- Individual tool implementations are in separate modules (calculator.rs, file.rs, etc.)

#### Context Management (`crates/mcp-projectfiles-core/src/context.rs`)

- `ToolContext` provides shared state for stateful tools
- Allows tools to persist data between calls (counters, cache)

### Key Design Patterns

1. **Transport Abstraction** - Core handler is transport-agnostic, allowing easy addition of new transports
2. **Tool Trait System** - Tools implement either stateless or `StatefulTool` traits
3. **Shared Context** - Stateful tools share a common context for data persistence
4. **Logging Strategy** - TTY-aware logging (colored for terminals, plain text for files/pipes) with all logs to stderr

### Adding New Tools

To add a new tool:

1. Create the tool struct in `crates/mcp-projectfiles-core/src/tools/`
2. Implement the appropriate trait (stateless or `StatefulTool`)
3. Add the tool to the `tool_box!` macro in `tools.rs`
4. Re-export in the module

### Binary Entry Point

The main binary (`crates/mcp-projectfiles-bin/src/main.rs`) uses clap for CLI parsing and delegates to the appropriate transport server function.
