# 🗺️ Skillz Future Plans & Roadmap

This document tracks planned features and ideas for future development.

---

## 🚀 High Priority (Next Up)

### Persistent Tool State
Allow tools to save/load state between calls:
```python
state = context.get_state()
state["counter"] = state.get("counter", 0) + 1
context.save_state(state)
```

---

## 🎯 Medium Priority

### HTTP Transport with SSE
- Run Skillz as a web service
- Multiple clients can connect
- Server-Sent Events for streaming

### Tool Result Streaming
- Stream large outputs progressively
- Real-time feedback for long operations

### Hot Reload
- Watch tool directories for changes
- Automatically reload modified tools
- Notify clients of updates

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

---

## 📝 Contributing Ideas

Have an idea? Open an issue on GitHub or submit a PR adding to this document!

---

*Last updated: 2024*

