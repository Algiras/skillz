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
[Documentation](#-documentation) •
[Examples](#-examples)

</div>

---

## ⚡ Installation

### Using Cargo (Recommended)

```bash
# Install WASM target (required for building tools)
rustup target add wasm32-wasip1

# Install Skillz
cargo install skillz
```

That's it! The `skillz` binary is now available in your PATH.

### From Source

```bash
git clone https://github.com/Algiras/skillz.git
cd skillz
cargo install --path .
```

### Pre-built Binaries

Download from [GitHub Releases](https://github.com/Algiras/skillz/releases):
- `skillz-linux-x86_64.tar.gz` - Linux (x86_64)
- `skillz-macos-x86_64.tar.gz` - macOS (Intel)
- `skillz-macos-aarch64.tar.gz` - macOS (Apple Silicon)

---

## 🔧 Editor Configuration

### Cursor IDE

Add to `~/.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "skillz": {
      "command": "skillz",
      "env": {
        "TOOLS_DIR": "~/.skillz/tools"
      }
    }
  }
}
```

Restart Cursor or run **Developer: Reload Window**.

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
      "command": "skillz",
      "env": {
        "TOOLS_DIR": "~/.skillz/tools"
      }
    }
  }
}
```

### VS Code / Continue

Add to your MCP settings:

```json
{
  "mcpServers": {
    "skillz": {
      "command": "skillz",
      "env": {
        "TOOLS_DIR": "~/.skillz/tools"
      }
    }
  }
}
```

### Other Editors (Windsurf, Zed, etc.)

Most MCP-compatible editors use the same JSON format. Just add `skillz` to your MCP servers configuration.

> **Note**: If `skillz` isn't in your PATH, use the full path: `~/.cargo/bin/skillz`

---

## 🎯 What is Skillz?

Skillz is a **Model Context Protocol (MCP) server** that allows AI assistants to dynamically create and execute custom tools at runtime.

### Key Capabilities

| Feature | Description |
|---------|-------------|
| 🦀 **WASM Tools** | Compile Rust code to WebAssembly on-the-fly |
| 📜 **Script Tools** | Register tools in Python, Node.js, Ruby, Bash, etc. |
| 🔧 **Step-by-Step Creation** | Guided workflow: Design → Implement → Test → Finalize |
| 📡 **JSON-RPC 2.0** | Full protocol support for script communication |
| 🔒 **Sandbox Execution** | Tools run in isolated WebAssembly environment |
| 💾 **Persistence** | Tools persist across server restarts |

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
└─────────────────────────────────────────────────────────────┘
```

---

## ✨ Features

### 🦀 WASM Tools (Rust)

Compile Rust code to WebAssembly at runtime:

```rust
fn main() {
    println!("Hello from WASM!");
}
```

### 📜 Script Tools (Any Language)

Register tools in any language with JSON-RPC 2.0:

```python
#!/usr/bin/env python3
import json, sys

request = json.loads(sys.stdin.read())
result = {"message": "Hello from Python!"}
print(json.dumps({"jsonrpc": "2.0", "result": result, "id": request["id"]}))
```

### 🔧 Sequential Skill Creation

Build complex tools step-by-step:

```
Step 1: Design    → Define inputs, outputs, behavior
Step 2: Implement → Write the code
Step 3: Test      → Validate compilation/execution
Step 4: Finalize  → Register and use
```

---

## 📖 Documentation

### Available Tools

| Tool | Description |
|------|-------------|
| `build_tool` | Compile Rust code → WASM tool |
| `register_script` | Register script tool (Python, Node, etc.) |
| `create_skill` | Step-by-step skill creation |
| `call_tool` | Execute a registered tool |
| `list_tools` | List all available tools |
| `test_validate` | Validate Rust code before building |

### Resources

| URI | Description |
|-----|-------------|
| `skillz://guide` | Usage guide (auto-updates with new tools) |
| `skillz://examples` | Code examples for all languages |
| `skillz://protocol` | JSON-RPC 2.0 protocol docs |
| `skillz://tools/{name}` | Individual tool documentation |

---

## 💡 Quick Examples

### Create a WASM Tool

```
build_tool(
  name: "fibonacci",
  code: "fn main() { let (mut a, mut b) = (0u64, 1); for _ in 0..20 { print!(\"{} \", a); (a, b) = (b, a + b); } }",
  description: "Generates Fibonacci numbers"
)
```

### Create a Python Tool

```
register_script(
  name: "word_count",
  interpreter: "python3",
  description: "Counts words in text",
  code: "#!/usr/bin/env python3
import json, sys
req = json.loads(sys.stdin.read())
text = req['params']['arguments'].get('text', '')
print(json.dumps({'jsonrpc': '2.0', 'result': {'words': len(text.split())}, 'id': req['id']}))"
)
```

### Execute Tools

```
call_tool(tool_name: "fibonacci")
call_tool(tool_name: "word_count", arguments: {"text": "Hello world!"})
```

---

## 🔒 Security

- **WASM Sandbox**: Tools run in isolated Wasmtime environment
- **Memory Safety**: Rust guarantees extend to compiled tools
- **Process Isolation**: Scripts run as separate processes
- **Filtered Environment**: Only safe env vars passed to scripts

See [SECURITY.md](SECURITY.md) for details.

---

## 🛠️ Development

```bash
# Clone
git clone https://github.com/Algiras/skillz.git
cd skillz

# Build
cargo build --release

# Test
cargo test

# Install locally
cargo install --path .
```

---

## 🤝 Contributing

Contributions welcome! See [CONTRIBUTING.md](CONTRIBUTING.md).

1. Fork the repository
2. Create feature branch (`git checkout -b feature/amazing`)
3. Commit changes (`git commit -m 'Add amazing feature'`)
4. Push (`git push origin feature/amazing`)
5. Open Pull Request

---

## 📄 License

MIT License - see [LICENSE](LICENSE)

---

## 🔗 Links

- **Crates.io**: [crates.io/crates/skillz](https://crates.io/crates/skillz)
- **Documentation**: [algiras.github.io/skillz](https://algiras.github.io/skillz)
- **GitHub**: [github.com/Algiras/skillz](https://github.com/Algiras/skillz)
- **MCP Protocol**: [modelcontextprotocol.io](https://modelcontextprotocol.io)

---

<div align="center">

**Built with ❤️ for AI-powered development**

</div>
