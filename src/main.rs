mod builder;
mod importer;
mod memory;
mod pipeline;
mod registry;
mod runtime;
mod watcher;

use anyhow::Result;
use clap::Parser;
use registry::ToolType;
use rmcp::schemars::JsonSchema;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        AnnotateAble, ListResourcesResult, LoggingLevel, LoggingMessageNotificationParam,
        PaginatedRequestParam, ProgressNotificationParam, RawResource, ReadResourceRequestParam,
        ReadResourceResult, ResourceContents, ServerCapabilities, ServerInfo,
    },
    service::{NotificationContext, RequestContext},
    tool, tool_handler, tool_router,
    transport::stdio,
    ErrorData as McpError, RoleServer, ServerHandler, ServiceExt,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// CLI arguments for Skillz MCP server
#[derive(Parser, Debug)]
#[command(name = "skillz")]
#[command(author = "Algimantas Krasauskas")]
#[command(version)]
#[command(about = "Self-extending MCP server - build and execute custom AI tools at runtime")]
#[command(long_about = r#"
Skillz is an MCP server that can create new tools on-the-fly.

TRANSPORT MODES:
  - stdio (default): Standard input/output for CLI integrations
  - http: HTTP server with SSE for web integrations

EXAMPLES:
  # Run with stdio (default, for Cursor/Claude Desktop)
  skillz

  # Run as HTTP server on port 8080
  skillz --transport http --port 8080

  # Custom tools directory
  TOOLS_DIR=/my/tools skillz
"#)]
struct Cli {
    /// Transport mode: stdio or http
    #[arg(short, long, default_value = "stdio")]
    transport: String,

    /// Port for HTTP transport (only used with --transport http)
    #[arg(short, long, default_value = "8080")]
    port: u16,

    /// Host to bind for HTTP transport
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Enable hot reload - watch tools directory for changes
    #[arg(long, default_value = "false")]
    hot_reload: bool,
}

use rmcp::service::Peer;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Shared peer reference for sending notifications
type SharedPeer = Arc<RwLock<Option<Peer<RoleServer>>>>;

// Define the state
/// Stores actual MCP client capabilities
#[derive(Debug, Clone, Default)]
struct McpClientCapabilities {
    sampling: bool,
    elicitation: bool,
    roots: bool,
}

type SharedClientCaps = Arc<RwLock<McpClientCapabilities>>;

#[derive(Clone)]
struct AppState {
    registry: registry::ToolRegistry,
    runtime: runtime::ToolRuntime,
    memory: memory::Memory,
    tool_router: ToolRouter<Self>,
    /// Shared peer for sending logging notifications
    peer: SharedPeer,
    /// Actual client capabilities from MCP initialization
    client_caps: SharedClientCaps,
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
/// Unified tool building/management
struct BuildToolArgs {
    name: String,
    code: String,
    description: String,
    /// JSON Schema describing the tool's input arguments
    input_schema: Option<serde_json::Value>,
    /// JSON Schema describing the tool's structured output (optional)
    output_schema: Option<serde_json::Value>,
    /// Tool annotations - hints about behavior
    /// Example: {"readOnlyHint": true} or {"destructiveHint": true, "openWorldHint": true}
    annotations: Option<serde_json::Value>,
    /// Rust crate dependencies for WASM tools
    /// Format: "name@version" or "name@version[feat1,feat2]" or just "name"
    /// Example: ["serde@1.0[derive]", "regex@1.10", "anyhow"]
    dependencies: Option<Vec<String>>,
    /// Allow overwriting existing tools
    overwrite: Option<bool>,
}

/// Register a script tool
#[derive(Deserialize, Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct RegisterScriptArgs {
    /// Unique name for the script tool
    name: String,
    /// Description of what the tool does
    description: String,
    /// The script code. CRITICAL for Python: Use sys.stdin.readline() NOT sys.stdin.read()!
    /// read() blocks forever waiting for EOF. Always flush output with sys.stdout.flush().
    /// Template: request = json.loads(sys.stdin.readline()); ... print(json.dumps({...})); sys.stdout.flush()
    code: String,
    /// Language/interpreter to use (python3, node, ruby, bash, etc.)
    /// If not provided, the script must be executable
    interpreter: Option<String>,
    /// File extension for the script (py, js, rb, sh, etc.)
    extension: Option<String>,
    /// JSON Schema describing the tool's input arguments
    /// Example: {"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]}
    input_schema: Option<serde_json::Value>,
    /// JSON Schema describing the tool's structured output (optional)
    /// Example: {"type": "object", "properties": {"count": {"type": "integer"}}}
    output_schema: Option<serde_json::Value>,
    /// Tool annotations - hints about behavior for clients
    /// Example: {"readOnlyHint": false, "destructiveHint": true, "openWorldHint": true}
    annotations: Option<serde_json::Value>,
    /// Allow overwriting existing tools
    overwrite: Option<bool>,
    /// Dependencies to install (pip packages for Python, npm packages for Node.js)
    /// Example: ["requests", "pandas"] for Python, ["axios", "lodash"] for Node.js
    dependencies: Option<Vec<String>>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct DeleteToolArgs {
    /// Name of the tool to delete
    tool_name: String,
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
enum VersionAction {
    #[serde(rename = "list")]
    List,
    #[serde(rename = "rollback")]
    Rollback,
    #[serde(rename = "info")]
    Info,
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct VersionArgs {
    /// Action: 'list', 'rollback', 'info'
    action: VersionAction,
    /// Tool name
    tool_name: Option<String>,
    /// Version to rollback to (for 'rollback' action)
    version: Option<String>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct CallToolArgs {
    tool_name: String,
    arguments: Option<serde_json::Value>,
}

/// Code execution mode - compose multiple tools via code
#[derive(Deserialize, Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct ExecuteCodeArgs {
    /// Code to execute (Python by default)
    code: String,
    /// Language/interpreter: "python" (default), "javascript", "typescript"
    language: Option<String>,
    /// Tool names to make available as callable functions in the code
    /// If not specified, all registered tools are available
    tools: Option<Vec<String>>,
    /// Timeout in seconds (default: 30)
    timeout: Option<u64>,
}

/// Import a tool from an external source
#[derive(Deserialize, Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct ImportToolArgs {
    /// Source to import from. Supported formats:
    /// - Git: "https://github.com/user/repo" or "https://github.com/user/repo#branch"
    /// - Gist: "gist:GIST_ID" or "https://gist.github.com/user/GIST_ID"
    source: String,
    /// Allow overwriting if tool already exists
    overwrite: Option<bool>,
}

/// A step in a pipeline
#[derive(Clone, Deserialize, Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct PipelineStepArg {
    /// Optional name for referencing this step's output (e.g., "fetch_data")
    name: Option<String>,
    /// Tool to execute
    tool: String,
    /// Arguments to pass. Use $input.field, $prev.field, or $step_name.field for dynamic values
    args: Option<serde_json::Value>,
    /// Continue pipeline even if this step fails (default: false)
    continue_on_error: Option<bool>,
    /// Condition to check before running (e.g., "$prev.success == true")
    condition: Option<String>,
}

/// Unified pipeline management
#[derive(Deserialize, Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct PipelineArgs {
    /// Action: 'create', 'list', 'delete', 'run'
    action: String,
    /// Pipeline name (required for create/delete/run)
    name: Option<String>,
    /// Description of what the pipeline does (for create)
    description: Option<String>,
    /// Steps to execute in order (for create)
    steps: Option<Vec<PipelineStepArg>>,
    /// Tags for organization (for create/list filter)
    tags: Option<Vec<String>>,
    /// Filter by tag (for list)
    tag: Option<String>,
}

// ==================== Memory Args ====================

/// Unified memory management - combines get, set, list, clear, stats
#[derive(Deserialize, Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct MemoryArgs {
    /// Action: 'store', 'get', 'update', 'delete', 'list'
    action: String,
    /// Tool name (namespace for isolation)
    tool_name: String,
    /// Key to retrieve or store (required for get/store/delete)
    key: Option<String>,
    /// Value to store (any JSON value) - required for store action
    value: Option<serde_json::Value>,
}

impl AppState {
    fn new(
        registry: registry::ToolRegistry,
        mut runtime: runtime::ToolRuntime,
        memory: memory::Memory,
    ) -> Self {
        let peer: SharedPeer = Arc::new(RwLock::new(None));

        // Set up logging handler that forwards to MCP peer
        let peer_for_logging = peer.clone();
        let logging_handler: runtime::LoggingHandler = Arc::new(move |level, message, data| {
            let peer = peer_for_logging.clone();
            Box::pin(async move {
                if let Some(ref p) = *peer.read().await {
                    let mcp_level = match level.to_lowercase().as_str() {
                        "debug" => LoggingLevel::Debug,
                        "info" => LoggingLevel::Info,
                        "warning" | "warn" => LoggingLevel::Warning,
                        "error" => LoggingLevel::Error,
                        _ => LoggingLevel::Info,
                    };

                    let log_data = data.unwrap_or_else(|| serde_json::json!({"message": message}));
                    let _ = p
                        .notify_logging_message(LoggingMessageNotificationParam {
                            level: mcp_level,
                            logger: Some("skillz".to_string()),
                            data: log_data,
                        })
                        .await;
                }
            })
        });

        // Set up progress handler that forwards to MCP peer
        let peer_for_progress = peer.clone();
        let progress_handler: runtime::ProgressHandler =
            Arc::new(move |current, total, message| {
                let peer = peer_for_progress.clone();
                Box::pin(async move {
                    if let Some(ref p) = *peer.read().await {
                        let _ = p
                            .notify_progress(ProgressNotificationParam {
                                progress_token: rmcp::model::ProgressToken(
                                    rmcp::model::NumberOrString::String("tool_progress".into()),
                                ),
                                progress: current as f64 / total as f64 * 100.0,
                                total: Some(100.0),
                                message,
                            })
                            .await;
                    }
                })
            });

        // Set up elicitation handler that forwards to MCP peer
        let peer_for_elicit = peer.clone();
        let elicitation_handler: runtime::ElicitationHandler = Arc::new(move |message, schema| {
            let peer = peer_for_elicit.clone();
            Box::pin(async move {
                if let Some(ref p) = *peer.read().await {
                    // Convert JSON schema to ElicitationSchema
                    let schema_obj = match schema.as_object() {
                        Some(obj) => obj.clone(),
                        None => {
                            return Ok(
                                serde_json::json!({"action": "error", "error": "Schema must be a JSON object"}),
                            )
                        }
                    };
                    let elicit_schema = match rmcp::model::ElicitationSchema::from_json_schema(
                        schema_obj,
                    ) {
                        Ok(s) => s,
                        Err(e) => {
                            return Ok(
                                serde_json::json!({"action": "error", "error": format!("Invalid schema: {}", e)}),
                            )
                        }
                    };

                    // Forward elicitation request to MCP client
                    match p
                        .create_elicitation(rmcp::model::CreateElicitationRequestParam {
                            message,
                            requested_schema: elicit_schema,
                        })
                        .await
                    {
                        Ok(result) => Ok(serde_json::json!({
                            "action": match result.action {
                                rmcp::model::ElicitationAction::Accept => "accept",
                                rmcp::model::ElicitationAction::Decline => "decline",
                                rmcp::model::ElicitationAction::Cancel => "cancel",
                            },
                            "content": result.content
                        })),
                        Err(e) => {
                            Ok(serde_json::json!({"action": "error", "error": e.to_string()}))
                        }
                    }
                } else {
                    Ok(serde_json::json!({"action": "error", "error": "No MCP peer available"}))
                }
            })
        });

        // Set up sampling handler that forwards to MCP peer
        let peer_for_sampling = peer.clone();
        let sampling_handler: runtime::SamplingHandler = Arc::new(move |params| {
            let peer = peer_for_sampling.clone();
            Box::pin(async move {
                if let Some(ref p) = *peer.read().await {
                    // Parse the params into CreateMessageRequestParam
                    let request: rmcp::model::CreateMessageRequestParam =
                        serde_json::from_value(params)
                            .map_err(|e| anyhow::anyhow!("Invalid params: {}", e))?;

                    match p.create_message(request).await {
                        Ok(result) => Ok(serde_json::to_value(result)?),
                        Err(e) => Ok(serde_json::json!({"error": e.to_string()})),
                    }
                } else {
                    Ok(serde_json::json!({"error": "No MCP peer available"}))
                }
            })
        });

        // Set up resource handlers - allow tools to list and read Skillz's resources
        let registry_for_list = registry.clone();
        let resource_list_handler: runtime::ResourceListHandler = Arc::new(move || {
            let reg = registry_for_list.clone();
            Box::pin(async move {
                let mut resources = vec![
                    runtime::ResourceInfo {
                        uri: "skillz://guide".to_string(),
                        name: "Skillz Guide".to_string(),
                        description: Some("Complete usage guide for Skillz".to_string()),
                        mime_type: Some("text/markdown".to_string()),
                    },
                    runtime::ResourceInfo {
                        uri: "skillz://examples".to_string(),
                        name: "Skillz Examples".to_string(),
                        description: Some("Code snippets for WASM and Script tools".to_string()),
                        mime_type: Some("text/markdown".to_string()),
                    },
                    runtime::ResourceInfo {
                        uri: "skillz://protocol".to_string(),
                        name: "JSON-RPC 2.0 Protocol".to_string(),
                        description: Some("How script tools communicate".to_string()),
                        mime_type: Some("text/markdown".to_string()),
                    },
                ];

                // Add dynamic resources for each built tool
                for tool in reg.list_tools() {
                    resources.push(runtime::ResourceInfo {
                        uri: format!("skillz://tools/{}", tool.name()),
                        name: tool.name().to_string(),
                        description: Some(tool.description().to_string()),
                        mime_type: Some("text/markdown".to_string()),
                    });
                }

                Ok(resources)
            })
        });

        let registry_for_read = registry.clone();
        let resource_read_handler: runtime::ResourceReadHandler = Arc::new(move |uri| {
            let reg = registry_for_read.clone();
            Box::pin(async move {
                let content = match uri.as_str() {
                    "skillz://guide" => get_guide_content_static(),
                    "skillz://examples" => get_examples_content_static(),
                    "skillz://protocol" => get_protocol_content_static(),
                    _ if uri.starts_with("skillz://tools/") => {
                        let tool_name = uri.strip_prefix("skillz://tools/").unwrap();
                        get_tool_info_static(&reg, tool_name)
                    }
                    _ => return Err(anyhow::anyhow!("Resource not found: {}", uri)),
                };

                Ok(runtime::ResourceContent {
                    uri: uri.clone(),
                    mime_type: Some("text/markdown".to_string()),
                    text: Some(content),
                    blob: None,
                })
            })
        });

        // Set up stream handler - forward stream chunks (for progressive output)
        let peer_for_stream = peer.clone();
        let stream_handler: runtime::StreamHandler = Arc::new(move |chunk| {
            let peer = peer_for_stream.clone();
            Box::pin(async move {
                // Log the chunk for debugging
                eprintln!(
                    "[STREAM] Received chunk {}: {:?}",
                    chunk.index.unwrap_or(0),
                    chunk.data
                );

                // Forward as a log message for now (could use custom notification in future)
                if let Some(ref p) = *peer.read().await {
                    let _ = p
                        .notify_logging_message(LoggingMessageNotificationParam {
                            level: LoggingLevel::Info,
                            logger: Some("skillz.stream".to_string()),
                            data: serde_json::json!({
                                "type": "stream_chunk",
                                "index": chunk.index,
                                "is_final": chunk.is_final,
                                "data": chunk.data
                            }),
                        })
                        .await;
                }
            })
        });

        // Set up tool call handler - allow tools to call other tools
        let registry_for_tool_call = registry.clone();
        let runtime_for_tool_call = runtime.clone();
        let tool_call_handler: runtime::ToolCallHandler = Arc::new(move |name, arguments| {
            let reg = registry_for_tool_call.clone();
            let rt = runtime_for_tool_call.clone();
            Box::pin(async move {
                let tool = reg
                    .get_tool(&name)
                    .ok_or_else(|| anyhow::anyhow!("Tool '{}' not found", name))?;

                let args = arguments.unwrap_or(serde_json::json!({}));

                // Use spawn_blocking for sync operations
                let result = tokio::task::spawn_blocking(move || rt.call_tool(&tool, args))
                    .await
                    .map_err(|e| anyhow::anyhow!("Task join error: {}", e))??;

                Ok(result)
            })
        });

        // Configure runtime with handlers
        runtime = runtime
            .with_logging_handler(logging_handler)
            .with_progress_handler(progress_handler)
            .with_elicitation_handler(elicitation_handler)
            .with_sampling_handler(sampling_handler)
            .with_resource_handlers(resource_list_handler, resource_read_handler)
            .with_tool_call_handler(tool_call_handler)
            .with_stream_handler(stream_handler);

        Self {
            registry,
            runtime,
            memory,
            tool_router: Self::tool_router(),
            peer,
            client_caps: Arc::new(RwLock::new(McpClientCapabilities::default())),
        }
    }

    /// Update the peer reference (called when we get a context)
    async fn update_peer(&self, new_peer: Peer<RoleServer>) {
        let mut peer = self.peer.write().await;
        *peer = Some(new_peer);
    }

    /// Update client capabilities from MCP initialization
    async fn update_client_caps(&self, caps: McpClientCapabilities) {
        let mut client_caps = self.client_caps.write().await;
        *client_caps = caps;
    }

    /// Get current client capabilities
    async fn get_client_caps(&self) -> McpClientCapabilities {
        self.client_caps.read().await.clone()
    }
}

#[tool_router]
impl AppState {
    // ==================== WASM TOOLS (Rust) ====================

    #[tool(
        description = "Compile and register a new WASM tool from Rust code. Supports Rust crate dependencies! Set overwrite=true to update existing tools."
    )]
    async fn build_tool(&self, Parameters(args): Parameters<BuildToolArgs>) -> String {
        eprintln!("Building WASM tool: {}", args.name);
        
        // Check if tool exists
        if self.registry.get_tool(&args.name).is_some() && !args.overwrite.unwrap_or(false) {
            return format!(
                "Error: Tool '{}' already exists. Use overwrite=true to update it.",
                args.name
            );
        }

        // Parse dependencies
        let deps = args.dependencies.clone().unwrap_or_default();
        let wasm_deps = builder::Builder::parse_dependencies(&deps);

        // Compile with dependencies
        let wasm_bytes =
            match builder::Builder::compile_tool_with_deps(&args.name, &args.code, &wasm_deps) {
                Ok(path) => match std::fs::read(&path) {
                    Ok(bytes) => bytes,
                    Err(e) => return format!("Error reading compiled WASM: {}", e),
                },
            Err(e) => return format!("Compilation error: {}", e),
        };
        
        // Build manifest
        let mut manifest = registry::ToolManifest::new(
            args.name.clone(),
            args.description.clone(),
            ToolType::Wasm,
        );
        manifest.input_schema = args
            .input_schema
            .map(registry::ToolSchema::from_value)
            .unwrap_or_default();
        manifest.output_schema = args.output_schema.map(registry::ToolSchema::from_value);
        manifest.annotations = args.annotations.map(registry::ToolAnnotations::from_value);
        manifest.wasm_dependencies = deps.clone();

        // Also save the source code so the tool can be recompiled
        match self
            .registry
            .register_wasm_tool(manifest, &wasm_bytes, &args.code)
        {
            Ok(config) => {
                let tool_dir = config.tool_dir.display();
                let deps_msg = if deps.is_empty() {
                    String::new()
                } else {
                    format!("\n📦 Dependencies: {:?}", deps)
                };
                if args.overwrite.unwrap_or(false) {
                    format!(
                        "🦀 WASM Tool '{}' updated successfully\n\nDirectory: {}{}",
                        args.name, tool_dir, deps_msg
                    )
                } else {
                    format!(
                        "🦀 WASM Tool '{}' built and registered\n\nDirectory: {}{}",
                        args.name, tool_dir, deps_msg
                    )
                }
            }
            Err(e) => format!("Registration error: {}", e),
        }
    }

    // ==================== SCRIPT TOOLS (Any Language) ====================

    #[tool(
        description = r#"Register a script tool in any language (Python, Node.js, Ruby, Bash, etc.). Scripts communicate via JSON-RPC 2.0 over stdin/stdout. CRITICAL: Use sys.stdin.readline() NOT sys.stdin.read() in Python scripts - read() blocks forever! Always call sys.stdout.flush() after printing.

Bidirectional Features (scripts can REQUEST from host):
- **Elicitation**: Request user input via {"jsonrpc":"2.0","method":"elicitation/create","params":{"message":"prompt","requestedSchema":{"type":"object","properties":{"field":{"type":"string"}}}},"id":1}
- **Sampling**: Request LLM completion via {"jsonrpc":"2.0","method":"sampling/createMessage","params":{"messages":[{"role":"user","content":{"type":"text","text":"prompt"}}],"maxTokens":100},"id":1}
- **Memory**: Store/retrieve state via memory/set, memory/get, memory/list
- **Logging**: Send logs via {"jsonrpc":"2.0","method":"logging/message","params":{"level":"info","message":"text"}}
- **Progress**: Report progress via {"jsonrpc":"2.0","method":"progress/update","params":{"current":1,"total":10,"message":"step"}}

Note: Check context.capabilities before using elicitation/sampling - not all clients support them."#
    )]
    async fn register_script(&self, Parameters(args): Parameters<RegisterScriptArgs>) -> String {
        eprintln!("Registering script tool: {}", args.name);

        // Check if tool exists
        if self.registry.get_tool(&args.name).is_some() && !args.overwrite.unwrap_or(false) {
            return format!(
                "Error: Tool '{}' already exists. Use overwrite=true to update it.",
                args.name
            );
        }

        // Build manifest
        let mut manifest = registry::ToolManifest::new(
            args.name.clone(),
            args.description.clone(),
            ToolType::Script,
        );
        manifest.interpreter = args.interpreter.clone();
        manifest.input_schema = args
            .input_schema
            .map(registry::ToolSchema::from_value)
            .unwrap_or_default();
        manifest.output_schema = args.output_schema.map(registry::ToolSchema::from_value);
        manifest.annotations = args.annotations.map(registry::ToolAnnotations::from_value);
        manifest.dependencies = args.dependencies.clone().unwrap_or_default();

        // Register the tool (this creates the directory and saves the script)
        let config = match self.registry.register_tool(manifest, args.code.as_bytes()) {
            Ok(c) => c,
            Err(e) => return format!("Registration error: {}", e),
        };

        // Handle dependencies
        let mut deps_message = String::new();
        if !config.manifest.dependencies.is_empty() {
            let tool_env_path = self.registry.tool_env_path(&args.name);

            match runtime::install_tool_deps(
                &tool_env_path,
                args.interpreter.as_deref(),
                &config.manifest.dependencies,
            ) {
                Ok(result) => {
                    if result.success {
                        let _ = self.registry.mark_deps_installed(&args.name);
                        deps_message = format!(
                            "\n\n📦 Dependencies installed: {:?}",
                            config.manifest.dependencies
                        );
                    } else {
                        deps_message =
                            format!("\n\n⚠️ Dependency install failed: {}", result.message);
                    }
                }
                Err(e) => {
                    deps_message = format!("\n\n⚠️ Dependency install error: {}", e);
                }
            }
        }

        let interpreter_info = args
            .interpreter
            .map(|i| format!(" (via {})", i))
            .unwrap_or_default();

        let tool_dir = config.tool_dir.display();
        if args.overwrite.unwrap_or(false) {
            format!(
                "📜 Script Tool '{}'{} updated successfully\n\nDirectory: {}{}",
                args.name, interpreter_info, tool_dir, deps_message
            )
        } else {
            format!(
                "📜 Script Tool '{}'{} registered\n\nDirectory: {}{}",
                args.name, interpreter_info, tool_dir, deps_message
            )
        }
    }

    #[tool(description = "Delete a registered tool and clean up its files")]
    async fn delete_tool(&self, Parameters(args): Parameters<DeleteToolArgs>) -> String {
        match self.registry.delete_tool(&args.tool_name) {
            Ok(true) => format!("🗑️ Tool '{}' deleted successfully", args.tool_name),
            Ok(false) => format!("Tool '{}' not found", args.tool_name),
            Err(e) => format!("Error deleting tool: {}", e),
        }
    }

    // ==================== VERSIONING ====================

    #[tool(
        description = r#"Manage tool versions. Actions: 'list' (show versions), 'rollback' (restore version), 'info' (current version).

Tools are automatically versioned:
- Updates auto-backup current version
- Version auto-increments on update
- Rollback restores any previous version

Example: `version(action: "rollback", tool_name: "my_tool", version: "1.0.0")`"#
    )]
    async fn version(&self, Parameters(args): Parameters<VersionArgs>) -> String {
        match args.action {
            VersionAction::List => {
                let name = match args.tool_name {
                    Some(n) => n,
                    None => return "❌ tool_name is required for 'list' action".to_string(),
                };
                match self.registry.list_versions(&name) {
                    Ok(versions) if versions.is_empty() => {
                        format!("No versions found for '{}'", name)
                    }
                    Ok(versions) => {
                        let mut output = format!("## 📦 Versions of '{}'\n\n", name);
                        for v in versions {
                            output.push_str(&format!("- {}\n", v));
                        }
                        output
                    }
                    Err(e) => format!("❌ Error listing versions: {}", e),
                }
            }
            VersionAction::Rollback => {
                let name = match args.tool_name {
                    Some(n) => n,
                    None => return "❌ tool_name is required for 'rollback' action".to_string(),
                };
                let version = match args.version {
                    Some(v) => v,
                    None => return "❌ version is required for 'rollback' action".to_string(),
                };
                match self.registry.rollback(&name, &version) {
                    Ok(msg) => format!("✅ {}", msg),
                    Err(e) => format!("❌ Rollback failed: {}", e),
                }
            }
            VersionAction::Info => {
                let name = match args.tool_name {
                    Some(n) => n,
                    None => return "❌ tool_name is required for 'info' action".to_string(),
                };
                match self.registry.get_tool(&name) {
                    Some(tool) => {
                        let versions = self.registry.list_versions(&name).unwrap_or_default();
                        format!(
                            "## 📦 {} v{}\n\n\
                            - **Type:** {:?}\n\
                            - **Created:** {}\n\
                            - **Updated:** {}\n\
                            - **Available versions:** {}\n",
                            name,
                            tool.manifest.version,
                            tool.manifest.tool_type,
                            tool.manifest.created_at.as_deref().unwrap_or("unknown"),
                            tool.manifest.updated_at.as_deref().unwrap_or("unknown"),
                            versions.len()
                        )
                    }
                    None => format!("Tool '{}' not found", name),
                }
            }
        }
    }

    // ==================== TOOL EXECUTION ====================

    #[tool(
        description = "Call a registered tool (WASM or Script). For script tools, arguments are passed via JSON-RPC 2.0."
    )]
    #[doc = "NOTE: This tool can ONLY call tools registered within Skillz, not tools from other MCP servers."]
    async fn call_tool(&self, Parameters(args): Parameters<CallToolArgs>) -> String {
        eprintln!("Calling tool: {}", args.tool_name);
        
        let tool = match self.registry.get_tool(&args.tool_name) {
            Some(t) => t,
            None => return format!("Error: Tool '{}' not found", args.tool_name),
        };
        
        let tool_args = args.arguments.unwrap_or(serde_json::json!({}));

        // Handle pipeline tools specially
        if *tool.tool_type() == ToolType::Pipeline {
            return self.execute_pipeline(&tool, tool_args).await;
        }

        let tool_config = tool.clone();
        let mut runtime = self.runtime.clone();

        // Update runtime with actual MCP client capabilities
        let mcp_caps = self.get_client_caps().await;
        runtime.update_capabilities(runtime::ClientCapabilities {
            sampling: mcp_caps.sampling,
            elicitation: mcp_caps.elicitation,
            memory: true,    // Memory is always available (server-side)
            resources: true, // Resources are always available (server-side)
        });

        // Use spawn_blocking for sync operations
        match tokio::task::spawn_blocking(move || runtime.call_tool(&tool_config, tool_args)).await
        {
            Ok(Ok(result)) => result.to_string(),
            Ok(Err(e)) => format!("Error executing tool: {}", e),
            Err(e) => format!("Task join error: {}", e),
        }
    }

    /// Execute a pipeline tool
    async fn execute_pipeline(
        &self,
        tool: &registry::ToolConfig,
        input: serde_json::Value,
    ) -> String {
        let start_time = std::time::Instant::now();
        let steps = tool.pipeline_steps();

        let mut step_results: std::collections::HashMap<String, serde_json::Value> =
            std::collections::HashMap::new();
        let mut prev_output: Option<serde_json::Value> = None;
        let mut results: Vec<pipeline::StepResult> = Vec::new();
        let mut pipeline_success = true;

        for (i, step) in steps.iter().enumerate() {
            let step_start = std::time::Instant::now();

            // Check condition
            if let Some(ref condition) = step.condition {
                match pipeline::PipelineExecutor::evaluate_condition(
                    condition,
                    &input,
                    &step_results,
                    prev_output.as_ref(),
                ) {
                    Ok(true) => {}
                    Ok(false) => {
                        results.push(pipeline::StepResult {
                            step_index: i,
                            step_name: step.name.clone(),
                            tool: step.tool.clone(),
                            success: true,
                            output: serde_json::json!({"skipped": true, "reason": "condition not met"}),
                            error: None,
                            duration_ms: 0,
                        });
                        continue;
                    }
                    Err(e) => {
                        results.push(pipeline::StepResult {
                            step_index: i,
                            step_name: step.name.clone(),
                            tool: step.tool.clone(),
                            success: false,
                            output: serde_json::json!(null),
                            error: Some(format!("Condition evaluation failed: {}", e)),
                            duration_ms: step_start.elapsed().as_millis() as u64,
                        });
                        if !step.continue_on_error {
                            pipeline_success = false;
                            break;
                        }
                        continue;
                    }
                }
            }

            // Resolve arguments
            let resolved_args = match pipeline::PipelineExecutor::resolve_args(
                &step.args,
                &input,
                &step_results,
                prev_output.as_ref(),
            ) {
                Ok(args) => args,
                Err(e) => {
                    results.push(pipeline::StepResult {
                        step_index: i,
                        step_name: step.name.clone(),
                        tool: step.tool.clone(),
                        success: false,
                        output: serde_json::json!(null),
                        error: Some(format!("Failed to resolve arguments: {}", e)),
                        duration_ms: step_start.elapsed().as_millis() as u64,
                    });
                    if !step.continue_on_error {
                        pipeline_success = false;
                        break;
                    }
                    continue;
                }
            };

            // Execute the step's tool
            let result = self
                .runtime
                .call_tool_by_name(&step.tool, Some(resolved_args), &self.registry)
                .await;
            let duration_ms = step_start.elapsed().as_millis() as u64;

            let (success, output, error) = match result {
                Ok(output_value) => {
                    // Result is already a Value, no parsing needed
                    (true, output_value, None)
                }
                Err(e) => (false, serde_json::json!(null), Some(e.to_string())),
            };

            let step_result = pipeline::StepResult {
                step_index: i,
                step_name: step.name.clone(),
                tool: step.tool.clone(),
                success,
                output: output.clone(),
                error,
                duration_ms,
            };

            if let Some(ref name) = step.name {
                step_results.insert(name.clone(), output.clone());
            }
            prev_output = Some(output);

            let step_failed = !step_result.success;
            results.push(step_result);

            if step_failed && !step.continue_on_error {
                pipeline_success = false;
                break;
            }
        }

        let total_duration_ms = start_time.elapsed().as_millis() as u64;

        // Format result
        let mut output = format!(
            "## {} Pipeline '{}' {}\n\n**Duration:** {}ms\n\n### Steps:\n\n",
            if pipeline_success { "✅" } else { "❌" },
            tool.name(),
            if pipeline_success {
                "Completed"
        } else {
                "Failed"
            },
            total_duration_ms
        );

        for result in &results {
            let status = if result.success { "✅" } else { "❌" };
            let default_name = format!("step_{}", result.step_index + 1);
            let name = result.step_name.as_deref().unwrap_or(&default_name);
            output.push_str(&format!(
                "**{} {}** ({}) - {}ms\n",
                status, name, result.tool, result.duration_ms
            ));

            if let Some(ref err) = result.error {
                output.push_str(&format!("  Error: {}\n", err));
            } else {
                let output_str = serde_json::to_string_pretty(&result.output).unwrap_or_default();
                if output_str.len() > 200 {
                    output.push_str(&format!("  Output: {}...\n", &output_str[..200]));
                } else {
                    output.push_str(&format!("  Output: {}\n", output_str));
                }
            }
            output.push('\n');
        }

        if let Some(last) = results.last() {
            output.push_str(&format!(
                "### Final Result:\n```json\n{}\n```",
                serde_json::to_string_pretty(&last.output).unwrap_or_default()
            ));
        }

        output
    }

    // ==================== TOOL LISTING ====================

    #[tool(description = "List all available tools (both WASM and Script tools)")]
    async fn list_tools(&self) -> String {
        let tools = self.registry.list_tools();
        if tools.is_empty() {
            return "No tools registered yet.\n\n• Use `build_tool` to create Rust/WASM tools\n• Use `register_script` to create tools in any language".to_string();
        }

        let wasm_tools: Vec<_> = tools
            .iter()
            .filter(|t| *t.tool_type() == ToolType::Wasm)
            .collect();
        let script_tools: Vec<_> = tools
            .iter()
            .filter(|t| *t.tool_type() == ToolType::Script)
            .collect();

        let mut output = format!("📦 Available Tools ({} total)\n\n", tools.len());

        if !wasm_tools.is_empty() {
            output.push_str(&format!("### 🦀 WASM Tools ({})\n\n", wasm_tools.len()));
            for tool in wasm_tools {
                output.push_str(&format!("• **{}** - {}\n", tool.name(), tool.description()));
            }
            output.push('\n');
        }

        if !script_tools.is_empty() {
            output.push_str(&format!("### 📜 Script Tools ({})\n\n", script_tools.len()));
            for tool in script_tools {
                let interpreter = tool.interpreter().unwrap_or("executable");
                output.push_str(&format!(
                    "• **{}** [{}] - {}\n",
                    tool.name(),
                    interpreter,
                    tool.description()
                ));
            }
            output.push('\n');
        }

        output.push_str("\n💡 Use `call_tool(tool_name: \"...\")` to execute any tool.");
        output
    }

    // ==================== CODE EXECUTION MODE ====================

    #[tool(
        description = "Execute code that can call multiple registered tools. Dramatically reduces token usage by composing tools in code instead of sequential calls. Supports Python (default) and JavaScript."
    )]
    #[doc = "NOTE: This tool can ONLY access tools registered within Skillz, not tools from other MCP servers."]
    async fn execute_code(&self, Parameters(args): Parameters<ExecuteCodeArgs>) -> String {
        let language = args.language.as_deref().unwrap_or("python");
        let _timeout = args.timeout.unwrap_or(30); // TODO: Implement timeout

        // Get available tools
        let available_tools: Vec<_> = if let Some(ref tool_names) = args.tools {
            self.registry
                .list_tools()
                .into_iter()
                .filter(|t| tool_names.contains(&t.name().to_string()))
                .collect()
        } else {
            self.registry.list_tools()
        };

        // Generate tool API stubs
        let tool_stubs = self.generate_tool_stubs(&available_tools, language);

        // Create the execution script
        let script = match language {
            "python" | "python3" => {
                self.wrap_python_code(&args.code, &tool_stubs, &available_tools)
            }
            "javascript" | "js" | "node" => {
                self.wrap_javascript_code(&args.code, &tool_stubs, &available_tools)
            }
            _ => {
                return format!(
                    "❌ Unsupported language: {}. Use 'python' or 'javascript'.",
                    language
                )
            }
        };

        // Execute in sandbox
        let interpreter = match language {
            "python" | "python3" => "python3",
            "javascript" | "js" | "node" => "node",
            _ => "python3",
        };

        // Create temp file
        let ext = match language {
            "python" | "python3" => "py",
            "javascript" | "js" | "node" => "js",
            _ => "py",
        };

        let temp_dir = std::env::temp_dir();
        let script_path = temp_dir.join(format!("skillz_exec_{}.{}", std::process::id(), ext));

        if let Err(e) = std::fs::write(&script_path, &script) {
            return format!("❌ Failed to create execution script: {}", e);
        }

        // Execute with timeout
        let output = std::process::Command::new(interpreter)
            .arg(&script_path)
            .output();

        // Cleanup
        let _ = std::fs::remove_file(&script_path);

        match output {
            Ok(result) => {
                let stdout = String::from_utf8_lossy(&result.stdout);
                let stderr = String::from_utf8_lossy(&result.stderr);

                if result.status.success() {
                    if stderr.is_empty() {
                        format!("✅ **Execution Result**\n\n```\n{}\n```", stdout.trim())
        } else {
                        format!(
                            "✅ **Execution Result**\n\n```\n{}\n```\n\n**Logs:**\n```\n{}\n```",
                            stdout.trim(),
                            stderr.trim()
                        )
                    }
                } else {
                    format!(
                        "❌ **Execution Failed**\n\n**Error:**\n```\n{}\n```\n\n**Output:**\n```\n{}\n```",
                        stderr.trim(),
                        stdout.trim()
                    )
                }
            }
            Err(e) => format!("❌ Failed to execute: {}", e),
        }
    }

    // ==================== TOOL IMPORT ====================

    #[tool(
        description = "Import a tool from an external source (git repository or GitHub gist). Supports: git URLs (https://github.com/user/repo), branch specifiers (url#branch), and gists (gist:ID or https://gist.github.com/user/ID)."
    )]
    async fn import_tool(&self, Parameters(args): Parameters<ImportToolArgs>) -> String {
        eprintln!("Importing tool from: {}", args.source);

        let importer = importer::Importer::new(self.registry.storage_dir().to_path_buf());

        match importer.import(
            &args.source,
            &self.registry,
            args.overwrite.unwrap_or(false),
        ) {
            Ok(result) => {
                // Reload registry to pick up the new tool immediately
                self.registry.reload();

                format!(
                    "✅ **Tool Imported Successfully**\n\n\
                    - **Name:** {}\n\
                    - **Type:** {:?}\n\
                    - **Source:** {}\n\n\
                    {}\n\n\
                    🎉 Tool is ready to use! Call it with `call_tool(tool_name: \"{}\")`",
                    result.tool_name,
                    result.tool_type,
                    result.source,
                    result.message,
                    result.tool_name
                )
            }
            Err(e) => {
                format!(
                    "❌ **Import Failed**\n\n\
                    **Source:** {}\n\
                    **Error:** {}\n\n\
                    **Supported formats:**\n\
                    - Git: `https://github.com/user/repo` or `https://github.com/user/repo#branch`\n\
                    - Gist: `gist:GIST_ID` or `https://gist.github.com/user/GIST_ID`",
                    args.source,
                    e
                )
            }
        }
    }

    // ==================== PIPELINES ====================

    #[tool(
        description = r#"Create and manage pipeline tools. Pipelines chain tools together with outputs available to subsequent steps.

Actions: 'create', 'list', 'delete'

NOTE: Pipelines can ONLY use Skillz's own registered tools, not tools from other MCP servers.

Variable syntax (for create):
- $input.field - Access pipeline input
- $prev - Previous step's entire output
- $prev.field - Access field from previous step
- $step_name.field - Access field from a named step

Example:
pipeline(action: "create", name: "my_pipeline", steps: [
    { name: "fetch", tool: "http_get", args: { url: "$input.url" } },
    { tool: "analyze", args: { text: "$fetch.body" } }
])"#
    )]
    async fn pipeline(&self, Parameters(args): Parameters<PipelineArgs>) -> String {
        match args.action.as_str() {
            "create" => {
                let name = match &args.name {
                    Some(n) => n.clone(),
                    None => return "Error: 'name' is required for create action".to_string(),
                };
                let steps = match &args.steps {
                    Some(s) => s.clone(),
                    None => return "Error: 'steps' is required for create action".to_string(),
                };

                eprintln!("Creating pipeline: {}", name);

                // Check if tool already exists
                if self.registry.get_tool(&name).is_some() {
                    return format!(
                        "❌ A tool named '{}' already exists. Choose a different name.",
                        name
                    );
                }

                // Validate steps reference existing tools
                for (i, step) in steps.iter().enumerate() {
                    if self.registry.get_tool(&step.tool).is_none() {
                        let built_in_tools = [
                            "build_tool",
                            "register_script",
                            "create_skill",
                            "import_tool",
                            "call_tool",
                            "list_tools",
                            "complete",
                            "execute_code",
                            "install_deps",
                            "delete_tool",
                            "test_validate",
                            "pipeline",
                            "memory",
                        ];
                        if !built_in_tools.contains(&step.tool.as_str()) {
                            return format!(
                                "❌ Step {} references unknown tool '{}'. Create or import it first.",
                                i + 1, step.tool
                            );
                        }
                    }
                }

                // Convert to registry PipelineStep
                let reg_steps: Vec<registry::PipelineStep> = steps
                    .iter()
                    .map(|s| registry::PipelineStep {
                        name: s.name.clone(),
                        tool: s.tool.clone(),
                        args: s.args.clone().unwrap_or(serde_json::json!({})),
                        continue_on_error: s.continue_on_error.unwrap_or(false),
                        condition: s.condition.clone(),
                    })
                    .collect();

                let mut manifest = registry::ToolManifest::new_pipeline(
                    name.clone(),
                    args.description.unwrap_or_default(),
                    reg_steps,
                );
                manifest.tags = args.tags.unwrap_or_default();

                match self.registry.register_tool(manifest, &[]) {
                    Ok(_) => format!(
                        "✅ **Pipeline '{}' Created**\n\nUse `call_tool(tool_name: \"{}\")` to run it",
                        name, name
                    ),
                    Err(e) => format!("❌ Failed to create pipeline: {}", e),
                }
            }
            "list" => {
                let all_tools = self.registry.list_tools();
                let pipelines: Vec<_> = all_tools
                    .iter()
                    .filter(|t| *t.tool_type() == ToolType::Pipeline)
                    .collect();

                let filtered: Vec<_> = if let Some(ref tag) = args.tag {
                    pipelines
                        .into_iter()
                        .filter(|p| p.manifest.tags.contains(tag))
                        .collect()
        } else {
                    pipelines
                };

                if filtered.is_empty() {
                    return "📭 No pipelines found. Create one with `pipeline(action: \"create\", ...)`.".to_string();
                }

                let mut output = format!("## 📋 Pipelines ({})\n\n", filtered.len());
                for p in filtered {
                    output.push_str(&format!(
                        "### {}\n- **Description:** {}\n- **Steps:** {}\n- **Tags:** {}\n\n",
                        p.name(),
                        if p.description().is_empty() {
                            "(none)"
                        } else {
                            p.description()
                        },
                        p.pipeline_steps().len(),
                        if p.manifest.tags.is_empty() {
                            "(none)".to_string()
                        } else {
                            p.manifest.tags.join(", ")
                        }
                    ));
                }
                output
            }
            "delete" => {
                let name = match &args.name {
                    Some(n) => n,
                    None => return "Error: 'name' is required for delete action".to_string(),
                };

                if let Some(tool) = self.registry.get_tool(name) {
                    if *tool.tool_type() != ToolType::Pipeline {
                        return format!(
                            "⚠️ '{}' is not a pipeline. Use delete_tool instead.",
                            name
                        );
                    }
                }

                match self.registry.delete_tool(name) {
                    Ok(true) => format!("🗑️ Pipeline '{}' deleted successfully", name),
                    Ok(false) => format!("⚠️ Pipeline '{}' not found", name),
                    Err(e) => format!("❌ Failed to delete pipeline: {}", e),
                }
            }
            _ => format!(
                "Unknown action: '{}'. Use: create, list, delete",
                args.action
            ),
        }
    }

    // ==================== MEMORY / PERSISTENT STATE ====================

    #[tool(
        description = r#"Manage knowledge entries. Actions: 'store' (save new), 'get' (by ID), 'update' (modify), 'delete' (remove), 'list' (browse), 'bulk_store' (create multiple), 'bulk_update' (update multiple). For bulk operations, use 'entries' array. Store any text, code, or notes for later retrieval."#
    )]
    async fn memory(&self, Parameters(args): Parameters<MemoryArgs>) -> String {
        match args.action.as_str() {
            "get" => {
                let key = match &args.key {
                    Some(k) => k,
                    None => return "Error: 'key' is required for get action".to_string(),
                };
                match self.memory.get(&args.tool_name, key).await {
                    Ok(Some(value)) => serde_json::to_string_pretty(&value).unwrap_or_else(|_| "null".to_string()),
                    Ok(None) => "null".to_string(),
                    Err(e) => format!("Error: {}", e),
                }
            }
            "store" | "set" => {
                let key = match &args.key {
                    Some(k) => k,
                    None => return "Error: 'key' is required for store action".to_string(),
                };
                let value = match &args.value {
                    Some(v) => v.clone(),
                    None => return "Error: 'value' is required for store action".to_string(),
                };
                match self.memory.set(&args.tool_name, key, value).await {
                    Ok(()) => format!("✅ Stored '{}' for tool '{}'", key, args.tool_name),
                    Err(e) => format!("Error: {}", e),
                }
            }
            "delete" | "clear" => {
                if let Some(key) = &args.key {
                    // Delete specific key
                    match self.memory.delete(&args.tool_name, key).await {
                        Ok(true) => format!("🗑️ Deleted '{}' from tool '{}'", key, args.tool_name),
                        Ok(false) => format!("Key '{}' not found for tool '{}'", key, args.tool_name),
                        Err(e) => format!("Error: {}", e),
                    }
                } else {
                    // Clear all keys for tool
                    match self.memory.clear(&args.tool_name).await {
                        Ok(count) => format!("🗑️ Cleared {} entries for tool '{}'", count, args.tool_name),
                        Err(e) => format!("Error: {}", e),
                    }
                }
            }
            "list" => {
                match self.memory.list_keys(&args.tool_name).await {
                    Ok(keys) => {
                        if keys.is_empty() {
                            format!("No memory stored for tool '{}'", args.tool_name)
                        } else {
                            format!("Keys for '{}': {}", args.tool_name, keys.join(", "))
                        }
                    }
                    Err(e) => format!("Error: {}", e),
                }
            }
            "stats" => {
                match self.memory.stats().await {
                    Ok(stats) => format!(
                        "📊 Memory Stats:\n  - Total entries: {}\n  - Tools with memory: {}\n  - Schema version: {}",
                        stats.total_entries, stats.total_tools, stats.schema_version
                    ),
                    Err(e) => format!("Error: {}", e),
                }
            }
            _ => format!("Unknown action: '{}'. Use: store, get, delete, list, stats", args.action),
        }
    }
}

impl AppState {
    fn generate_tool_stubs(&self, tools: &[registry::ToolConfig], language: &str) -> String {
        let mut stubs = String::new();

        for tool in tools {
            match language {
                "python" | "python3" => {
                    stubs.push_str(&format!(
                        r#"
def {}(**kwargs):
    """{}"""
    return _call_tool("{}", kwargs)
"#,
                        tool.name().replace('-', "_"),
                        tool.description(),
                        tool.name()
                    ));
                }
                "javascript" | "js" | "node" => {
                    stubs.push_str(&format!(
                        r#"
function {}(args) {{
    /** {} */
    return _call_tool("{}", args || {{}});
}}
"#,
                        tool.name().replace('-', "_"),
                        tool.description(),
                        tool.name()
                    ));
                }
                _ => {}
            }
        }

        stubs
    }

    fn wrap_python_code(
        &self,
        user_code: &str,
        tool_stubs: &str,
        tools: &[registry::ToolConfig],
    ) -> String {
        let tool_names: Vec<_> = tools.iter().map(|t| format!("\"{}\"", t.name())).collect();

        format!(
            r#"#!/usr/bin/env python3
import json
import subprocess
import sys

# Tool registry
_TOOLS = [{}]

def _call_tool(name, args):
    """Call a registered Skillz tool"""
    # For now, print the tool call (in production, this would use JSON-RPC)
    print(f"[TOOL_CALL] {{name}}: {{json.dumps(args)}}")
    return {{"status": "called", "tool": name, "args": args}}

# Generated tool stubs
{}

# User code
{}
"#,
            tool_names.join(", "),
            tool_stubs,
            user_code
        )
    }

    fn wrap_javascript_code(
        &self,
        user_code: &str,
        tool_stubs: &str,
        tools: &[registry::ToolConfig],
    ) -> String {
        let tool_names: Vec<_> = tools.iter().map(|t| format!("\"{}\"", t.name())).collect();

        format!(
            r#"#!/usr/bin/env node
// Tool registry
const _TOOLS = [{}];

function _call_tool(name, args) {{
    // Call a registered Skillz tool
    console.log(`[TOOL_CALL] ${{name}}: ${{JSON.stringify(args)}}`);
    return {{status: "called", tool: name, args: args}};
}}

// Generated tool stubs
{}

// User code
{}
"#,
            tool_names.join(", "),
            tool_stubs,
            user_code
        )
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AppState {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("Skillz - Build and execute custom tools at runtime. Supports WASM (Rust) and Script tools (Python, Node.js, Ruby, etc.) via JSON-RPC 2.0. NOTE: Skillz tools can ONLY call other Skillz tools, not tools from other MCP servers. CRITICAL: For Python scripts, use sys.stdin.readline() NOT sys.stdin.read() - read() blocks forever! Always call sys.stdout.flush() after printing.".into()),
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                // Note: logging disabled temporarily due to Cursor sending setLevel before initialized
                // which violates MCP spec (setLevel should come AFTER initialized notification)
                // .enable_logging()
                .build(),
            ..Default::default()
        }
    }

    /// Called when a client initializes - capture the peer for logging
    async fn on_initialized(&self, ctx: NotificationContext<RoleServer>) {
        // Get client info to check capabilities
        if let Some(client_info) = ctx.peer.peer_info() {
            eprintln!("MCP client initialized: {:?}", client_info.client_info.name);

            // Check and store client capabilities
            let caps = &client_info.capabilities;
            let mcp_caps = McpClientCapabilities {
                sampling: caps.sampling.is_some(),
                elicitation: caps.elicitation.is_some(),
                roots: caps.roots.is_some(),
            };

            eprintln!("  Client capabilities:");
            eprintln!("    - sampling: {}", mcp_caps.sampling);
            eprintln!("    - elicitation: {}", mcp_caps.elicitation);
            eprintln!("    - roots: {}", mcp_caps.roots);

            self.update_client_caps(mcp_caps).await;
        } else {
            eprintln!("MCP client initialized (no client info available)");
        }

        self.update_peer(ctx.peer).await;
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParam>,
        _ctx: RequestContext<RoleServer>,
    ) -> std::result::Result<ListResourcesResult, McpError> {
        let mut resources = vec![
            RawResource::new(
                "skillz://guide",
                "Skillz Guide - Complete usage guide".to_string(),
            )
            .no_annotation(),
            RawResource::new(
                "skillz://examples",
                "Skillz Examples - Code snippets for WASM and Script tools".to_string(),
            )
            .no_annotation(),
            RawResource::new(
                "skillz://protocol",
                "JSON-RPC 2.0 Protocol - How script tools communicate".to_string(),
            )
            .no_annotation(),
        ];

        // Add dynamic resources for each built tool
        for tool in self.registry.list_tools() {
            let type_emoji = match tool.tool_type() {
                ToolType::Wasm => "🦀",
                ToolType::Script => "📜",
                ToolType::Pipeline => "⛓️",
            };
            resources.push(
                RawResource::new(
                    format!("skillz://tools/{}", tool.name()),
                    format!("{} {} - {}", type_emoji, tool.name(), tool.description()),
                )
                .no_annotation(),
            );
        }

        Ok(ListResourcesResult {
            resources,
            next_cursor: None,
        })
    }

    async fn read_resource(
        &self,
        ReadResourceRequestParam { uri }: ReadResourceRequestParam,
        _ctx: RequestContext<RoleServer>,
    ) -> std::result::Result<ReadResourceResult, McpError> {
        let content = match uri.as_str() {
            "skillz://guide" => self.get_guide_content(),
            "skillz://examples" => self.get_examples_content(),
            "skillz://protocol" => self.get_protocol_content(),
            _ if uri.starts_with("skillz://tools/") => {
                let tool_name = uri.strip_prefix("skillz://tools/").unwrap();
                self.get_tool_info(tool_name)
            }
            _ => {
                return Err(McpError::resource_not_found(
                    "Resource not found",
                    Some(serde_json::json!({ "uri": uri })),
                ));
            }
        };

        Ok(ReadResourceResult {
            contents: vec![ResourceContents::text(content, uri)],
        })
    }
}

impl AppState {
    fn get_guide_content(&self) -> String {
        let tools = self.registry.list_tools();
        let wasm_tools: Vec<_> = tools
            .iter()
            .filter(|t| *t.tool_type() == ToolType::Wasm)
            .collect();
        let script_tools: Vec<_> = tools
            .iter()
            .filter(|t| *t.tool_type() == ToolType::Script)
            .collect();

        let mut guide = String::from(
            r##"# 🚀 Skillz Guide

## Overview
Skillz is a self-extending MCP server that allows you to build and execute custom tools at runtime.

**Two types of tools:**
- 🦀 **WASM Tools** - Compiled from Rust, run in a WebAssembly sandbox
- 📜 **Script Tools** - Any language (Python, Node.js, Ruby, etc.) via JSON-RPC 2.0

---

## Built-in Tools

### `build_tool` - Create WASM tools from Rust
```
build_tool(name: "my_tool", code: "fn main() {...}", description: "...")
```

### `register_script` - Register any-language tools
```
register_script(name: "my_tool", code: "...", interpreter: "python3", description: "...")
```

### `create_skill` - Step-by-step skill creation
```
create_skill(name: "my_tool", description: "...", step: 1, content: "...", skill_type: "wasm")
```

### `call_tool` - Execute any registered tool
```
call_tool(tool_name: "my_tool", arguments: {...})
```

### `list_tools` - List all registered tools
### `test_validate` - Validate Rust code before building
### `delete_tool` - Remove a registered tool

---

## 🧠 Memory (Persistent State)

Tools can store and retrieve data that persists across sessions using libSQL/SQLite.

### `memory_set` - Store a value
```
memory_set(tool_name: "my_tool", key: "counter", value: 42)
memory_set(tool_name: "my_tool", key: "config", value: {"theme": "dark"})
```

### `memory_get` - Retrieve a value
```
memory_get(tool_name: "my_tool", key: "counter")  // Returns: 42
```

### `memory_list` - List all keys for a tool
```
memory_list(tool_name: "my_tool")  // Returns: counter, config
```

### `memory_clear` - Clear memory
```
memory_clear(tool_name: "my_tool")  // Clear one tool's memory
memory_clear()                       // Clear ALL memory
```

### `memory_stats` - Get statistics
```
memory_stats()  // Total entries, tools, schema version
```

---

"##,
        );

        // Add dynamic registered tools section
        if !wasm_tools.is_empty() || !script_tools.is_empty() {
            guide.push_str("## 📦 Registered Tools\n\n");

            if !wasm_tools.is_empty() {
                guide.push_str("### 🦀 WASM Tools\n\n");
                for tool in &wasm_tools {
                    guide.push_str(&format!("- **{}** - {}\n", tool.name(), tool.description()));
                    guide.push_str(&format!(
                        "  ```\n  call_tool(tool_name: \"{}\")\n  ```\n\n",
                        tool.name()
                    ));
                }
            }

            if !script_tools.is_empty() {
                guide.push_str("### 📜 Script Tools\n\n");
                for tool in &script_tools {
                    let interp = tool.interpreter().unwrap_or("executable");
                    guide.push_str(&format!(
                        "- **{}** [{}] - {}\n",
                        tool.name(),
                        interp,
                        tool.description()
                    ));
                    guide.push_str(&format!(
                        "  ```\n  call_tool(tool_name: \"{}\")\n  ```\n\n",
                        tool.name()
                    ));
                }
            }

            guide.push_str("---\n\n");
        }

        guide.push_str(
            r##"## Quick Start

### Creating a WASM Tool (Rust)
1. Write Rust code with `fn main()`
2. Use `build_tool` to compile
3. Use `call_tool` to execute

### Creating a Script Tool (Any Language)
1. Write script following JSON-RPC 2.0 protocol
2. Use `register_script` with appropriate interpreter
3. Use `call_tool` to execute

### Step-by-Step Creation
1. `create_skill` with step=1 (Design)
2. `create_skill` with step=2 (Implement)
3. `create_skill` with step=3 (Test)
4. `create_skill` with step=4 (Finalize)

---

## Resources

- `skillz://guide` - This guide (updates with new tools!)
- `skillz://examples` - Code examples for WASM and Script tools
- `skillz://protocol` - JSON-RPC 2.0 protocol documentation
- `skillz://tools/{name}` - Individual tool documentation
"##,
        );

        guide
    }

    fn get_examples_content(&self) -> String {
        r##"# 📝 Skillz Code Examples

## 🦀 WASM Tools (Rust)

### Hello World
```rust
fn main() {
    println!("Hello from WASM!");
}
```

### Fibonacci
```rust
fn main() {
    let mut a = 0u64;
    let mut b = 1u64;
    for _ in 0..20 {
        print!("{} ", a);
        let temp = a + b;
        a = b;
        b = temp;
    }
    println!();
}
```

---

## 📜 Script Tools (Any Language)

### Python Example
```python
#!/usr/bin/env python3
import json
import sys

def main():
    # IMPORTANT: Use readline() not read()!
    # read() blocks waiting for EOF and causes timeouts
    request = json.loads(sys.stdin.readline())
    args = request.get("params", {}).get("arguments", {})
    
    # Your tool logic here
    result = {"message": "Hello from Python!", "args": args}
    
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

### Node.js Example
```javascript
// Use readline to read one line at a time (not stdin.read which blocks)
const readline = require('readline');

const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
    terminal: false
});

rl.on('line', (line) => {
    const request = JSON.parse(line);
    const args = request.params?.arguments || {};
    
    // Your tool logic here
    const result = { message: "Hello from Node.js!", args };
    
    console.log(JSON.stringify({
        jsonrpc: "2.0",
        result: result,
        id: request.id
    }));
    process.exit(0);
});
```

### Bash Example
```bash
#!/bin/bash
read request
# Simple echo back
echo '{"jsonrpc":"2.0","result":{"message":"Hello from Bash!"},"id":1}'
```

### Ruby Example
```ruby
#!/usr/bin/env ruby
require 'json'

# Use gets (reads one line) instead of read (waits for EOF)
request = JSON.parse(STDIN.gets)
args = request.dig("params", "arguments") || {}
result = { message: "Hello from Ruby!", args: args }
response = { jsonrpc: "2.0", result: result, id: request["id"] }
puts response.to_json
STDOUT.flush
```
"##
        .to_string()
    }

    fn get_protocol_content(&self) -> String {
        r##"# 📡 JSON-RPC 2.0 Protocol for Script Tools

## Overview
Script tools communicate via JSON-RPC 2.0 over stdin/stdout, similar to MCP servers.

**Features:**
- 📁 **Roots** - Access to workspace directories
- 📝 **Logging** - Emit log messages during execution
- 📊 **Progress** - Report progress updates
- 🔧 **Context** - Environment and tool information
- 🧠 **Memory** - Store/retrieve persistent data
- 📦 **Resources** - List and read server resources
- 🎤 **Elicitation** - Request user input (if client supports)
- 🤖 **Sampling** - Request LLM completions (if client supports)

---

## Request Format (with Roots, Context & Capabilities)

When Skillz calls your tool, it sends:

```json
{
    "jsonrpc": "2.0",
    "method": "execute",
    "params": {
        "arguments": {
            // User-provided arguments from call_tool
        },
        "context": {
            "roots": ["/path/to/workspace", "/another/path"],
            "working_directory": "/current/dir",
            "tool_name": "my_tool",
            "tools_dir": "/path/to/tools",
            "environment": {"HOME": "...", "USER": "...", "PATH": "..."},
            "capabilities": {
                "sampling": true,
                "elicitation": true,
                "memory": true,
                "resources": true
            }
        }
    },
    "id": 1
}
```

### Context Fields
- `roots` - Workspace directories (from MCP client, `SKILLZ_ROOTS` env, or cwd)
- `working_directory` - Current working directory
- `tool_name` - Name of the executing tool
- `tools_dir` - Directory where tools are stored
- `environment` - Environment variables (HOME, USER, PATH, TERM, LANG + all SKILLZ_* vars)
- `capabilities` - What MCP features the client supports

### Environment Variables & Secrets
Tools receive safe env vars plus **all `SKILLZ_*` prefixed variables** for secrets:
```bash
export SKILLZ_OPENAI_KEY="sk-..."
export SKILLZ_API_TOKEN="secret123"
```
Access in your tool:
```python
context = request["params"]["context"]
api_key = context["environment"].get("SKILLZ_OPENAI_KEY")
```

### Configuring Roots
Roots are resolved in this priority order:
1. **MCP Client** - Roots provided by the MCP client (automatic)
2. **SKILLZ_ROOTS env** - Colon-separated paths: `SKILLZ_ROOTS=/path/one:/path/two`
3. **cwd** - Current working directory (fallback)

---

## Logging (like MCP logging)

Scripts can emit log messages during execution:

```json
{"jsonrpc": "2.0", "method": "log", "params": {"level": "info", "message": "Processing..."}}
{"jsonrpc": "2.0", "method": "log", "params": {"level": "warning", "message": "Watch out!"}}
{"jsonrpc": "2.0", "method": "log", "params": {"level": "error", "message": "Something failed", "data": {...}}}
```

**Log Levels:** `debug`, `info`, `warning`, `error`

---

## Progress Updates

Report progress during long operations:

```json
{"jsonrpc": "2.0", "method": "progress", "params": {"current": 50, "total": 100, "message": "Halfway done"}}
```

---

## 🧠 Memory (Persistent State)

Store and retrieve data that persists across sessions.
**Check `context.capabilities.memory` before using!**

All memory operations are isolated per-tool (tool name is automatic).

### Get a Value
```json
{"jsonrpc": "2.0", "method": "memory/get", "params": {"key": "counter"}, "id": 10}
```
**Response:**
```json
{"jsonrpc": "2.0", "result": {"value": 42}, "id": 10}
```

### Set a Value
```json
{"jsonrpc": "2.0", "method": "memory/set", "params": {"key": "counter", "value": 43}, "id": 11}
```
**Response:**
```json
{"jsonrpc": "2.0", "result": {"success": true}, "id": 11}
```

### List All Keys
```json
{"jsonrpc": "2.0", "method": "memory/list", "params": {}, "id": 12}
```
**Response:**
```json
{"jsonrpc": "2.0", "result": {"keys": ["counter", "config", "history"]}, "id": 12}
```

### Delete a Key
```json
{"jsonrpc": "2.0", "method": "memory/delete", "params": {"key": "counter"}, "id": 13}
```
**Response:**
```json
{"jsonrpc": "2.0", "result": {"deleted": true}, "id": 13}
```

### Python Helper Functions
```python
def memory_get(key):
    req = {"jsonrpc": "2.0", "method": "memory/get", "params": {"key": key}, "id": 10}
    print(json.dumps(req), flush=True)
    resp = json.loads(sys.stdin.readline())
    return resp.get("result", {}).get("value")

def memory_set(key, value):
    req = {"jsonrpc": "2.0", "method": "memory/set", "params": {"key": key, "value": value}, "id": 11}
    print(json.dumps(req), flush=True)
    resp = json.loads(sys.stdin.readline())
    return resp.get("result", {}).get("success", False)
```

---

## 📦 Resources (List and Read Server Resources)

Access Skillz resources from your script tool.
**Check `context.capabilities.resources` before using!**

### List Available Resources
```json
{"jsonrpc": "2.0", "method": "resources/list", "params": {}, "id": 20}
```
**Response:**
```json
{
    "jsonrpc": "2.0",
    "result": {
        "resources": [
            {"uri": "skillz://guide", "name": "Skillz Guide", "description": "Complete usage guide"},
            {"uri": "skillz://examples", "name": "Skillz Examples", "description": "Code snippets"},
            {"uri": "skillz://protocol", "name": "JSON-RPC 2.0 Protocol", "description": "How to communicate"},
            {"uri": "skillz://tools/my_tool", "name": "my_tool", "description": "Tool description"}
        ]
    },
    "id": 20
}
```

### Read Resource Content
```json
{"jsonrpc": "2.0", "method": "resources/read", "params": {"uri": "skillz://guide"}, "id": 21}
```
**Response:**
```json
{
    "jsonrpc": "2.0",
    "result": {
        "contents": [
            {"uri": "skillz://guide", "mime_type": "text/markdown", "text": "# 🚀 Skillz Guide..."}
        ]
    },
    "id": 21
}
```

### Python Helper Functions
```python
def resources_list():
    req = {"jsonrpc": "2.0", "method": "resources/list", "params": {}, "id": 20}
    print(json.dumps(req), flush=True)
    resp = json.loads(sys.stdin.readline())
    return resp.get("result", {}).get("resources", [])

def resources_read(uri):
    req = {"jsonrpc": "2.0", "method": "resources/read", "params": {"uri": uri}, "id": 21}
    print(json.dumps(req), flush=True)
    resp = json.loads(sys.stdin.readline())
    contents = resp.get("result", {}).get("contents", [])
    return contents[0].get("text") if contents else None
```

---

## 🎤 Elicitation (User Input)

Request structured input from the user during tool execution.
**Check `context.capabilities.elicitation` before using!**

### Request User Input
```json
{
    "jsonrpc": "2.0",
    "method": "elicitation/create",
    "params": {
        "message": "Please provide your email address",
        "requestedSchema": {
            "type": "object",
            "properties": {
                "email": {"type": "string", "format": "email"},
                "name": {"type": "string"}
            },
            "required": ["email"]
        }
    },
    "id": 100
}
```

### Response from User
```json
{
    "jsonrpc": "2.0",
    "result": {
        "action": "accept",
        "content": {"email": "user@example.com", "name": "John"}
    },
    "id": 100
}
```

**Actions:** `accept` (user provided input), `decline` (user refused), `cancel` (user dismissed)

---

## 🤖 Sampling (LLM Requests)

Request LLM completions during tool execution.
**Check `context.capabilities.sampling` before using!**

### Request LLM Completion
```json
{
    "jsonrpc": "2.0",
    "method": "sampling/createMessage",
    "params": {
        "messages": [
            {"role": "user", "content": {"type": "text", "text": "Summarize this code..."}}
        ],
        "maxTokens": 1000,
        "temperature": 0.7
    },
    "id": 101
}
```

### Response from LLM
```json
{
    "jsonrpc": "2.0",
    "result": {
        "message": {
            "role": "assistant",
            "content": {"type": "text", "text": "This code does..."}
        },
        "model": "claude-3",
        "stopReason": "endTurn"
    },
    "id": 101
}
```

---

## Final Response

### Success
```json
{"jsonrpc": "2.0", "result": {"your": "data"}, "id": 1}
```

### Error
```json
{"jsonrpc": "2.0", "error": {"code": -32000, "message": "Error description", "data": {}}, "id": 1}
```

---

## Complete Python Example with All Features

```python
#!/usr/bin/env python3
import json
import sys

def log(level, message):
    print(json.dumps({"jsonrpc": "2.0", "method": "log", 
        "params": {"level": level, "message": message}}), flush=True)

def resources_list():
    """List available resources"""
    req = {"jsonrpc": "2.0", "method": "resources/list", "params": {}, "id": 20}
    print(json.dumps(req), flush=True)
    resp = json.loads(sys.stdin.readline())
    return resp.get("result", {}).get("resources", [])

def resources_read(uri):
    """Read resource content"""
    req = {"jsonrpc": "2.0", "method": "resources/read", "params": {"uri": uri}, "id": 21}
    print(json.dumps(req), flush=True)
    resp = json.loads(sys.stdin.readline())
    contents = resp.get("result", {}).get("contents", [])
    return contents[0].get("text") if contents else None

def elicit(message, schema):
    """Request user input (check capabilities first!)"""
    req = {"jsonrpc": "2.0", "method": "elicitation/create", 
           "params": {"message": message, "requestedSchema": schema}, "id": 100}
    print(json.dumps(req), flush=True)
    return json.loads(sys.stdin.readline())

def sample(prompt, max_tokens=500):
    """Request LLM completion (check capabilities first!)"""
    req = {"jsonrpc": "2.0", "method": "sampling/createMessage",
           "params": {"messages": [{"role": "user", "content": {"type": "text", "text": prompt}}],
                      "maxTokens": max_tokens}, "id": 101}
    print(json.dumps(req), flush=True)
    return json.loads(sys.stdin.readline())

def main():
    # IMPORTANT: Use readline() not read() - read() blocks for EOF!
    request = json.loads(sys.stdin.readline())
    context = request.get("params", {}).get("context", {})
    caps = context.get("capabilities", {})
    
    log("info", f"Capabilities: {caps}")
    
    # Example: List and read resources (always available)
    if caps.get("resources"):
        resources = resources_list()
        log("info", f"Available resources: {len(resources)}")
        # guide = resources_read("skillz://guide")
    
    # Example: Request user input if supported
    if caps.get("elicitation"):
        log("info", "Elicitation available!")
        # resp = elicit("What's your name?", {"type": "object", "properties": {"name": {"type": "string"}}})
    
    # Example: Request LLM completion if supported  
    if caps.get("sampling"):
        log("info", "Sampling available!")
        # resp = sample("Hello, how are you?")
    
    result = {"message": "Done!", "capabilities_available": caps}
    print(json.dumps({"jsonrpc": "2.0", "result": result, "id": request.get("id")}))

if __name__ == "__main__":
    main()
```

---

## ⚠️ Critical Tips

### Use `readline()` NOT `read()`!
```python
# ✅ CORRECT - Returns immediately
request = json.loads(sys.stdin.readline())

# ❌ WRONG - Blocks forever waiting for EOF!
request = json.loads(sys.stdin.read())
```

### Other Tips
- Check `context.capabilities` before using features
- `memory` and `resources` are always available (server-side)
- `elicitation` and `sampling` depend on MCP client support
- Use `flush=True` to ensure output is sent immediately
- All bidirectional methods require an `id` field for responses
- Log to stderr for debug output that won't interfere with JSON-RPC
- Always include `id` from original request in final response
"##.to_string()
    }

    fn get_tool_info(&self, tool_name: &str) -> String {
        match self.registry.get_tool(tool_name) {
            Some(tool) => {
                let (type_name, type_emoji, path_info) = match tool.tool_type() {
                    ToolType::Wasm => (
                        "WASM",
                        "🦀",
                        format!("Directory: {}", tool.tool_dir.display()),
                    ),
                    ToolType::Script => {
                        let interp = tool.interpreter().unwrap_or("executable");
                        (
                            "Script",
                            "📜",
                            format!(
                                "Directory: {}\nInterpreter: {}",
                                tool.tool_dir.display(),
                                interp
                            ),
                        )
                    }
                    ToolType::Pipeline => (
                        "Pipeline",
                        "⛓️",
                        format!(
                            "Directory: {}\nSteps: {}",
                            tool.tool_dir.display(),
                            tool.pipeline_steps().len()
                        ),
                    ),
                };
                let version_info = format!("- **Version:** {}\n", tool.manifest.version);
                format!(
                    "# {} {} Tool: {}\n\n## Description\n{}\n\n## Details\n- **Type:** {}\n- **Name:** {}\n{}- {}\n- **Status:** ✅ Ready to use\n\n## Usage\n```\ncall_tool(tool_name: \"{}\")\n```\n",
                    type_emoji, type_name, tool.name(),
                    tool.description(),
                    type_name, tool.name(), version_info, path_info,
                    tool.name()
                )
            }
            None => format!(
                "# ❌ Tool Not Found\n\nNo tool named '{}' exists.",
                tool_name
            ),
        }
    }
}

// ==================== Static Resource Content Helpers ====================
// These functions provide resource content without needing &self reference
// Used by resource handlers passed to the runtime

fn get_guide_content_static() -> String {
    r##"# 🚀 Skillz Guide

## Overview
Skillz is a self-extending MCP server that allows you to build and execute custom tools at runtime.

**Two types of tools:**
- 🦀 **WASM Tools** - Compiled from Rust, run in a WebAssembly sandbox
- 📜 **Script Tools** - Any language (Python, Node.js, Ruby, etc.) via JSON-RPC 2.0

---

## Built-in Tools

### `build_tool` - Create WASM tools from Rust
```
build_tool(name: "my_tool", code: "fn main() {...}", description: "...")
```

### `register_script` - Register any-language tools
```
register_script(name: "my_tool", code: "...", interpreter: "python3", description: "...")
```

### `call_tool` - Execute any registered tool
```
call_tool(tool_name: "my_tool", arguments: {...})
```

### `list_tools` - List all registered tools
### `delete_tool` - Remove a registered tool

---

## Script Tool Protocol

Script tools communicate via JSON-RPC 2.0 over stdin/stdout and can:
- 📝 **Log** - Emit log messages during execution
- 📊 **Progress** - Report progress updates  
- 🧠 **Memory** - Store/retrieve persistent data
- 🎤 **Elicitation** - Request user input
- 🤖 **Sampling** - Request LLM completions
- 📦 **Resources** - List and read server resources

See `skillz://protocol` for the complete protocol specification.
"##.to_string()
}

fn get_examples_content_static() -> String {
    r##"# 📝 Skillz Code Examples

## 🦀 WASM Tools (Rust)

### Hello World
```rust
fn main() {
    println!("Hello from WASM!");
}
```

---

## 📜 Script Tools (Any Language)

### Python Example
```python
#!/usr/bin/env python3
import json
import sys

def main():
    # IMPORTANT: Use readline() not read()!
    request = json.loads(sys.stdin.readline())
    args = request.get("params", {}).get("arguments", {})
    
    result = {"message": "Hello from Python!", "args": args}
    
    response = {"jsonrpc": "2.0", "result": result, "id": request.get("id")}
    print(json.dumps(response))
    sys.stdout.flush()

if __name__ == "__main__":
    main()
```

### Node.js Example
```javascript
const readline = require('readline');
const rl = readline.createInterface({input: process.stdin, terminal: false});

rl.on('line', (line) => {
    const request = JSON.parse(line);
    const result = { message: "Hello from Node.js!" };
    console.log(JSON.stringify({jsonrpc: "2.0", result, id: request.id}));
    process.exit(0);
});
```
"##.to_string()
}

fn get_protocol_content_static() -> String {
    r##"# 📡 JSON-RPC 2.0 Protocol for Script Tools

## Request Format
```json
{
    "jsonrpc": "2.0",
    "method": "execute",
    "params": {
        "arguments": {},
        "context": {
            "roots": ["/path/to/workspace"],
            "capabilities": {
                "sampling": true,
                "elicitation": true,
                "memory": true,
                "resources": true
            }
        }
    },
    "id": 1
}
```

## Available Methods (Script → Skillz)

### Logging
```json
{"jsonrpc": "2.0", "method": "log", "params": {"level": "info", "message": "..."}}
```

### Progress
```json
{"jsonrpc": "2.0", "method": "progress", "params": {"current": 50, "total": 100, "message": "..."}}
```

### Memory
```json
{"jsonrpc": "2.0", "method": "memory/get", "params": {"key": "counter"}, "id": 10}
{"jsonrpc": "2.0", "method": "memory/set", "params": {"key": "counter", "value": 42}, "id": 11}
{"jsonrpc": "2.0", "method": "memory/list", "params": {}, "id": 12}
{"jsonrpc": "2.0", "method": "memory/delete", "params": {"key": "counter"}, "id": 13}
```

### Resources (NEW!)
```json
{"jsonrpc": "2.0", "method": "resources/list", "params": {}, "id": 20}
{"jsonrpc": "2.0", "method": "resources/read", "params": {"uri": "skillz://guide"}, "id": 21}
```

**Response for resources/list:**
```json
{"jsonrpc": "2.0", "result": {"resources": [
    {"uri": "skillz://guide", "name": "Skillz Guide", "description": "..."},
    {"uri": "skillz://tools/my_tool", "name": "my_tool", "description": "..."}
]}, "id": 20}
```

**Response for resources/read:**
```json
{"jsonrpc": "2.0", "result": {"contents": [
    {"uri": "skillz://guide", "mime_type": "text/markdown", "text": "# Guide..."}
]}, "id": 21}
```

### Elicitation (User Input)
```json
{"jsonrpc": "2.0", "method": "elicitation/create", "params": {"message": "...", "requestedSchema": {}}, "id": 100}
```

### Sampling (LLM Completion)
```json
{"jsonrpc": "2.0", "method": "sampling/createMessage", "params": {"messages": [...], "maxTokens": 1000}, "id": 101}
```

## Final Response
```json
{"jsonrpc": "2.0", "result": {"your": "data"}, "id": 1}
```

## ⚠️ Critical: Use readline() NOT read()!
```python
# ✅ CORRECT
request = json.loads(sys.stdin.readline())

# ❌ WRONG - Blocks forever!
request = json.loads(sys.stdin.read())
```
"##.to_string()
}

fn get_tool_info_static(registry: &registry::ToolRegistry, tool_name: &str) -> String {
    match registry.get_tool(tool_name) {
        Some(tool) => {
            let (type_name, type_emoji) = match tool.tool_type() {
                ToolType::Wasm => ("WASM", "🦀"),
                ToolType::Script => ("Script", "📜"),
                ToolType::Pipeline => ("Pipeline", "⛓️"),
            };
            format!(
                "# {} {} Tool: {}\n\n## Description\n{}\n\n## Details\n- **Type:** {}\n- **Version:** {}\n- **Status:** ✅ Ready to use\n\n## Usage\n```\ncall_tool(tool_name: \"{}\")\n```\n",
                type_emoji, type_name, tool.name(),
                tool.description(),
                type_name, tool.manifest.version,
                tool.name()
            )
        }
        None => format!(
            "# ❌ Tool Not Found\n\nNo tool named '{}' exists.",
            tool_name
        ),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Get tools directory from env var or use ~/tools as default
    let tools_dir = std::env::var("TOOLS_DIR").unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            format!("{}/tools", home)
        });
    
    let storage_dir = std::path::PathBuf::from(tools_dir);
    std::fs::create_dir_all(&storage_dir)?;

    eprintln!("Tools directory: {}", storage_dir.display());

    let registry = registry::ToolRegistry::new(storage_dir.clone());
    let memory = memory::Memory::new(&storage_dir).await?;

    // Create runtime with memory support
    let runtime = runtime::ToolRuntime::new()?.with_memory(memory.clone());

    eprintln!("Memory database initialized (with runtime integration)");

    let state = AppState::new(registry, runtime, memory);

    // Start hot reload if enabled
    let _hot_reload = if cli.hot_reload {
        match watcher::HotReload::start(storage_dir.clone()).await {
            Ok(mut hr) => {
                let registry_clone = state.registry.clone();
                // Spawn task to handle reload events
                tokio::spawn(async move {
                    while let Some(event) = hr.next_event().await {
                        match event {
                            watcher::WatchEvent::ToolModified(name) => {
                                eprintln!("🔄 Hot reload: {} modified, reloading...", name);
                                if let Err(e) = registry_clone.reload_tool(&name) {
                                    eprintln!("   ❌ Failed to reload {}: {}", name, e);
                                } else {
                                    eprintln!("   ✅ {} reloaded successfully", name);
                                }
                            }
                            watcher::WatchEvent::ToolAdded(name) => {
                                eprintln!("🆕 Hot reload: {} added, loading...", name);
                                if let Err(e) = registry_clone.reload_tool(&name) {
                                    eprintln!("   ❌ Failed to load {}: {}", name, e);
                                } else {
                                    eprintln!("   ✅ {} loaded successfully", name);
                                }
                            }
                            watcher::WatchEvent::ToolRemoved(name) => {
                                eprintln!("🗑️ Hot reload: {} removed", name);
                                registry_clone.unload_tool(&name);
                            }
                            watcher::WatchEvent::Error(e) => {
                                eprintln!("⚠️ Hot reload error: {}", e);
                            }
                        }
                    }
                });
                Some(())
            }
            Err(e) => {
                eprintln!("⚠️ Failed to start hot reload: {}", e);
                None
            }
        }
    } else {
        None
    };

    match cli.transport.as_str() {
        "stdio" => {
            eprintln!("Skillz MCP started (stdio transport)");
            if cli.hot_reload {
                eprintln!("🔥 Hot reload enabled");
            }
    state.serve(stdio()).await?.waiting().await?;
        }
        "http" | "sse" => {
            use rmcp::transport::sse_server::{SseServer, SseServerConfig};
            use tokio_util::sync::CancellationToken;

            let addr: SocketAddr = format!("{}:{}", cli.host, cli.port).parse()?;
            let ct = CancellationToken::new();

            let config = SseServerConfig {
                bind: addr,
                sse_path: "/sse".to_string(),
                post_path: "/message".to_string(),
                ct: ct.clone(),
                sse_keep_alive: None,
            };

            eprintln!("Skillz MCP started (HTTP/SSE transport)");
            eprintln!("  SSE endpoint: http://{}/sse", addr);
            eprintln!("  POST endpoint: http://{}/message", addr);
            if cli.hot_reload {
                eprintln!("  🔥 Hot reload: enabled");
            }
            eprintln!();
            eprintln!("Connect with:");
            eprintln!(
                "  curl -N http://{}/sse -H 'Accept: text/event-stream'",
                addr
            );

            // Start the SSE server (it handles the HTTP server internally)
            let mut sse_server = SseServer::serve_with_config(config).await?;

            // Accept and serve MCP connections
            loop {
                tokio::select! {
                    transport = sse_server.next_transport() => {
                        if let Some(transport) = transport {
                            let state_clone = state.clone();
                            tokio::spawn(async move {
                                eprintln!("New SSE client connected");
                                if let Err(e) = state_clone.serve(transport).await {
                                    eprintln!("Client error: {}", e);
                                }
                            });
                        } else {
                            break;
                        }
                    }
                    _ = tokio::signal::ctrl_c() => {
                        eprintln!("\nShutting down...");
                        ct.cancel();
                        break;
                    }
                }
            }
        }
        other => {
            eprintln!("Unknown transport: {}. Use 'stdio' or 'http'", other);
            std::process::exit(1);
        }
    }

    Ok(())
}
