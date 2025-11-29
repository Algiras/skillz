# 🚀 Skillz - Self-Extending MCP Server

<div align="center">

[![CI](https://github.com/Algiras/skillz/actions/workflows/ci.yml/badge.svg)](https://github.com/Algiras/skillz/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![MCP](https://img.shields.io/badge/MCP-2024--11--05-blue.svg)](https://modelcontextprotocol.io/)

**Build and execute custom tools at runtime. Let your AI create its own tools.**

[Getting Started](#-getting-started) •
[Features](#-features) •
[Documentation](#-documentation) •
[Examples](#-examples) •
[Contributing](#-contributing)

</div>

---

## 🎯 What is Skillz?

Skillz is a **Model Context Protocol (MCP) server** that allows AI assistants to dynamically create and execute custom tools at runtime. Unlike static tool systems, Skillz enables AI to:

- 🦀 **Build WASM tools** from Rust code on-the-fly
- 📜 **Register script tools** in any language (Python, Node.js, Ruby, Bash, etc.)
- 🔄 **Execute tools** with full JSON-RPC 2.0 protocol support
- 🧠 **Think step-by-step** with sequential thinking capabilities
- 🔒 **Run safely** in a WebAssembly sandbox

```
┌─────────────────────────────────────────────────────────────┐
│                    AI Assistant (Claude, etc.)               │
└───────────────────────────┬─────────────────────────────────┘
                            │ MCP Protocol
┌───────────────────────────▼─────────────────────────────────┐
│                     Skillz MCP Server                        │
│  ┌─────────────────┐  ┌─────────────────┐  ┌──────────────┐ │
│  │   WASM Tools    │  │  Script Tools   │  │  Thinking    │ │
│  │  (Rust → WASM)  │  │ (Any Language)  │  │  Framework   │ │
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

### 🧠 Sequential Thinking
Built-in support for step-by-step reasoning with revision and branching capabilities.

### 🔌 Full MCP Protocol
- **Resources**: Dynamic documentation and tool info
- **Roots**: Workspace directory access for scripts
- **Logging**: Real-time log streaming
- **Progress**: Progress reporting for long operations
- **Elicitation**: Request user input (when supported)
- **Sampling**: Request LLM completions (when supported)

---

## 🚀 Getting Started

### Prerequisites

- **Rust 1.70+** with `wasm32-wasip1` target
- **Cargo** package manager

### Installation

```bash
# Install WASM target
rustup target add wasm32-wasip1

# Clone the repository
git clone https://github.com/Algiras/skillz.git
cd skillz

# Build release
cargo build --release
```

### Configure with Cursor IDE

Add to your `~/.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "skillz": {
      "command": "/path/to/skillz/mcp-wasm-host/target/release/mcp-wasm-host",
      "args": [],
      "env": {
        "TOOLS_DIR": "/path/to/tools",
        "PATH": "/usr/local/bin:/usr/bin:/bin:~/.cargo/bin"
      }
    }
  }
}
```

### Configure with Claude Desktop

Add to your Claude Desktop config:

```json
{
  "mcpServers": {
    "skillz": {
      "command": "/path/to/mcp-wasm-host",
      "env": {
        "TOOLS_DIR": "~/tools"
      }
    }
  }
}
```

---

## 📖 Documentation

### Available MCP Tools

| Tool | Description |
|------|-------------|
| `build_tool` | Compile Rust code → WASM tool |
| `register_script` | Register any-language script tool |
| `call_tool` | Execute a registered tool |
| `list_tools` | List all available tools |
| `test_validate` | Validate Rust code before building |
| `add_root` | Add workspace root for scripts |
| `list_roots` | List current workspace roots |

### Available Resources

| URI | Description |
|-----|-------------|
| `skillz://guide` | Complete usage guide |
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

### Execute a Tool

```
call_tool(tool_name: "fibonacci")
call_tool(tool_name: "word_count", arguments: {"text": "Hello world!"})
```

---

## 🏗️ Architecture

```
mcp-wasm-host/
├── src/
│   ├── main.rs       # MCP server, tools, resources
│   ├── builder.rs    # Rust → WASM compilation
│   ├── runtime.rs    # WASM & Script execution
│   └── registry.rs   # Tool storage & management
├── tools/            # Compiled WASM & scripts
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
python3 test_e2e.py
```

### Testing

```bash
# Full end-to-end test
python3 test_e2e.py

# Validation tests
python3 test_validate.py

# Persistence tests
python3 test_persistence.py

# Workflow tests
python3 test_workflow.py
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
