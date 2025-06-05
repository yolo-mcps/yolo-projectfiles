# Mcp Projectfiles: xtask

[Parent](../README.md)

## Usage

This is a development task runner for the yolo-systemtools workspace.

### Install Commands

Install all binary crates:
```bash
cargo xtask install
```

Install a specific crate:
```bash
cargo xtask install mcp-projectfiles-bin
cargo xtask install yolo-projectfiles
cargo xtask install yolo-homefiles
cargo xtask install yolo-executioner
cargo xtask install yolo-terminator
cargo xtask install yolo-memento
```

### Available Binary Crates

- **mcp-projectfiles-bin** - Original MCP server for project file operations
- **yolo-projectfiles** - MCP server for project file operations
- **yolo-homefiles** - MCP server for home directory file operations
- **yolo-executioner** - MCP server for process execution and management
- **yolo-terminator** - MCP server for process termination and cleanup
- **yolo-memento** - MCP server for state persistence and memory