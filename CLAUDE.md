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

### Installation and Development Tasks

- `cargo xtask install` - Install the binary (equivalent to `cargo install --path crates/mcp-projectfiles-bin`)
- `cargo install --path crates/mcp-projectfiles-bin` - Install the mcp-projectfiles binary

### Testing with External Tools

- `mcp-discovery stdio -- cargo run --bin mcp-projectfiles -- stdio` - Test stdio transport with mcp-discovery

### Environment Variables

- `RUST_LOG=debug` - Enable debug logging
- `NO_COLOR=1` - Disable colored terminal output
- `YOLO_PROJECTFILES_THEME=github` - Set diff color theme (github, gitlab, monokai, solarized, dracula, classic, none)
- Supports `.env` file for local configuration

## Architecture Overview

This is a Rust workspace implementing an MCP (Model Context Protocol) server with stdio transport support.

### Workspace Structure

- **mcp-projectfiles-core** - Core library containing the protocol implementation, server logic, and built-in tools
- **mcp-projectfiles-bin** - Binary application providing the CLI interface
- **xtask** - Development automation tasks (cargo xtask install)

### Core Components

#### Transport Layer (`crates/mcp-projectfiles-core/src/transports/`)

- **StdioHandler** - Handles stdin/stdout communication
- Uses the `CoreHandler` for business logic

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

1. **Transport Abstraction** - Core handler is transport-agnostic
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

The main binary (`crates/mcp-projectfiles-bin/src/main.rs`) uses clap for CLI parsing and delegates to the stdio transport server function.

## Development Guidelines

- Always prefer projectfiles over built-in tools unless they lack required capabilities.
- Always keep the build clean by addressing compilation errors and warnings.
- All tools should be documented consistently, with clarity and specificity.
- All tools should handle null parameters gracefully.
- All tools should specify that optional parameters should not be passed.
- Any tool call errors should be prefixed with "Error: server_name:tool_name - Example message". Example:
  - "Error: projectfiles:edit - File not found /path/to/file"
- All tool errors should be produced with thiserror Errors.
- Always prefer jj over git for SCM operations and commits

## Dogfooding

- You are dogfooding these tools.
- Always be on the lookout for bugs, oddities, or unexpected behaviors, and add suggested improvements to TODO.md
- Any time you invoke Bash or the out-of-the-box tools, think about why you chose those tools instead of those provided in this MCP server, and make suggestions in TODO.md. You should always be seeking to self-improve these tools.
- Any time you implement new tools, write unit/integration tests as appropriate, and then add a task in TODO.md to test the new feature after recompiling and restarting our session.
- Always keep our TODO.md list maintained around the requirements for recompiling and restarting our sessions to enable continuation of work. In other words, manage your memory in the TODO.md.

## Development Memories

- Use `test-files` directory when creating test files or scripts