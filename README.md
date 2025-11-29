# 🚀 Skillz - Self-Extending MCP Server

<div align="center">

[![Crates.io](https://img.shields.io/crates/v/skillz.svg)](https://crates.io/crates/skillz)
[![CI](https://github.com/Algiras/skillz/actions/workflows/ci.yml/badge.svg)](https://github.com/Algiras/skillz/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![MCP](https://img.shields.io/badge/MCP-2024--11--05-blue.svg)](https://modelcontextprotocol.io/)

**Build and execute custom tools at runtime. Let your AI create its own tools.**

[Install](#-installation) •
[Configure](#-editor-configuration) •
[Features](#-features) •
[Examples](#-quick-examples) •
[Documentation](https://algiras.github.io/skillz)

</div>

---

## 🎯 Why Skillz?

Traditional MCP servers have a fixed set of tools. **Skillz lets your AI create new tools on the fly.**

- **Problem**: Need a new capability? Write a server, deploy it, restart your editor.
- **Skillz Solution**: Ask your AI to build the tool. It compiles Rust to WASM or registers a script. Done.

**Example**: "Build me a tool that fetches weather data" → AI writes the code → Tool is instantly available.

No deployments. No restarts. Just ask.

---

## ⚡ Installation

```bash
# Install WASM target (required for building tools)
rustup target add wasm32-wasip1

# Install Skillz from crates.io
cargo install skillz
```

[![Crates.io](https://img.shields.io/crates/v/skillz.svg)](https://crates.io/crates/skillz)
[![Downloads](https://img.shields.io/crates/d/skillz.svg)](https://crates.io/crates/skillz)

Or build from source:
```bash
git clone https://github.com/Algiras/skillz.git
cd skillz/mcp-wasm-host
cargo install --path .
```

---

## ☕ Support Skillz

If you find Skillz useful, please consider supporting its development:

[![Buy Me a Coffee](https://img.shields.io/badge/Buy%20Me%20a%20Coffee-ffdd00?style=for-the-badge&logo=buy-me-a-coffee&logoColor=black)](https://buymeacoffee.com/algiras)

Your support enables new features and improvements!

---

## 🔧 Editor Configuration

### Cursor IDE

Add to `~/.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "skillz": {
      "command": "skillz"
    }
  }
}
```

### Claude Desktop

| Platform | Config Location |
|----------|-----------------|
| macOS | `~/Library/Application Support/Claude/claude_desktop_config.json` |
| Windows | `%APPDATA%\Claude\claude_desktop_config.json` |
| Linux | `~/.config/Claude/claude_desktop_config.json` |

```json
{
  "mcpServers": {
    "skillz": {
      "command": "skillz"
    }
  }
}
```

> **Note**: If `skillz` isn't in your PATH, use: `~/.cargo/bin/skillz`

### HTTP Server Mode *(v0.4.0+)*

Run Skillz as an HTTP server for web integrations:

```bash
# Start HTTP server on port 8080
skillz --transport http --port 8080

# Custom host binding
skillz --transport http --host 0.0.0.0 --port 3000
```

**Endpoints:**
- `GET /sse` - Server-Sent Events stream for real-time updates
- `POST /message` - Send JSON-RPC messages

**Connect with curl:**
```bash
# Establish SSE connection
curl -N http://localhost:8080/sse -H 'Accept: text/event-stream'

# Send a message (in another terminal)
curl -X POST http://localhost:8080/message \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

---

## 🎯 Features

| Feature | Description |
|---------|-------------|
| 🦀 **WASM Tools** | Compile Rust → WebAssembly at runtime |
| 📦 **Rust Crates** | Add serde, regex, anyhow, etc. to WASM tools! |
| 📜 **Script Tools** | Python, Node.js, Ruby, Bash, or any language |
| 🏷️ **Tool Annotations** | Hints for clients (readOnly, destructive, idempotent) |
| 🔍 **Completion API** | Autocomplete for tool arguments |
| ⚡ **Code Execution** | Compose multiple tools via code (98% token savings!) |
| 📦 **Dependencies** | Auto-install pip/npm/cargo packages per tool |
| 💾 **Persistence** | Tools survive server restarts |
| 🔒 **Sandbox** | Optional bubblewrap/firejail/nsjail isolation |
| 📂 **Shareable** | Each tool has its own directory with manifest.json |
| 📖 **Dynamic Guide** | Built-in `skillz://guide` resource updates automatically |
| 🌐 **Tool Import** | Import tools from GitHub repos or Gists |
| ⛓️ **Pipelines** | Chain tools together declaratively *(v0.3.0+)* |
| 🌍 **HTTP Transport** | Run as HTTP server with SSE for web apps *(v0.4.0+)* |

---

## 📖 Available Tools

| Tool | Description |
|------|-------------|
| `build_tool` | Compile Rust code → WASM tool |
| `register_script` | Register script with optional deps & annotations |
| `create_skill` | Step-by-step guided creation |
| `import_tool` | Import tools from git repos or GitHub gists |
| `create_pipeline` | Create a pipeline that chains tools together *(v0.3.0+)* |
| `list_pipelines` | List all pipeline tools *(v0.3.0+)* |
| `call_tool` | Execute any tool (WASM, Script, or Pipeline) |
| `list_tools` | List all available tools |
| `complete` | Get autocomplete suggestions for arguments |
| `execute_code` | Run code that composes multiple tools |
| `install_deps` | Install dependencies for a tool |
| `delete_tool` | Remove a tool and clean up |
| `test_validate` | Validate Rust code before building |

---

## 💡 Quick Examples

### Build a WASM Tool (Rust)

```
build_tool(
  name: "fibonacci",
  description: "Generates Fibonacci numbers",
  code: "fn main() { 
    let (mut a, mut b) = (0u64, 1); 
    for _ in 0..20 { print!(\"{} \", a); (a, b) = (b, a + b); } 
  }",
  annotations: {"readOnlyHint": true}
)
```

### WASM Tool with Rust Dependencies

```
build_tool(
  name: "json_processor",
  description: "Process JSON with serde",
  dependencies: ["serde@1.0[derive]", "serde_json@1.0"],
  code: """
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
struct Data { name: String, value: i32 }

fn main() {
    let data = Data { name: "test".to_string(), value: 42 };
    println!("{}", serde_json::to_string(&data).unwrap());
}
"""
)
```

### Register a Script Tool (Python)

```
register_script(
  name: "word_counter",
  description: "Counts words in text",
  interpreter: "python3",
  input_schema: {
    "type": "object",
    "properties": {"text": {"type": "string"}},
    "required": ["text"]
  },
  output_schema: {
    "type": "object",
    "properties": {"count": {"type": "integer"}}
  },
  annotations: {"readOnlyHint": true},
  code: """#!/usr/bin/env python3
import json, sys

request = json.loads(sys.stdin.readline())
text = request['params']['arguments'].get('text', '')
result = {'count': len(text.split())}
print(json.dumps({'jsonrpc': '2.0', 'result': result, 'id': request['id']}))
"""
)
```

### Tool with Dependencies

```
register_script(
  name: "data_analyzer",
  interpreter: "python3",
  dependencies: ["pandas", "numpy"],
  code: "..."
)
```

### Import Tools from GitHub

```
# Import from a git repository
import_tool(
  source: "https://github.com/user/skillz-json-tools"
)

# Import from a specific branch
import_tool(
  source: "https://github.com/user/repo#main"
)

# Import from a GitHub Gist
import_tool(
  source: "gist:abc123def456"
)

# Or with full gist URL
import_tool(
  source: "https://gist.github.com/user/abc123def456"
)
```

### Create a Pipeline (Chain Tools)

```
# Pipelines are tools that chain other tools together!
create_pipeline(
  name: "process_data",
  description: "Fetch, transform, and format data",
  steps: [
    { name: "fetch", tool: "http_get", args: { url: "$input.url" } },
    { tool: "json_parse", args: { text: "$fetch.body" } },
    { tool: "format_report", args: { data: "$prev" } }
  ]
)

# Run the pipeline like any other tool
call_tool(tool_name: "process_data", arguments: { url: "https://api.example.com/data" })
```

**Variable syntax in pipelines:**
- `$input.field` - Access pipeline input
- `$prev` - Previous step's entire output
- `$prev.field` - Access field from previous step
- `$step_name.field` - Access field from a named step

### Execute Multiple Tools via Code

```
execute_code(
  language: "python",
  code: """
# All registered tools are available as functions!
fib = fibonacci()
primes = prime_finder()

# Combine results with custom logic
print(f"Fibonacci: {fib}")
print(f"Primes: {primes}")

# Loops, conditionals, anything you need
for i in range(3):
    result = my_calculator(a=i, b=10)
    print(result)
"""
)
```

This pattern reduces token usage by **98%** (150k → 2k tokens)!

---

## 📝 Writing Script Tools

### JSON-RPC 2.0 Protocol

Scripts communicate via JSON-RPC 2.0 over stdin/stdout:

<div align="center">
  <img src="docs/architecture.png" alt="Skillz Architecture Diagram" width="800">
</div>

### ⚠️ Important: Use `readline()` not `read()`

```python
# ✅ CORRECT - Returns immediately after reading the request
request = json.loads(sys.stdin.readline())

# ❌ WRONG - Blocks waiting for EOF, causes timeout!
request = json.loads(sys.stdin.read())
```

### Python Template

```python
#!/usr/bin/env python3
import json
import sys

def main():
    # Read JSON-RPC request (use readline!)
    request = json.loads(sys.stdin.readline())
    
    # Extract arguments
    args = request.get('params', {}).get('arguments', {})
    context = request.get('params', {}).get('context', {})
    
    # Your logic here
    result = {"message": "Hello!", "input": args}
    
    # Return JSON-RPC response
    response = {
        "jsonrpc": "2.0",
        "result": result,
        "id": request.get("id")
    }
    print(json.dumps(response))
    sys.stdout.flush()

if __name__ == "__main__":
    main()
```

### Node.js Template

```javascript
#!/usr/bin/env node
const readline = require('readline');

const rl = readline.createInterface({
  input: process.stdin,
  output: process.stdout,
  terminal: false
});

rl.on('line', (line) => {
  const request = JSON.parse(line);
  const args = request.params?.arguments || {};
  
  // Your logic here
  const result = { message: "Hello from Node!", input: args };
  
  console.log(JSON.stringify({
    jsonrpc: "2.0",
    result: result,
    id: request.id
  }));
  
  process.exit(0);
});
```

### Execution Context

Scripts receive context information:

```json
{
  "jsonrpc": "2.0",
  "method": "execute",
  "params": {
    "arguments": { "your": "args" },
    "context": {
      "roots": ["/path/to/workspace"],
      "working_directory": "/current/dir",
      "tool_name": "my_tool",
      "tools_dir": "~/.skillz/tools",
      "capabilities": {
        "sampling": false,
        "elicitation": false
      }
    }
  },
  "id": 1
}
```

---

## 🏷️ Tool Annotations

Annotations help clients understand tool behavior:

```json
{
  "title": "File Reader",
  "readOnlyHint": true,
  "destructiveHint": false,
  "idempotentHint": true,
  "openWorldHint": false
}
```

| Annotation | Meaning |
|------------|---------|
| `title` | Human-readable display name |
| `readOnlyHint` | Tool only reads, doesn't modify state |
| `destructiveHint` | Tool may delete or overwrite data |
| `idempotentHint` | Safe to retry with same arguments |
| `openWorldHint` | Interacts with external systems (network, APIs) |

---

## 📂 Tool Directory Structure

Each tool is stored in its own directory with a shareable `manifest.json`:

```
~/tools/
├── fibonacci/
│   ├── manifest.json     # Tool metadata
│   ├── fibonacci.wasm    # Compiled binary
│   └── src.rs            # Source code (for recompilation)
├── word_counter/
│   ├── manifest.json
│   └── word_counter.py
└── http_client/
    ├── manifest.json
    ├── http_client.py
    └── env/              # Virtual environment
```

### manifest.json Example

```json
{
  "name": "json_processor",
  "version": "1.0.0",
  "description": "Process JSON with serde",
  "tool_type": "wasm",
  "wasm_dependencies": ["serde@1.0[derive]", "serde_json@1.0"],
  "author": "Your Name",
  "license": "MIT",
  "tags": ["json", "utility"]
}
```

Tools are **shareable** - just copy the directory!

---

## 🔧 Environment Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `TOOLS_DIR` | Where tools are stored | `~/.skillz/tools` |
| `SKILLZ_ROOTS` | Workspace roots (colon-separated) | `/home/user/project:/data` |
| `SKILLZ_SANDBOX` | Sandbox mode | `bubblewrap`, `firejail`, `nsjail` |
| `SKILLZ_SANDBOX_NETWORK` | Allow network in sandbox | `1` |

**Root Priority:** MCP client roots > `SKILLZ_ROOTS` env > cwd

---

## 🔒 Security

### Sandbox Modes (Linux)

Enable sandboxing via environment variable:

```bash
# Bubblewrap (namespace isolation)
export SKILLZ_SANDBOX=bubblewrap

# Firejail (seccomp + namespaces)
export SKILLZ_SANDBOX=firejail

# nsjail (most restrictive)
export SKILLZ_SANDBOX=nsjail

# Allow network in sandbox
export SKILLZ_SANDBOX_NETWORK=1
```

See [SECURITY.md](SECURITY.md) for full details.

---

## 🛠️ Development

```bash
git clone https://github.com/Algiras/skillz.git
cd skillz
cargo build --release
cargo test
```

---

## 🤝 Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

---

## 📄 License

MIT License - see [LICENSE](LICENSE)

---

## 🔗 Links

- **Crates.io**: [crates.io/crates/skillz](https://crates.io/crates/skillz)
- **Docs**: [algiras.github.io/skillz](https://algiras.github.io/skillz)
- **GitHub**: [github.com/Algiras/skillz](https://github.com/Algiras/skillz)
- **MCP Spec**: [modelcontextprotocol.io](https://modelcontextprotocol.io)

---

<div align="center">

**Built with ❤️ for AI-powered development**

</div>
