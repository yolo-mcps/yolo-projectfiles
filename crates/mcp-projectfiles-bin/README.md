# projectfiles-bin

Binary application for the Model Context Protocol (MCP) server.

[Parent](../../README.md) | [Specification](SPECIFICATION.md)

## Installation

```bash
cargo install --path .
```

## Usage

### Stdio Transport

Run the server using stdio for communication:

```bash
projectfiles stdio [OPTIONS]

Options:
  --name <NAME>        Server name [default: projectfiles]
  --version <VERSION>  Server version [default: current version]
  -h, --help           Print help
```

Example:
```bash
projectfiles stdio --name "my-server" --version "1.0.0"
```

### SSE Transport

Run the server using Server-Sent Events over HTTP:

```bash
projectfiles sse [OPTIONS]

Options:
  --name <NAME>        Server name [default: projectfiles]
  --version <VERSION>  Server version [default: current version]
  -p, --port <PORT>    Port to listen on [default: 3000]
  -h, --help           Print help
```

Example:
```bash
projectfiles sse --port 8080
```

## Environment Variables

- `RUST_LOG`: Control logging output
  - `error`: Only show errors
  - `warn`: Show warnings and errors
  - `info`: Show info, warnings, and errors (default)
  - `debug`: Show all log messages
  - `trace`: Show trace-level details

Example:
```bash
RUST_LOG=debug projectfiles stdio
```

## Testing with mcp-discovery

### Stdio mode
```bash
mcp-discovery stdio -- projectfiles stdio
```

### SSE mode
```bash
# Start the server
projectfiles sse --port 3000

# In another terminal
mcp-discovery sse http://localhost:3000
```

## License

MIT