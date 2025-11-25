# ahma_list_tools

CLI utility that connects to an MCP server and dumps all tool information to the terminal in a human-readable text format. This is useful for tests, development, and verifying MCP server tool definitions.

## Usage

### Connect via command-line arguments (stdio mode)

```bash
# Pass the MCP server command directly
ahma_list_tools -- /path/to/ahma_mcp --tools-dir ./tools

# With additional arguments
ahma_list_tools -- /path/to/ahma_mcp --mode stdio --tools-dir /path/to/tools
```

### Connect via mcp.json configuration file

```bash
# Use a server from mcp.json (defaults to first server)
ahma_list_tools --mcp-config /path/to/mcp.json

# Specify which server to use from mcp.json
ahma_list_tools --mcp-config /path/to/mcp.json --server Ahma
```

### Connect to HTTP MCP server

```bash
# Connect to HTTP MCP server
ahma_list_tools --http http://localhost:3000
```

## Output Format

The tool outputs all tool information in a structured text format:

```text
MCP Server Tools
================

Server: Ahma
Version: 0.6.0

Tool: cargo_build
  Description: Build a Rust project
  Parameters:
    - working_directory (string, optional): Working directory for command execution
    - release (boolean, optional): Build in release mode

Tool: git_status
  Description: Show the working tree status
  Parameters:
    - short (boolean, optional): Give the output in short format
...
```

## Exit Codes

- `0`: Success
- `1`: Connection error
- `2`: Configuration error
