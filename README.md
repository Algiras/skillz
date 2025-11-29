# 🚀 Skillz - Self-Extending MCP Server

<div align="center">

[![CI](https://github.com/Algiras/skillz/actions/workflows/ci.yml/badge.svg)](https://github.com/Algiras/skillz/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![MCP](https://img.shields.io/badge/MCP-2024--11--05-blue.svg)](https://modelcontextprotocol.io/)

**Build and execute custom tools at runtime. Let your AI create its own tools.**

[Quick Install](#-quick-install) •
[Features](#-features) •
[Documentation](#-documentation) •
[Examples](#-examples) •
[Contributing](#-contributing)

</div>

---

## ⚡ Quick Install

### One-Click Install for Editors

<table>
<tr>
<td align="center" width="33%">

### 🖥️ Cursor

[![Install in Cursor](https://img.shields.io/badge/Install-Cursor-blue?style=for-the-badge&logo=cursor)](cursor://settings/mcp)

</td>
<td align="center" width="33%">

### 🤖 Claude Desktop

[![Install in Claude](https://img.shields.io/badge/Install-Claude_Desktop-orange?style=for-the-badge&logo=anthropic)](https://claude.ai/download)

</td>
<td align="center" width="33%">

### 💻 VS Code

[![Install in VS Code](https://img.shields.io/badge/Install-VS_Code-007ACC?style=for-the-badge&logo=visualstudiocode)](vscode:extension/anthropic.claude-mcp)

</td>
</tr>
</table>

### Install from Source

```bash
# 1. Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. Add WASM target
rustup target add wasm32-wasip1

# 3. Clone and build
git clone https://github.com/Algiras/skillz.git
cd skillz
cargo build --release

# 4. The binary is at: ./target/release/skillz
```

---

## 🔧 Editor Configuration

### Cursor IDE

Add to `~/.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "skillz": {
      "command": "/absolute/path/to/skillz",
      "args": [],
      "env": {
        "TOOLS_DIR": "~/skillz-tools",
        "PATH": "/usr/local/bin:/usr/bin:/bin:~/.cargo/bin"
      }
    }
  }
}
```

Then restart Cursor or run `Developer: Reload Window`.

### Claude Desktop

**macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`
**Windows**: `%APPDATA%\Claude\claude_desktop_config.json`

```json
{
  "mcpServers": {
    "skillz": {
      "command": "/absolute/path/to/skillz",
      "env": {
        "TOOLS_DIR": "~/skillz-tools"
      }
    }
  }
}
```

### VS Code (with Continue or similar)

Add to your MCP configuration:

```json
{
  "mcpServers": {
    "skillz": {
      "command": "/absolute/path/to/skillz",
      "env": {
        "TOOLS_DIR": "~/skillz-tools"
      }
    }
  }
}
```

### Windsurf / Zed / Other MCP Clients

Most MCP clients use the same JSON configuration format. Add `skillz` to your MCP servers configuration with the command pointing to the built binary.

---

## 🎯 What is Skillz?

Skillz is a **Model Context Protocol (MCP) server** that allows AI assistants to dynamically create and execute custom tools at runtime. Unlike static tool systems, Skillz enables AI to:

- 🦀 **Build WASM tools** from Rust code on-the-fly
- 📜 **Register script tools** in any language (Python, Node.js, Ruby, Bash, etc.)
- 🔄 **Execute tools** with full JSON-RPC 2.0 protocol support
- 🔧 **Create skills step-by-step** with guided workflow
- 🔒 **Run safely** in a WebAssembly sandbox

```
┌─────────────────────────────────────────────────────────────┐
│                    AI Assistant (Claude, etc.)               │
└───────────────────────────┬─────────────────────────────────┘
                            │ MCP Protocol
┌───────────────────────────▼─────────────────────────────────┐
│                     Skillz MCP Server                        │
│  ┌─────────────────┐  ┌─────────────────┐  ┌──────────────┐ │
│  │   WASM Tools    │  │  Script Tools   │  │  Sequential  │ │
│  │  (Rust → WASM)  │  │ (Any Language)  │  │   Creation   │ │
│  └─────────────────┘  └─────────────────┘  └──────────────┘ │
│  ┌─────────────────────────────────────────────────────────┐│
│  │              Tool Registry & Runtime                    ││
│  └─────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

---

## ✨ Features

### 🦀 WASM Tools (Rust)
Compile Rust code to WebAssembly at runtime. Tools run in a secure sandbox with memory isolation.

```rust
fn main() {
    println!("Hello from WASM!");
}
```

### 📜 Script Tools (Any Language)
Register tools written in Python, Node.js, Ruby, Bash, or any language. Scripts communicate via JSON-RPC 2.0.

```python
#!/usr/bin/env python3
import json, sys

request = json.loads(sys.stdin.read())
result = {"message": "Hello from Python!"}
print(json.dumps({"jsonrpc": "2.0", "result": result, "id": request["id"]}))
```

### 🔧 Sequential Skill Creation
Build tools step-by-step with guided workflow: **Design → Implement → Test → Finalize**

### 🔌 Full MCP Protocol
- **Resources**: Dynamic documentation and tool info (updates when tools are added!)
- **Roots**: Workspace directory access for scripts
- **Logging**: Real-time log streaming
- **Progress**: Progress reporting for long operations
- **Elicitation**: Request user input (when supported)
- **Sampling**: Request LLM completions (when supported)

---

## 📖 Documentation

### Available MCP Tools

| Tool | Description |
|------|-------------|
| `build_tool` | Compile Rust code → WASM tool |
| `register_script` | Register any-language script tool |
| `create_skill` | Step-by-step skill creation workflow |
| `call_tool` | Execute a registered tool |
| `list_tools` | List all available tools |
| `test_validate` | Validate Rust code before building |
| `add_root` | Add workspace root for scripts |
| `list_roots` | List current workspace roots |

### Available Resources

| URI | Description |
|-----|-------------|
| `skillz://guide` | Complete usage guide (auto-updates with new tools!) |
| `skillz://examples` | Code examples for all languages |
| `skillz://protocol` | JSON-RPC 2.0 protocol documentation |
| `skillz://tools/{name}` | Individual tool documentation |

### Script Tool Protocol

Scripts receive JSON-RPC 2.0 requests on stdin and respond on stdout:

```json
// Request
{
  "jsonrpc": "2.0",
  "method": "execute",
  "params": {
    "arguments": { /* user args */ },
    "context": {
      "roots": ["/workspace"],
      "capabilities": { "sampling": true, "elicitation": true }
    }
  },
  "id": 1
}

// Response
{"jsonrpc": "2.0", "result": { /* output */ }, "id": 1}
```

**Script Features:**
- **Logging**: `{"jsonrpc": "2.0", "method": "log", "params": {"level": "info", "message": "..."}}`
- **Progress**: `{"jsonrpc": "2.0", "method": "progress", "params": {"current": 50, "total": 100}}`

---

## 💡 Examples

### Build a WASM Tool

```
Use build_tool to create a Fibonacci generator:

build_tool(
  name: "fibonacci",
  code: "fn main() { let mut a=0u64; let mut b=1; for _ in 0..20 { print!(\"{} \", a); let t=a+b; a=b; b=t; } }",
  description: "Generates Fibonacci numbers"
)
```

### Register a Python Tool

```
Use register_script to create a Python tool:

register_script(
  name: "word_count",
  interpreter: "python3",
  code: "#!/usr/bin/env python3\nimport json,sys\nreq=json.loads(sys.stdin.read())\ntext=req['params']['arguments'].get('text','')\nresult={'words':len(text.split()),'chars':len(text)}\nprint(json.dumps({'jsonrpc':'2.0','result':result,'id':req['id']}))",
  description: "Counts words and characters in text"
)
```

### Step-by-Step Skill Creation

```
create_skill(name: "calculator", description: "Math calculator", step: 1, content: "Design notes...", skill_type: "wasm")
create_skill(name: "calculator", description: "Math calculator", step: 2, content: "fn main() {...}", skill_type: "wasm")
create_skill(name: "calculator", description: "Math calculator", step: 3, content: "fn main() {...}", skill_type: "wasm")
create_skill(name: "calculator", description: "Math calculator", step: 4, content: "fn main() {...}", skill_type: "wasm")
```

### Execute a Tool

```
call_tool(tool_name: "fibonacci")
call_tool(tool_name: "word_count", arguments: {"text": "Hello world!"})
```

---

## 🏗️ Architecture

```
skillz/
├── src/
│   ├── main.rs       # MCP server, tools, resources
│   ├── builder.rs    # Rust → WASM compilation
│   ├── runtime.rs    # WASM & Script execution
│   └── registry.rs   # Tool storage & management
├── tests/            # Rust integration tests
├── docs/             # GitHub Pages documentation
└── .github/          # CI/CD workflows
```

### How It Works

1. **Build Request**: AI sends Rust code via `build_tool`
2. **Compilation**: Code compiled to WASM using `cargo build --target wasm32-wasip1`
3. **Registration**: Tool metadata stored in `manifest.json`
4. **Execution**: WASM runs in Wasmtime sandbox, scripts via subprocess
5. **Response**: Output returned to AI via MCP protocol

---

## 🔒 Security

- **WASM Sandbox**: Tools run in isolated WebAssembly environment
- **Memory Safety**: Rust's guarantees extend to compiled tools
- **Script Isolation**: Scripts run as separate processes
- **Filtered Environment**: Only safe env vars passed to scripts
- **Root Restrictions**: Scripts only access declared workspace roots

See [SECURITY.md](SECURITY.md) for full security documentation.

---

## 🛠️ Development

### Building

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Run tests
cargo test
```

### Testing

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_mcp_initialization

# Run with output
cargo test -- --nocapture
```

---

## 🤝 Contributing

Contributions are welcome! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit changes (`git commit -m 'Add amazing feature'`)
4. Push to branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

---

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---

## 🙏 Acknowledgments

- [Model Context Protocol](https://modelcontextprotocol.io/) by Anthropic
- [rmcp](https://crates.io/crates/rmcp) - Official Rust MCP SDK
- [Wasmtime](https://wasmtime.dev/) - WebAssembly runtime
- [Tokio](https://tokio.rs/) - Async Rust runtime

---

<div align="center">

**Built with ❤️ for AI-powered development**

[⬆ Back to Top](#-skillz---self-extending-mcp-server)

</div>
