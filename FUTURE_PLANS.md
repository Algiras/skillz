# 🗺️ Skillz Future Plans & Roadmap

This document tracks planned features and ideas for future development.

---

## 🚀 High Priority (Next Up)

*(All high priority items completed! See completed features below.)*

---

## 🎯 Medium Priority

### Tool Result Streaming (Enhanced)
- Stream large outputs progressively
- Real-time feedback for long operations

### Tool Templates
Pre-built templates for common patterns:
```
create_from_template(
  template: "api-client",
  name: "github_api",
  config: { base_url: "https://api.github.com" }
)
```

### Tool Versioning & Rollback
- Keep multiple versions of tools
- `call_tool(name: "my_tool", version: "1.0.0")`
- Automatic version bumping on updates

---

## 💡 Ideas / Research

### Multi-language WASM
Support compiling from other languages to WASM:
- AssemblyScript (TypeScript-like)
- TinyGo
- C/C++ via Emscripten

### Tool Debugging
- Step-through execution
- Breakpoints for scripts
- Inspect tool state mid-execution

### Tool Marketplace
- Centralized registry of community tools
- Ratings, reviews, download counts
- Categories and search

### Tool Permissions
- Fine-grained capability model
- Network access permissions
- File system access scopes

### AI-Powered Tool Generation
- Generate tool code from natural language description
- Auto-generate tests
- Suggest improvements

---

## ✅ Completed Features

- [x] WASM tools (Rust → WebAssembly)
- [x] Script tools (Python, Node.js, Ruby, Bash, etc.)
- [x] WASM/Rust dependencies (crates)
- [x] Script dependencies (pip/npm)
- [x] Tool annotations
- [x] Completion API
- [x] Code execution mode
- [x] Sandbox isolation (bubblewrap/firejail/nsjail)
- [x] Per-tool directories with manifest.json
- [x] Dynamic guide resource
- [x] Sequential skill creation workflow
- [x] Environment variable configuration (SKILLZ_ROOTS, etc.)
- [x] Tool import from Git repos and GitHub Gists
- [x] **Pipelines** - Chain tools together (pipelines ARE tools!)
- [x] **HTTP Transport with SSE** - Run as web service with multiple clients
- [x] **Persistent Memory** - Key-value storage for tools (libSQL backend)
- [x] **Elicitation** - Scripts can request user input via MCP protocol
- [x] **Sampling** - Scripts can request LLM completions via MCP protocol
- [x] **Hot Reload** - Watch tools directory for changes, auto-reload
- [x] **Versioning** - Auto-backup on update, rollback to any version
- [x] **Resources** - Tools can list/read server resources (`resources/list`, `resources/read`)
- [x] **Secrets** - Forward `SKILLZ_*` env vars to tools for API keys, tokens
- [x] **Caching/TTL** - Memory with TTL support (`memory/set` with `ttl` parameter)
- [x] **Tools/Call** - Tools can call other tools (`tools/call` method)
- [x] **Streaming** - Progressive output via `stream/chunk` notifications

---

## 📝 Contributing Ideas

Have an idea? Open an issue on GitHub or submit a PR adding to this document!

---

*Last updated: November 2025*

