<div align="center">

# 🚀 Skillz - Self-Extending MCP Server

<img src="docs/architecture.png" alt="Skillz Architecture" width="100%">

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
# 🚀 One-line install (Linux & macOS)
curl -fsSL https://raw.githubusercontent.com/Algiras/skillz/master/install.sh | sh

# 🎯 Install specific version
curl -fsSL https://raw.githubusercontent.com/Algiras/skillz/master/install.sh | sh -s -- v0.4.0

# ⚡ Run without installing (npx-style)
curl -fsSL https://raw.githubusercontent.com/Algiras/skillz/master/install.sh | sh -s -- latest run

# Or install from crates.io
cargo install skillz

# Install WASM target (required for building tools)
rustup target add wasm32-wasip1
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

### Zero-Install Mode (Run on Demand)

You can run Skillz without installing it, similar to `npx`:

```json
{
  "mcpServers": {
    "skillz": {
      "command": "sh",
      "args": ["-c", "curl -fsSL https://raw.githubusercontent.com/Algiras/skillz/master/install.sh | sh -s -- latest run"]
    }
  }
}
```

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
| 📦 **Versioning** | Auto-backup on update, rollback to any version |
| 📋 **Templates** | Pre-built tool patterns, create & share custom templates |

---

## 📖 Available Tools (11 Core)

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
| `version` | List versions, rollback to previous, view version info |
| `template` | Use/create tool templates (list, info, use, create, delete) |

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

> **🤖 For LLMs & Advanced Users**  
> See [docs/LLM_GUIDE.md](docs/LLM_GUIDE.md) for detailed technical specifications, JSON-RPC protocols, script templates, and advanced usage examples.

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
