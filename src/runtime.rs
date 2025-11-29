use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use wasmtime::{Engine, Linker, Module, Store};
use wasmtime_wasi::preview1::{self, WasiP1Ctx};
use wasmtime_wasi::{pipe::MemoryOutputPipe, WasiCtxBuilder};

use crate::registry::{ToolConfig, ToolType};

// ==================== Sandbox Configuration ====================

/// Sandbox mode for script execution
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub enum SandboxMode {
    /// No sandboxing - scripts run with normal permissions
    #[default]
    None,
    /// Use bubblewrap (bwrap) for Linux sandboxing
    Bubblewrap,
    /// Use firejail for Linux sandboxing
    Firejail,
    /// Use nsjail for Linux sandboxing (most restrictive)
    Nsjail,
}

/// Configuration for script sandboxing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Sandbox mode to use
    pub mode: SandboxMode,
    /// Allow network access in sandbox
    pub allow_network: bool,
    /// Directories to allow read access (in addition to workspace roots)
    pub read_paths: Vec<PathBuf>,
    /// Directories to allow write access
    pub write_paths: Vec<PathBuf>,
    /// Memory limit in MB (0 = unlimited)
    pub memory_limit_mb: u64,
    /// CPU time limit in seconds (0 = unlimited)
    pub time_limit_secs: u64,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            mode: SandboxMode::None,
            allow_network: false,
            read_paths: vec![],
            write_paths: vec![],
            memory_limit_mb: 512,
            time_limit_secs: 30,
        }
    }
}

impl SandboxConfig {
    /// Check if the required sandbox tool is available
    pub fn check_available(&self) -> Result<bool> {
        match self.mode {
            SandboxMode::None => Ok(true),
            SandboxMode::Bubblewrap => Ok(Command::new("bwrap").arg("--version").output().is_ok()),
            SandboxMode::Firejail => Ok(Command::new("firejail").arg("--version").output().is_ok()),
            SandboxMode::Nsjail => Ok(Command::new("nsjail").arg("--version").output().is_ok()),
        }
    }

    /// Wrap a command with sandbox
    pub fn wrap_command(&self, cmd: &mut Command, script_path: &Path, roots: &[String]) {
        if self.mode == SandboxMode::None {
            return;
        }

        let program = cmd.get_program().to_string_lossy().to_string();
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();

        // Build sandbox command
        let sandbox_args = match self.mode {
            SandboxMode::None => return,
            SandboxMode::Bubblewrap => self.build_bwrap_args(script_path, roots),
            SandboxMode::Firejail => self.build_firejail_args(script_path, roots),
            SandboxMode::Nsjail => self.build_nsjail_args(script_path, roots),
        };

        // Replace the command with sandboxed version
        let sandbox_cmd = match self.mode {
            SandboxMode::Bubblewrap => "bwrap",
            SandboxMode::Firejail => "firejail",
            SandboxMode::Nsjail => "nsjail",
            SandboxMode::None => unreachable!(),
        };

        // Clear and rebuild command
        *cmd = Command::new(sandbox_cmd);
        for arg in sandbox_args {
            cmd.arg(arg);
        }
        cmd.arg("--").arg(&program);
        for arg in args {
            cmd.arg(arg);
        }
    }

    fn build_bwrap_args(&self, script_path: &Path, roots: &[String]) -> Vec<String> {
        let mut args = vec![
            "--unshare-all".to_string(),
            "--die-with-parent".to_string(),
            "--ro-bind".to_string(),
            "/usr".to_string(),
            "--ro-bind".to_string(),
            "/lib".to_string(),
            "--ro-bind".to_string(),
            "/lib64".to_string(),
            "--ro-bind".to_string(),
            "/bin".to_string(),
            "--ro-bind".to_string(),
            "/sbin".to_string(),
            "--proc".to_string(),
            "/proc".to_string(),
            "--dev".to_string(),
            "/dev".to_string(),
            "--tmpfs".to_string(),
            "/tmp".to_string(),
        ];

        // Add read-only access to script
        if let Some(parent) = script_path.parent() {
            args.push("--ro-bind".to_string());
            args.push(parent.to_string_lossy().to_string());
            args.push(parent.to_string_lossy().to_string());
        }

        // Add workspace roots
        for root in roots {
            args.push("--bind".to_string());
            args.push(root.clone());
            args.push(root.clone());
        }

        // Add custom read paths
        for path in &self.read_paths {
            args.push("--ro-bind".to_string());
            args.push(path.to_string_lossy().to_string());
            args.push(path.to_string_lossy().to_string());
        }

        // Add custom write paths
        for path in &self.write_paths {
            args.push("--bind".to_string());
            args.push(path.to_string_lossy().to_string());
            args.push(path.to_string_lossy().to_string());
        }

        // Network isolation
        if !self.allow_network {
            args.push("--unshare-net".to_string());
        }

        args
    }

    fn build_firejail_args(&self, _script_path: &Path, roots: &[String]) -> Vec<String> {
        let mut args = vec![
            "--quiet".to_string(),
            "--private-tmp".to_string(),
            "--nogroups".to_string(),
            "--nonewprivs".to_string(),
            "--noroot".to_string(),
            "--seccomp".to_string(),
        ];

        // Network isolation
        if !self.allow_network {
            args.push("--net=none".to_string());
        }

        // Add workspace roots as whitelist
        for root in roots {
            args.push(format!("--whitelist={}", root));
        }

        // Memory limit
        if self.memory_limit_mb > 0 {
            args.push(format!(
                "--rlimit-as={}",
                self.memory_limit_mb * 1024 * 1024
            ));
        }

        // Time limit
        if self.time_limit_secs > 0 {
            args.push(format!("--timeout=00:00:{:02}", self.time_limit_secs));
        }

        args
    }

    fn build_nsjail_args(&self, script_path: &Path, roots: &[String]) -> Vec<String> {
        let mut args = vec![
            "--mode".to_string(),
            "o".to_string(), // Once mode
            "--quiet".to_string(),
            "--disable_clone_newcgroup".to_string(),
            "-R".to_string(),
            "/usr".to_string(),
            "-R".to_string(),
            "/lib".to_string(),
            "-R".to_string(),
            "/lib64".to_string(),
            "-R".to_string(),
            "/bin".to_string(),
        ];

        // Add script directory
        if let Some(parent) = script_path.parent() {
            args.push("-R".to_string());
            args.push(parent.to_string_lossy().to_string());
        }

        // Add workspace roots with write access
        for root in roots {
            args.push("-B".to_string());
            args.push(root.clone());
        }

        // Network isolation
        if !self.allow_network {
            args.push("--disable_clone_newnet".to_string());
        }

        // Resource limits
        if self.memory_limit_mb > 0 {
            args.push("--cgroup_mem_max".to_string());
            args.push((self.memory_limit_mb * 1024 * 1024).to_string());
        }

        if self.time_limit_secs > 0 {
            args.push("--time_limit".to_string());
            args.push(self.time_limit_secs.to_string());
        }

        args
    }
}

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
    /// Priority: MCP roots > SKILLZ_ROOTS env > cwd
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

        // Get roots from environment variable if set, otherwise use cwd
        // Format: SKILLZ_ROOTS=/path/one:/path/two (colon-separated)
        let roots = match std::env::var("SKILLZ_ROOTS") {
            Ok(roots_str) if !roots_str.is_empty() => roots_str
                .split(':')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            _ => vec![cwd.clone()],
        };

        // Safe environment variables to pass
        let mut env = std::collections::HashMap::new();
        for key in ["HOME", "USER", "LANG", "PATH", "TERM"] {
            if let Ok(val) = std::env::var(key) {
                env.insert(key.to_string(), val);
            }
        }

        Self {
            roots,
            working_directory: cwd,
            tool_name: String::new(),
            environment: env,
            tools_dir,
            capabilities: ClientCapabilities::default(),
        }
    }
}

impl ExecutionContext {
    /// Update roots from MCP client (takes priority over env/defaults)
    pub fn with_roots(mut self, roots: Vec<String>) -> Self {
        if !roots.is_empty() {
            self.roots = roots;
        }
        self
    }

    /// Update capabilities from MCP client
    pub fn with_capabilities(mut self, capabilities: ClientCapabilities) -> Self {
        self.capabilities = capabilities;
        self
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
    sandbox_config: SandboxConfig,
}

impl ToolRuntime {
    pub fn new() -> Result<Self> {
        let engine = Engine::default();

        // Check for sandbox mode from environment
        let sandbox_mode = match std::env::var("SKILLZ_SANDBOX").as_deref() {
            Ok("bubblewrap") | Ok("bwrap") => SandboxMode::Bubblewrap,
            Ok("firejail") => SandboxMode::Firejail,
            Ok("nsjail") => SandboxMode::Nsjail,
            _ => SandboxMode::None,
        };

        let sandbox_config = SandboxConfig {
            mode: sandbox_mode,
            allow_network: std::env::var("SKILLZ_SANDBOX_NETWORK").is_ok(),
            ..Default::default()
        };

        Ok(Self {
            engine,
            context: ExecutionContext::default(),
            sandbox_config,
        })
    }

    /// Create runtime with custom sandbox configuration
    pub fn with_sandbox(sandbox_config: SandboxConfig) -> Result<Self> {
        let engine = Engine::default();
        Ok(Self {
            engine,
            context: ExecutionContext::default(),
            sandbox_config,
        })
    }

    /// Get current sandbox configuration
    pub fn sandbox_config(&self) -> &SandboxConfig {
        &self.sandbox_config
    }

    /// Check if sandbox is available
    pub fn sandbox_available(&self) -> bool {
        self.sandbox_config.check_available().unwrap_or(false)
    }

    /// Execute a tool based on its type
    /// Note: Pipeline tools must be executed via call_pipeline, not call_tool
    pub fn call_tool(&self, config: &ToolConfig, args: Value) -> Result<Value> {
        match config.tool_type() {
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
            ToolType::Pipeline => {
                anyhow::bail!("Pipeline tools must be executed via call_pipeline")
            }
        }
    }

    /// Execute a tool by name (for pipelines)
    /// Returns the result as a JSON Value to preserve structure for pipeline variable resolution
    pub async fn call_tool_by_name(
        &self,
        tool_name: &str,
        args: Option<Value>,
        registry: &crate::registry::ToolRegistry,
    ) -> Result<Value> {
        let config = registry
            .get_tool(tool_name)
            .ok_or_else(|| anyhow::anyhow!("Tool '{}' not found", tool_name))?;

        let args_value = args.unwrap_or(Value::Object(serde_json::Map::new()));
        
        // Use spawn_blocking because call_tool is sync and may block
        let runtime = self.clone();
        let config_clone = config.clone();
        let result = tokio::task::spawn_blocking(move || {
            runtime.call_tool(&config_clone, args_value)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Task join error: {}", e))??;

        Ok(result)
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
        context.tool_name = config.name().to_string();

        // Save roots for sandbox (before context is moved)
        let sandbox_roots = context.roots.clone();

        // Ensure arguments is an object, not a string
        // (MCP sometimes passes args as stringified JSON)
        let arguments = match &args {
            Value::String(s) => {
                serde_json::from_str(s).unwrap_or(args.clone())
            }
            _ => args,
        };

        // Build the JSON-RPC request with context
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            method: "execute".to_string(),
            params: ExecuteParams {
                arguments,
                context,
            },
            id: 1,
        };

        let request_json = serde_json::to_string(&request)?;
        eprintln!("Script request: {}", request_json);

        // Determine how to run the script (with virtual environment if configured)
        let interpreter = config.interpreter();
        let mut cmd = if let Some(interp) = interpreter {
            // Check if we have a virtual environment
            if let Some(ref env_path) = config.env_path {
                let interpreter_in_env = match interp {
                    "python3" | "python" => {
                        let venv_python = env_path.join("bin").join("python");
                        if venv_python.exists() {
                            venv_python.to_string_lossy().to_string()
                        } else {
                            interp.to_string()
                        }
                    }
                    "node" | "nodejs" => interp.to_string(), // Node uses NODE_PATH
                    _ => interp.to_string(),
                };
                let mut c = Command::new(&interpreter_in_env);
                c.arg(&config.script_path);

                // For Node.js, set NODE_PATH to include local node_modules
                if interp == "node" || interp == "nodejs" {
                    let node_modules = env_path.join("node_modules");
                    if node_modules.exists() {
                        c.env("NODE_PATH", node_modules);
                    }
                }
                c
            } else {
                let mut c = Command::new(interp);
                c.arg(&config.script_path);
                c
            }
        } else {
            Command::new(&config.script_path)
        };

        // Apply sandbox wrapper if configured
        self.sandbox_config
            .wrap_command(&mut cmd, &config.script_path, &sandbox_roots);

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

// ==================== Dependency Management ====================

/// Result of dependency installation
#[derive(Debug)]
pub struct InstallResult {
    pub success: bool,
    pub message: String,
    pub env_path: Option<std::path::PathBuf>,
}

/// Install Python dependencies using pip in a virtual environment
pub fn install_python_deps(env_path: &Path, dependencies: &[String]) -> Result<InstallResult> {
    // Create virtual environment if it doesn't exist
    if !env_path.exists() {
        eprintln!("Creating Python venv at: {:?}", env_path);
        let output = Command::new("python3")
            .args(["-m", "venv"])
            .arg(env_path)
            .output()
            .context("Failed to create venv")?;

        if !output.status.success() {
            return Ok(InstallResult {
                success: false,
                message: format!(
                    "Failed to create venv: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
                env_path: None,
            });
        }
    }

    // Install dependencies using pip
    let pip_path = env_path.join("bin").join("pip");
    if !pip_path.exists() {
        return Ok(InstallResult {
            success: false,
            message: "pip not found in venv".to_string(),
            env_path: None,
        });
    }

    if dependencies.is_empty() {
        return Ok(InstallResult {
            success: true,
            message: "No dependencies to install".to_string(),
            env_path: Some(env_path.to_path_buf()),
        });
    }

    eprintln!("Installing Python deps: {:?}", dependencies);
    let output = Command::new(&pip_path)
        .arg("install")
        .args(dependencies)
        .output()
        .context("Failed to run pip install")?;

    if output.status.success() {
        Ok(InstallResult {
            success: true,
            message: format!(
                "Installed {} dependencies:\n{}",
                dependencies.len(),
                String::from_utf8_lossy(&output.stdout)
            ),
            env_path: Some(env_path.to_path_buf()),
        })
    } else {
        Ok(InstallResult {
            success: false,
            message: format!(
                "pip install failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            ),
            env_path: None,
        })
    }
}

/// Install Node.js dependencies using npm
pub fn install_node_deps(env_path: &Path, dependencies: &[String]) -> Result<InstallResult> {
    // Create directory if it doesn't exist
    std::fs::create_dir_all(env_path)?;

    if dependencies.is_empty() {
        return Ok(InstallResult {
            success: true,
            message: "No dependencies to install".to_string(),
            env_path: Some(env_path.to_path_buf()),
        });
    }

    // Create package.json
    let package_json = serde_json::json!({
        "name": "skillz-tool",
        "version": "1.0.0",
        "private": true,
        "dependencies": dependencies.iter()
            .map(|d| {
                let parts: Vec<&str> = d.splitn(2, '@').collect();
                if parts.len() == 2 {
                    (parts[0].to_string(), parts[1].to_string())
                } else {
                    (d.clone(), "latest".to_string())
                }
            })
            .collect::<std::collections::HashMap<String, String>>()
    });

    let package_path = env_path.join("package.json");
    std::fs::write(&package_path, serde_json::to_string_pretty(&package_json)?)?;

    eprintln!("Installing Node.js deps: {:?}", dependencies);
    let output = Command::new("npm")
        .current_dir(env_path)
        .args(["install", "--production"])
        .output()
        .context("Failed to run npm install")?;

    if output.status.success() {
        Ok(InstallResult {
            success: true,
            message: format!(
                "Installed {} dependencies:\n{}",
                dependencies.len(),
                String::from_utf8_lossy(&output.stdout)
            ),
            env_path: Some(env_path.to_path_buf()),
        })
    } else {
        Ok(InstallResult {
            success: false,
            message: format!(
                "npm install failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            ),
            env_path: None,
        })
    }
}

/// Install dependencies for a tool based on its interpreter
pub fn install_tool_deps(
    env_path: &Path,
    interpreter: Option<&str>,
    dependencies: &[String],
) -> Result<InstallResult> {
    match interpreter {
        Some("python3") | Some("python") => install_python_deps(env_path, dependencies),
        Some("node") | Some("nodejs") => install_node_deps(env_path, dependencies),
        Some(other) => Ok(InstallResult {
            success: false,
            message: format!(
                "Dependency installation not supported for interpreter: {}",
                other
            ),
            env_path: None,
        }),
        None => Ok(InstallResult {
            success: false,
            message: "No interpreter specified for dependency installation".to_string(),
            env_path: None,
        }),
    }
}
