<div align="center">

# 🚀 Skillz - Self-Extending MCP Server

<img src="docs/hero-banner.png" alt="Skillz Hero Banner" width="100%">

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

<div align="center">
<table>
<tr>
<td width="50%" align="center">
  
### ❌ Traditional MCP
  
🔒 **Fixed Tool Set**  
Write server code

⬆️ **Deploy & Restart**  
Restart your editor

⏱️ **Time Consuming**  
Manual process

</td>
<td width="50%" align="center">
  
### ✅ Skillz

🔧 **Dynamic Tools**  
AI writes the code

⚡ **Instant**  
Compiles to WASM/Script

🚀 **Zero Downtime**  
No restarts needed

</td>
</tr>
</table>
</div>

**Example**: "Build me a tool that fetches weather data" → AI writes the code → Tool is instantly available.

No deployments. No restarts. Just ask.

---

## ⚡ Installation

<div align="center">

[![Crates.io](https://img.shields.io/crates/v/skillz.svg)](https://crates.io/crates/skillz)
[![Downloads](https://img.shields.io/crates/d/skillz.svg)](https://crates.io/crates/skillz)

</div>

```bash
# Install WASM target (required for building tools)
rustup target add wasm32-wasip1

# Install Skillz from crates.io
cargo install skillz
```

**Or build from source:**

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

# Enable hot reload (watch tools directory for changes)
skillz --hot-reload

# HTTP server with hot reload
skillz --transport http --port 8080 --hot-reload
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

<div align="center">

### 🌟 Core Capabilities

</div>

| Feature | Description |
|---------|-------------|
| 🦀 **WASM Tools** | Compile Rust → WebAssembly at runtime |
| 📦 **Rust Crates** | Add serde, regex, anyhow, etc. to WASM tools! |
| 📜 **Script Tools** | Python, Node.js, Ruby, Bash, or any language |
| 🏷️ **Tool Annotations** | Hints for clients (readOnly, destructive, idempotent) |
| ⚡ **Code Execution** | Compose multiple tools via code (98% token savings!) |
| 📦 **Dependencies** | Auto-install pip/npm/cargo packages per tool |
| 💾 **Persistence** | Tools survive server restarts |
| 🔒 **Sandbox** | Optional bubblewrap/firejail/nsjail isolation |
| 📂 **Shareable** | Each tool has its own directory with manifest.json |
| 📖 **Dynamic Guide** | Built-in `skillz://guide` resource updates automatically |
| 🌐 **Tool Import** | Import tools from GitHub repos or Gists |
| ⛓️ **Pipelines** | Chain tools together declaratively |
| 🌍 **HTTP Transport** | Run as HTTP server with SSE for web apps |
| 💬 **Elicitation** | Scripts can request user input via MCP protocol |
| 🧠 **Memory** | Persistent key-value storage for tools |
| 📊 **Logging/Progress** | Scripts can send logs and progress updates |
| 🔥 **Hot Reload** | Watch tools directory, auto-reload on changes |

---

## 📖 Available Tools (9 Core)

| Tool | Description |
|------|-------------|
| `build_tool` | Compile Rust code → WASM tool (with crate dependencies) |
| `register_script` | Register script tool (Python, Node.js, etc.) with deps |
| `call_tool` | Execute any tool (WASM, Script, or Pipeline) |
| `list_tools` | List all available tools |
| `delete_tool` | Remove a tool and clean up |
| `import_tool` | Import tools from Git repos or GitHub Gists |
| `execute_code` | Run code that composes multiple tools |
| `pipeline` | Create, list, delete pipeline tools (action-based) |
| `memory` | Persistent storage for tools (store, get, list, delete, stats) |

---

## 💡 Quick Examples

<div align="center">

### 🔥 See Skillz in Action

</div>

### 🦀 Build a WASM Tool (Rust)

```rust
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

### 🦀 WASM Tool with Rust Dependencies

```rust
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

### 🐍 Register a Script Tool (Python)

```python
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

### 📦 Tool with Dependencies

```python
register_script(
  name: "data_analyzer",
  interpreter: "python3",
  dependencies: ["pandas", "numpy"],
  code: "..."
)
```

### 🌐 Import Tools from GitHub

```bash
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

### ⛓️ Create a Pipeline (Chain Tools)

```yaml
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

<details>
<summary><b>📘 Variable syntax in pipelines</b></summary>

- `$input.field` - Access pipeline input
- `$prev` - Previous step's entire output
- `$prev.field` - Access field from previous step
- `$step_name.field` - Access field from a named step

</details>

### ⚡ Execute Multiple Tools via Code

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

### 🏗️ Architecture Overview

<img src="docs/architecture.png" alt="Skillz Architecture Diagram" width="800">

*The diagram shows how scripts communicate with the Skillz host using JSON-RPC 2.0 protocol over stdin/stdout*

</div>

### ⚠️ Important: Use `readline()` not `read()`

> **⚠️ WARNING**  
> Always use `readline()` to read JSON-RPC requests. Using `read()` will block waiting for EOF and cause timeouts!

```python
# ✅ CORRECT - Returns immediately after reading the request
request = json.loads(sys.stdin.readline())

# ❌ WRONG - Blocks waiting for EOF, causes timeout!
request = json.loads(sys.stdin.read())
```

### 🐍 Python Template

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

### 📗 Node.js Template

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

<div align="center">

```tree
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

💡 **Tools are fully shareable** - just copy the directory!

</div>

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

<div align="center">

### Platform Support

![Linux](https://img.shields.io/badge/Linux-FCC624?style=flat&logo=linux&logoColor=black)
![macOS](https://img.shields.io/badge/macOS-000000?style=flat&logo=apple&logoColor=white)
![Windows](https://img.shields.io/badge/Windows-0078D6?style=flat&logo=windows&logoColor=white)

</div>

### 🛡️ Sandbox Modes (Linux Only)

Enable sandboxing via environment variable:

<table>
<tr>
<th>Sandbox</th>
<th>Security Level</th>
<th>Features</th>
</tr>
<tr>
<td>🟢 <b>bubblewrap</b></td>
<td>Medium</td>
<td>Namespace isolation</td>
</tr>
<tr>
<td>🟡 <b>firejail</b></td>
<td>High</td>
<td>seccomp + namespaces</td>
</tr>
<tr>
<td>🔴 <b>nsjail</b></td>
<td>Very High</td>
<td>Most restrictive</td>
</tr>
</table>

**Configuration:**

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

> 📖 See [SECURITY.md](SECURITY.md) for full details.

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

<div align="center">

| Resource | Link |
|----------|------|
| 📦 **Crates.io** | [crates.io/crates/skillz](https://crates.io/crates/skillz) |
| 📖 **Documentation** | [algiras.github.io/skillz](https://algiras.github.io/skillz) |
| 💻 **GitHub** | [github.com/Algiras/skillz](https://github.com/Algiras/skillz) |
| 📋 **MCP Spec** | [modelcontextprotocol.io](https://modelcontextprotocol.io) |

---

**Built with ❤️ for AI-powered development**

[![Star on GitHub](https://img.shields.io/github/stars/Algiras/skillz?style=social)](https://github.com/Algiras/skillz)

</div>
