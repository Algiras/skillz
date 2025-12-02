# 🏗️ Skillz Architecture Documentation

> **Version**: 0.7.0  
> **Last Updated**: December 2025  
> **Total Codebase**: ~8,500 lines of Rust

---

## 📋 Table of Contents

1. [Overview](#-overview)
2. [Current Architecture](#-current-architecture)
3. [Module Breakdown](#-module-breakdown)
4. [Design Decisions](#-design-decisions)
5. [Pain Points](#-pain-points)
6. [Compositional Architecture Roadmap](#-compositional-architecture-roadmap)
7. [Implementation Plan](#-implementation-plan)

---

## 🎯 Overview

### What is Skillz?

Skillz is a **self-extending MCP (Model Context Protocol) server** that allows AI assistants to create and execute custom tools at runtime. It supports:

- **WASM Tools**: Rust code compiled to WebAssembly
- **Script Tools**: Python, Node.js, Ruby, Bash via JSON-RPC 2.0
- **Pipeline Tools**: Declarative tool chains

### Why Does It Exist?

Traditional MCP servers have fixed tool sets. Skillz enables:
- Dynamic tool creation without server restarts
- AI-driven tool composition
- Tool persistence and versioning
- Cross-platform compatibility

---

## 🏛️ Current Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        main.rs (~3300 lines)                     │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │  CLI Parsing │ AppState │ MCP Handlers │ Tool Args │ Routes ││
│  └─────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────┘
                                 │
         ┌───────────────────────┼───────────────────────┐
         ▼                       ▼                       ▼
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│  runtime.rs     │    │  registry.rs    │    │   memory.rs     │
│  (1493 lines)   │    │  (1100 lines)   │    │   (598 lines)   │
│                 │    │                 │    │                 │
│ • WASM Runtime  │    │ • ToolManifest  │    │ • SQLite KV     │
│ • Script Runner │    │ • ToolRegistry  │    │ • TTL Support   │
│ • Sandboxing    │    │ • Versioning    │    │ • Migrations    │
│ • JSON-RPC 2.0  │    │ • Persistence   │    │                 │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                       │                       │
         ▼                       ▼                       ▼
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│  builder.rs     │    │  importer.rs    │    │  client.rs      │
│  (243 lines)    │    │  (370 lines)    │    │  (330 lines)    │
│                 │    │                 │    │                 │
│ • Cargo Gen     │    │ • Git Import    │    │ • MCP Client    │
│ • WASM Compile  │    │ • Gist Import   │    │ • Stdio Support │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                       │                       │
         ▼                       ▼                       ▼
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│  pipeline.rs    │    │  watcher.rs     │    │  config.rs      │
│  (391 lines)    │    │  (273 lines)    │    │  (50 lines)     │
│                 │    │                 │    │                 │
│ • Var Resolution│    │ • File Watching │    │ • skillz.toml   │
│ • Conditions    │    │ • Hot Reload    │    │ • Server Config │
└─────────────────┘    └─────────────────┘    └─────────────────┘
```
                    │ • Hot Reload    │
                    └─────────────────┘
```

### Data Flow

```
MCP Client Request
       │
       ▼
┌──────────────────┐
│   AppState       │──────────────────────────────────┐
│   (main.rs)      │                                  │
└──────────────────┘                                  │
       │                                              │
       ├──► build_tool() ──► Builder ──► Registry    │
       │                                              │
       ├──► register_script() ──► Registry           │
       │                                              │
       ├──► call_tool() ──► Runtime ──► Execute      │
       │                        │                    │
       │                        ├──► WASM (wasmtime) │
       │                        └──► Script (spawn)  │
       │                                              │
       ├──► memory() ──► Memory (SQLite)              │
       │                                              │
       └──► pipeline() ──► PipelineExecutor           │
                              │                       │
                              └──► call_tool() (loop) │
```

---

## 📦 Module Breakdown

### `main.rs` - The Monolith (2974 lines)

**Current Responsibilities** (TOO MANY):
- CLI argument parsing (`Cli` struct)
- Application state (`AppState`)
- MCP server initialization
- All tool handler implementations
- Resource handlers
- Prompt handlers
- Guide/Protocol content generation
- HTTP/SSE server setup
- Hot reload coordination
- Logging configuration

**Key Structs**:
```rust
struct AppState {
    registry: ToolRegistry,      // Tool storage
    runtime: ToolRuntime,        // Tool execution
    memory: Memory,              // Persistent KV
    tool_router: ToolRouter,     // MCP routing
    peer: SharedPeer,            // Notification sender
    client_caps: SharedClientCaps,  // Client capabilities
    subscriptions: SharedSubscriptions,  // Resource subscriptions
}
```

### `runtime.rs` - Execution Engine (1493 lines)

**Responsibilities**:
- WASM execution via `wasmtime`
- Script execution via process spawning
- JSON-RPC 2.0 communication with scripts
- Sandbox configuration (bubblewrap, firejail, nsjail)
- Handler types for MCP capabilities:
  - `ElicitationHandler`
  - `SamplingHandler`
  - `ProgressHandler`
  - `LogHandler`
  - `ResourceListHandler`
  - `ResourceReadHandler`
  - `ToolCallHandler`
  - `StreamHandler`

**Key Abstractions**:
```rust
struct ToolRuntime {
    sandbox_config: SandboxConfig,
    // Various handlers stored as Option<Arc<dyn Fn>>
}

struct ExecutionContext {
    arguments: Value,
    roots: Vec<String>,
    environment: HashMap<String, String>,
    capabilities: ClientCapabilities,
    meta: Option<RequestMeta>,
}
```

### `registry.rs` - Tool Storage (973 lines)

**Responsibilities**:
- Tool manifest management
- File-based persistence
- Versioning and rollback
- Tool type discrimination (WASM, Script, Pipeline)

**Key Types**:
```rust
enum ToolType { Wasm, Script, Pipeline }

struct ToolManifest {
    name: String,
    description: String,
    tool_type: ToolType,
    version: String,
    // ... annotations, schemas, etc.
}

struct ToolRegistry {
    tools: Arc<RwLock<HashMap<String, ToolConfig>>>,
    storage_dir: PathBuf,
}
```

### `memory.rs` - Persistent Storage (598 lines)

**Responsibilities**:
- SQLite-based key-value store
- TTL (time-to-live) for cached entries
- Schema migrations
- Per-tool namespacing

**Design Choice**: Uses `rusqlite` with bundled SQLite for cross-platform compatibility (Windows, macOS, Linux).

### `pipeline.rs` - Tool Composition (391 lines)

**Responsibilities**:
- Variable resolution (`$input`, `$prev`, `$step_name`)
- Condition evaluation
- Step result tracking

### `importer.rs` - External Tools (370 lines)

**Responsibilities**:
- Import from Git repositories
- Import from GitHub Gists
- Manifest validation

### `watcher.rs` - Hot Reload (273 lines)

**Responsibilities**:
- File system watching with debouncing
- Event classification (added, modified, removed)
- Notification dispatch

### `builder.rs` - WASM Compilation (243 lines)

**Responsibilities**:
- Generate Cargo.toml for tools
- Compile Rust to WASM
- Dependency management

---

## 🎨 Design Decisions

### Why These Choices Were Made

| Decision | Rationale | Trade-off |
|----------|-----------|-----------|
| **Single `main.rs`** | Rapid prototyping, easy navigation | Hard to maintain at scale |
| **`wasmtime` for WASM** | Industry standard, WASI support | Large binary size |
| **JSON-RPC 2.0 for scripts** | Bidirectional communication | Protocol overhead |
| **SQLite for memory** | Cross-platform, embedded | Single writer limitation |
| **File-based registry** | Inspectable, portable | No indexing |
| **`rmcp` crate** | Official MCP Rust SDK | Learning curve |

### MCP Protocol Choices

- **Full protocol support**: Tools, Resources, Prompts, Logging, Progress, Elicitation, Sampling
- **Bidirectional scripts**: Scripts can request user input and LLM completions
- **Subscriptions**: Clients can subscribe to resource updates
- **Hot reload**: Tools auto-reload on file changes

---

## 🔴 Pain Points

### 1. **Monolithic `main.rs`** (Critical)
- 2974 lines is too large for one file
- Mixing concerns: CLI, handlers, content, routing
- Hard to test individual components
- Difficult to onboard new contributors

### 2. **AppState God Object**
- Holds references to everything
- No clear ownership boundaries
- Clone required for async handlers

### 3. **Handler Registration Complexity**
- Runtime has 8+ handler types
- Each handler is `Option<Arc<dyn Fn + Send + Sync>>`
- Verbose wiring in main.rs

### 4. **Embedded Content**
- Guide and Protocol strings are hardcoded
- ~800 lines of markdown in Rust code
- Can't be edited without recompilation

### 5. **Tight Coupling**
- Runtime depends on Registry types
- Main depends on everything
- No dependency injection

### 6. **Limited Testability**
- Integration tests require full server
- Handler logic can't be unit tested
- Mock implementations are difficult

---

## 🚀 Compositional Architecture Roadmap

### Target Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                          Application                             │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐        │
│  │   CLI    │  │   HTTP   │  │  Config  │  │  Startup │        │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘        │
└─────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────┐
│                        MCP Layer                                 │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐        │
│  │  Tools   │  │Resources │  │ Prompts  │  │  Notify  │        │
│  │ Handler  │  │ Handler  │  │ Handler  │  │ Handler  │        │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘        │
└─────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────┐
│                        Domain Layer                              │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐        │
│  │ToolMgmt  │  │ Executor │  │ Pipeline │  │ Importer │        │
│  │ Service  │  │ Service  │  │ Service  │  │ Service  │        │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘        │
└─────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────┐
│                     Infrastructure Layer                         │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐        │
│  │ Registry │  │  Memory  │  │  WASM    │  │  Script  │        │
│  │  (File)  │  │ (SQLite) │  │ Runtime  │  │ Runtime  │        │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘        │
└─────────────────────────────────────────────────────────────────┘
```

### Key Principles

1. **Single Responsibility**: Each module does one thing well
2. **Dependency Inversion**: Depend on traits, not implementations
3. **Interface Segregation**: Small, focused interfaces
4. **Composition over Inheritance**: Compose behaviors via traits

---

## 📋 Implementation Plan

### Phase 1: Extract Content (Low Risk)

**Goal**: Move embedded strings to external files

**Changes**:
```
docs/
├── content/
│   ├── guide.md        # Extracted from get_guide_content()
│   ├── protocol.md     # Extracted from get_protocol_content()
│   └── examples.md     # Extracted from get_examples_content()
```

**Code**:
```rust
// Before (in main.rs)
fn get_guide_content() -> String {
    r##"# 🚀 Skillz Guide..."##.to_string()
}

// After (new content.rs module)
pub fn load_guide() -> String {
    include_str!("../docs/content/guide.md").to_string()
}
```

**Effort**: 1-2 hours  
**Risk**: Very Low  
**Benefit**: Easier content editing, smaller main.rs

---

### Phase 2: Extract Tool Handlers (Medium Risk)

**Goal**: Move tool implementations to separate modules

**New Structure**:
```
src/
├── handlers/
│   ├── mod.rs
│   ├── build_tool.rs
│   ├── register_script.rs
│   ├── call_tool.rs
│   ├── memory.rs
│   ├── pipeline.rs
│   ├── import.rs
│   └── version.rs
```

**Example Extraction**:
```rust
// src/handlers/build_tool.rs
pub struct BuildToolHandler {
    registry: Arc<ToolRegistry>,
    builder: Arc<Builder>,
}

impl BuildToolHandler {
    pub async fn handle(&self, args: BuildToolArgs) -> String {
        // Move logic from AppState::build_tool()
    }
}
```

**Effort**: 4-6 hours  
**Risk**: Medium (need to update routing)  
**Benefit**: Testable handlers, smaller main.rs (~1500 lines less)

---

### Phase 3: Introduce Service Layer (Medium Risk)

**Goal**: Create domain services that encapsulate business logic

**New Modules**:
```rust
// src/services/tool_service.rs
pub struct ToolService {
    registry: Arc<ToolRegistry>,
    runtime: Arc<ToolRuntime>,
}

impl ToolService {
    pub async fn build_wasm(&self, args: BuildArgs) -> Result<Tool>;
    pub async fn register_script(&self, args: ScriptArgs) -> Result<Tool>;
    pub async fn execute(&self, name: &str, args: Value) -> Result<Value>;
    pub async fn delete(&self, name: &str) -> Result<()>;
}

// src/services/memory_service.rs
pub struct MemoryService {
    memory: Arc<Memory>,
}

impl MemoryService {
    pub async fn get(&self, tool: &str, key: &str) -> Result<Option<Value>>;
    pub async fn set(&self, tool: &str, key: &str, value: Value, ttl: Option<u64>) -> Result<()>;
    pub async fn list(&self, tool: &str) -> Result<Vec<String>>;
}
```

**Effort**: 6-8 hours  
**Risk**: Medium  
**Benefit**: Reusable services, cleaner handlers

---

### Phase 4: Trait-Based Runtime (Higher Risk)

**Goal**: Abstract runtime behind traits for testability

```rust
// src/runtime/traits.rs
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute(&self, config: &ToolConfig, context: ExecutionContext) -> Result<String>;
}

#[async_trait]
pub trait WasmExecutor: Send + Sync {
    async fn run(&self, module: &[u8], stdin: &str) -> Result<String>;
}

#[async_trait]
pub trait ScriptExecutor: Send + Sync {
    async fn run(&self, script: &Path, interpreter: &str, context: &ExecutionContext) -> Result<String>;
}

// Real implementations
pub struct WasmtimeExecutor { engine: Engine }
pub struct ProcessExecutor { sandbox: SandboxConfig }

// Test implementations
pub struct MockExecutor { responses: HashMap<String, String> }
```

**Effort**: 8-12 hours  
**Risk**: Higher (changes core execution)  
**Benefit**: Unit testable execution, swappable runtimes

---

### Phase 5: Configuration-Driven (Lower Risk)

**Goal**: Extract configuration to files

```toml
# skillz.toml
[server]
transport = "stdio"  # or "http"
port = 8080
host = "127.0.0.1"

[tools]
directory = "~/tools"
hot_reload = true

[sandbox]
mode = "none"  # bubblewrap, firejail, nsjail
allow_network = false
memory_limit_mb = 512
time_limit_secs = 30

[features]
versioning = true
pipelines = true
```

**Effort**: 3-4 hours  
**Risk**: Low  
**Benefit**: Runtime configuration, no recompilation

---

### Phase 6: MCP Handler Abstraction (Medium Risk)

**Goal**: Create a cleaner MCP integration layer

```rust
// src/mcp/mod.rs
pub struct McpServer {
    state: Arc<AppState>,
}

impl McpServer {
    pub fn new(services: Services) -> Self;
    
    pub fn tool_router(&self) -> ToolRouter {
        tool_router! {
            BuildToolHandler::new(self.state.tool_service.clone()),
            RegisterScriptHandler::new(self.state.tool_service.clone()),
            CallToolHandler::new(self.state.executor_service.clone()),
            // ...
        }
    }
}
```

**Effort**: 6-8 hours  
**Risk**: Medium  
**Benefit**: Clean MCP layer, reusable for other protocols

---

## 📊 Migration Priority Matrix

| Phase | Effort | Risk | Impact | Priority |
|-------|--------|------|--------|----------|
| 1. Extract Content | Low | Very Low | Medium | 🟢 **Do First** |
| 2. Extract Handlers | Medium | Medium | High | 🟢 **Do Second** |
| 5. Configuration | Low | Low | Medium | 🟡 **Do Third** |
| 3. Service Layer | Medium | Medium | High | 🟡 **Do Fourth** |
| 4. Trait Runtime | High | Higher | High | 🟠 **Do Fifth** |
| 6. MCP Abstraction | Medium | Medium | Medium | 🟠 **Do Last** |

---

## 🎯 Success Metrics

After refactoring:

| Metric | Current | Target |
|--------|---------|--------|
| `main.rs` lines | 2974 | < 500 |
| Largest module | 2974 | < 600 |
| Test coverage | ~60% | > 80% |
| Handler unit tests | 0 | 100% |
| Config options | CLI only | File + CLI + Env |

---

## 🔧 Quick Wins (Do Today)

1. **Extract guide/protocol content** to markdown files
2. **Add `#[cfg(test)]` module** to main.rs for handler tests
3. **Create `src/handlers/mod.rs`** and move one handler
4. **Add `tracing`** crate for structured logging

---

## 📚 References

- [MCP Specification](https://modelcontextprotocol.io)
- [rmcp Crate Docs](https://docs.rs/rmcp)
- [wasmtime Book](https://docs.wasmtime.dev)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)

---

*This document should be updated as the architecture evolves.*

