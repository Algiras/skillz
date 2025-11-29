use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use wasmtime::{Engine, Linker, Module, Store};
use wasmtime_wasi::preview1::{self, WasiP1Ctx};
use wasmtime_wasi::{pipe::MemoryOutputPipe, WasiCtxBuilder};

use crate::registry::{ToolConfig, ToolType};

// ==================== JSON-RPC 2.0 Protocol ====================

/// JSON-RPC 2.0 Request sent to scripts
#[derive(Debug, Serialize)]
struct JsonRpcRequest {
    jsonrpc: &'static str,
    method: String,
    params: ExecuteParams,
    id: u64,
}

/// Parameters for the execute method
#[derive(Debug, Serialize)]
struct ExecuteParams {
    /// User-provided arguments
    arguments: Value,
    /// Execution context (roots, environment, etc.)
    context: ExecutionContext,
}

/// Client capabilities passed to scripts
#[derive(Debug, Clone, Serialize, Default)]
pub struct ClientCapabilities {
    /// Whether the client supports sampling (LLM requests)
    pub sampling: bool,
    /// Whether the client supports elicitation (user input requests)
    pub elicitation: bool,
}

/// Context information passed to scripts (like MCP roots)
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionContext {
    /// Workspace roots (directories the tool can operate on)
    pub roots: Vec<String>,
    /// Current working directory
    pub working_directory: String,
    /// Name of the tool being executed
    pub tool_name: String,
    /// Environment variables (filtered for safety)
    pub environment: std::collections::HashMap<String, String>,
    /// Tools directory path
    pub tools_dir: String,
    /// Client capabilities (what features the client supports)
    pub capabilities: ClientCapabilities,
}

impl Default for ExecutionContext {
    fn default() -> Self {
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| ".".to_string());

        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let tools_dir = std::env::var("TOOLS_DIR").unwrap_or_else(|_| format!("{}/tools", home));

        // Safe environment variables to pass
        let mut env = std::collections::HashMap::new();
        for key in ["HOME", "USER", "LANG", "PATH", "TERM"] {
            if let Ok(val) = std::env::var(key) {
                env.insert(key.to_string(), val);
            }
        }

        Self {
            roots: vec![cwd.clone()],
            working_directory: cwd,
            tool_name: String::new(),
            environment: env,
            tools_dir,
            capabilities: ClientCapabilities::default(),
        }
    }
}

/// JSON-RPC 2.0 Response from scripts
#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<JsonRpcError>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    params: Option<Value>,
    #[allow(dead_code)]
    id: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[serde(default)]
    data: Option<Value>,
}

/// Log entry from a script
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub level: String,
    pub message: String,
    #[serde(default)]
    pub data: Option<Value>,
}

/// Progress update from a script
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressUpdate {
    #[serde(default)]
    pub current: u64,
    #[serde(default)]
    pub total: u64,
    #[serde(default)]
    pub message: Option<String>,
}

/// Result of script execution including logs
#[derive(Debug)]
pub struct ScriptResult {
    pub output: Value,
    pub logs: Vec<LogEntry>,
    #[allow(dead_code)]
    pub progress: Vec<ProgressUpdate>,
}

// ==================== Tool Runtime ====================

#[derive(Clone)]
pub struct ToolRuntime {
    engine: Engine,
    context: ExecutionContext,
}

impl ToolRuntime {
    pub fn new() -> Result<Self> {
        let engine = Engine::default();
        Ok(Self {
            engine,
            context: ExecutionContext::default(),
        })
    }

    /// Execute a tool based on its type
    pub fn call_tool(&self, config: &ToolConfig, args: Value) -> Result<Value> {
        match config.tool_type {
            ToolType::Wasm => self.call_wasm_tool(&config.wasm_path, args),
            ToolType::Script => {
                let result = self.call_script_tool(config, args)?;
                // Format output with logs if present
                if result.logs.is_empty() {
                    Ok(result.output)
                } else {
                    let mut output = serde_json::Map::new();
                    output.insert("result".to_string(), result.output);
                    output.insert("logs".to_string(), serde_json::to_value(&result.logs)?);
                    Ok(Value::Object(output))
                }
            }
        }
    }

    /// Execute a WASM tool
    fn call_wasm_tool(&self, wasm_path: &Path, _args: Value) -> Result<Value> {
        let mut linker: Linker<WasiP1Ctx> = Linker::new(&self.engine);
        preview1::add_to_linker_sync(&mut linker, |t| t)?;

        let stdout = MemoryOutputPipe::new(4096);

        let wasi = WasiCtxBuilder::new()
            .inherit_stderr()
            .stdout(stdout.clone())
            .build_p1();

        let mut store = Store::new(&self.engine, wasi);
        let module = Module::from_file(&self.engine, wasm_path)?;

        linker.module(&mut store, "", &module)?;

        let instance = linker.instantiate(&mut store, &module)?;
        let start = instance.get_typed_func::<(), ()>(&mut store, "_start")?;

        start.call(&mut store, ())?;

        let output = stdout.contents();
        let output_str = String::from_utf8_lossy(&output);

        Ok(Value::String(output_str.to_string()))
    }

    /// Execute a Script tool via JSON-RPC 2.0 with logging support
    fn call_script_tool(&self, config: &ToolConfig, args: Value) -> Result<ScriptResult> {
        // Build execution context
        let mut context = self.context.clone();
        context.tool_name = config.name.clone();

        // Build the JSON-RPC request with context
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            method: "execute".to_string(),
            params: ExecuteParams {
                arguments: args,
                context,
            },
            id: 1,
        };

        let request_json = serde_json::to_string(&request)?;
        eprintln!("Script request: {}", request_json);

        // Determine how to run the script
        let mut cmd = if let Some(ref interpreter) = config.interpreter {
            let mut c = Command::new(interpreter);
            c.arg(&config.script_path);
            c
        } else {
            Command::new(&config.script_path)
        };

        // Set up the process
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context(format!("Failed to spawn script: {:?}", config.script_path))?;

        // Write request to stdin and close it
        {
            let stdin = child.stdin.as_mut().context("Failed to get stdin")?;
            stdin.write_all(request_json.as_bytes())?;
            stdin.write_all(b"\n")?;
            stdin.flush()?;
        }
        // stdin is closed here when it goes out of scope

        // Collect logs and result
        let mut logs = Vec::new();
        let mut progress = Vec::new();
        let mut final_result: Option<Value> = None;
        let mut final_error: Option<String> = None;

        // Read stdout line by line
        let stdout = child.stdout.take().context("Failed to get stdout")?;
        let reader = BufReader::new(stdout);

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            eprintln!("Script output line: {}", line);

            // Try to parse as JSON-RPC
            if let Ok(response) = serde_json::from_str::<JsonRpcResponse>(&line) {
                // Check if it's a notification (method present, no id)
                if let Some(method) = &response.method {
                    match method.as_str() {
                        "log" | "logging/message" => {
                            if let Some(params) = response.params {
                                if let Ok(log) = serde_json::from_value::<LogEntry>(params) {
                                    eprintln!("[{}] {}", log.level.to_uppercase(), log.message);
                                    logs.push(log);
                                }
                            }
                        }
                        "progress" | "notifications/progress" => {
                            if let Some(params) = response.params {
                                if let Ok(prog) = serde_json::from_value::<ProgressUpdate>(params) {
                                    if let Some(ref msg) = prog.message {
                                        eprintln!(
                                            "[PROGRESS] {}/{} - {}",
                                            prog.current, prog.total, msg
                                        );
                                    }
                                    progress.push(prog);
                                }
                            }
                        }
                        _ => {
                            eprintln!("Unknown notification method: {}", method);
                        }
                    }
                } else {
                    // It's a response (has id or result/error)
                    if let Some(error) = response.error {
                        final_error = Some(format!("Error {}: {}", error.code, error.message));
                        if let Some(data) = error.data {
                            final_error = Some(format!("{}\nData: {}", final_error.unwrap(), data));
                        }
                    } else if let Some(result) = response.result {
                        final_result = Some(result);
                    }
                }
            } else {
                // Not JSON-RPC, treat as plain text output
                if final_result.is_none() {
                    final_result = Some(Value::String(line));
                }
            }
        }

        // Wait for the process to finish
        let status = child.wait()?;

        // Check stderr for any errors
        if let Some(mut stderr) = child.stderr.take() {
            let mut stderr_content = String::new();
            std::io::Read::read_to_string(&mut stderr, &mut stderr_content)?;
            if !stderr_content.is_empty() {
                eprintln!("Script stderr: {}", stderr_content);
                // Add stderr as a log entry
                logs.push(LogEntry {
                    level: "error".to_string(),
                    message: stderr_content,
                    data: None,
                });
            }
        }

        // Determine final output
        let output = if let Some(err) = final_error {
            Value::String(err)
        } else if let Some(result) = final_result {
            result
        } else if !status.success() {
            Value::String(format!("Script exited with status: {}", status))
        } else {
            Value::String("Script completed with no output".to_string())
        };

        Ok(ScriptResult {
            output,
            logs,
            progress,
        })
    }
}
