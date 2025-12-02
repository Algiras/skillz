use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock, oneshot};
use tokio::time::Duration;

use crate::config::ServerConfig;
use crate::registry::{ToolManifest, ToolType};

const MAX_RESTARTS: u32 = 3;
const RESTART_RESET_WINDOW: Duration = Duration::from_secs(300); // 5 minutes

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
    id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsonRpcNotification {
    jsonrpc: String,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<JsonRpcError>,
    id: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[serde(default)]
    #[allow(dead_code)]
    data: Option<Value>,
}

struct ClientState {
    child: Option<Child>,
    stdin: Option<tokio::process::ChildStdin>,
    pending_requests: HashMap<u64, oneshot::Sender<Result<Value>>>,
    next_id: u64,
}

pub struct McpClient {
    id: String,
    config: ServerConfig,
    state: Arc<Mutex<ClientState>>,
    restart_count: Arc<Mutex<u32>>,
    last_restart: Arc<Mutex<Option<Instant>>>,
}

impl McpClient {
    pub fn new(id: String, config: ServerConfig) -> Self {
        Self {
            id,
            config,
            state: Arc::new(Mutex::new(ClientState {
                child: None,
                stdin: None,
                pending_requests: HashMap::new(),
                next_id: 1,
            })),
            restart_count: Arc::new(Mutex::new(0)),
            last_restart: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn start(&self) -> Result<()> {
        self.connect().await
    }

    async fn connect(&self) -> Result<()> {
        // Check restart policy
        {
            let mut count = self.restart_count.lock().await;
            let mut last = self.last_restart.lock().await;
            
            if let Some(last_time) = *last {
                if last_time.elapsed() > RESTART_RESET_WINDOW {
                    *count = 0;
                }
            }

            if *count >= MAX_RESTARTS {
                anyhow::bail!("Max restarts ({}) exceeded for server {}", MAX_RESTARTS, self.id);
            }

            *count += 1;
            *last = Some(Instant::now());
        }

        eprintln!("Starting MCP server: {} ({})", self.id, self.config.command);

        let mut cmd = Command::new(&self.config.command);
        cmd.args(&self.config.args);
        cmd.envs(&self.config.env);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn().context(format!("Failed to spawn {}", self.id))?;
        let stdin = child.stdin.take().context("Failed to open stdin")?;
        let stdout = child.stdout.take().context("Failed to open stdout")?;
        let stderr = child.stderr.take().context("Failed to open stderr")?;

        // Spawn stderr logger
        let id_clone = self.id.clone();
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                eprintln!("[{}] stderr: {}", id_clone, line);
            }
        });

        // Spawn stdout reader (response handler)
        let state_clone = self.state.clone();
        let id_clone = self.id.clone();
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.trim().is_empty() { continue; }
                
                if let Ok(response) = serde_json::from_str::<JsonRpcResponse>(&line) {
                    if let Some(id) = response.id {
                        let mut state = state_clone.lock().await;
                        if let Some(sender) = state.pending_requests.remove(&id) {
                            if let Some(error) = response.error {
                                let _ = sender.send(Err(anyhow::anyhow!("RPC Error {}: {}", error.code, error.message)));
                            } else {
                                let _ = sender.send(Ok(response.result.unwrap_or(Value::Null)));
                            }
                        }
                    }
                } else {
                    // Maybe a notification or log?
                    eprintln!("[{}] stdout: {}", id_clone, line);
                }
            }
            eprintln!("[{}] Connection closed", id_clone);
        });

        {
            let mut state = self.state.lock().await;
            state.child = Some(child);
            state.stdin = Some(stdin);
        }

        // Perform handshake
        self.initialize().await?;

        Ok(())
    }

    async fn send_request(&self, method: &str, params: Option<Value>) -> Result<Value> {
        let (tx, rx) = oneshot::channel();
        let request_json;
        
        {
            let mut state = self.state.lock().await;
            
            let id = state.next_id;
            state.next_id += 1;
            state.pending_requests.insert(id, tx);

            let request = JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                method: method.to_string(),
                params,
                id,
            };
            request_json = serde_json::to_string(&request)?;
            
            if let Some(stdin) = &mut state.stdin {
                stdin.write_all(request_json.as_bytes()).await?;
                stdin.write_all(b"\n").await?;
                stdin.flush().await?;
            } else {
                anyhow::bail!("Not connected");
            }
        }

        // Wait for response with timeout (120s to allow for slow npm installs)
        match tokio::time::timeout(Duration::from_secs(120), rx).await {
            Ok(Ok(result)) => result.map_err(|e| anyhow::anyhow!("Tool execution failed: {}", e)),
            Ok(Err(_)) => anyhow::bail!("Channel closed"),
            Err(_) => anyhow::bail!("Request timed out"),
        }
    }

    async fn request(&self, method: &str, params: Option<Value>) -> Result<Value> {
        {
            let state = self.state.lock().await;
            if state.stdin.is_none() {
                drop(state);
                self.connect().await?;
            }
        }
        self.send_request(method, params).await
    }

    async fn notify(&self, method: &str, params: Option<Value>) -> Result<()> {
        let mut state = self.state.lock().await;
        if let Some(stdin) = &mut state.stdin {
            let notification = JsonRpcNotification {
                jsonrpc: "2.0".to_string(),
                method: method.to_string(),
                params,
            };
            let json = serde_json::to_string(&notification)?;
            stdin.write_all(json.as_bytes()).await?;
            stdin.write_all(b"\n").await?;
            stdin.flush().await?;
            Ok(())
        } else {
            anyhow::bail!("Not connected");
        }
    }

    async fn initialize(&self) -> Result<()> {
        let params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "roots": {
                    "listChanged": true
                },
                "sampling": {}
            },
            "clientInfo": {
                "name": "skillz",
                "version": "0.7.0"
            }
        });

        let result = self.send_request("initialize", Some(params)).await?;
        eprintln!("[{}] Initialized: {:?}", self.id, result);

        self.notify("notifications/initialized", None).await?;
        Ok(())
    }

    pub async fn list_tools(&self) -> Result<Vec<ToolManifest>> {
        let result = self.request("tools/list", None).await?;
        
        let tools_val = result.get("tools").ok_or_else(|| anyhow::anyhow!("No tools field in response"))?;
        let tools_array = tools_val.as_array().ok_or_else(|| anyhow::anyhow!("Tools is not an array"))?;

        let mut manifests = Vec::new();
        for tool in tools_array {
            // Convert MCP tool definition to our ToolManifest
            // Note: We need to handle the conversion carefully
            let name = tool.get("name").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
            let description = tool.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let input_schema = tool.get("inputSchema").cloned().unwrap_or(serde_json::json!({}));

            let mut manifest = ToolManifest::new(
                name,
                description,
                ToolType::Mcp, // We'll add this type
            );
            manifest.input_schema = crate::registry::ToolSchema::from_value(input_schema);
            
            manifests.push(manifest);
        }

        Ok(manifests)
    }

    pub async fn call_tool(&self, name: &str, args: Value) -> Result<Value> {
        let params = serde_json::json!({
            "name": name,
            "arguments": args
        });
        
        let result = self.request("tools/call", Some(params)).await?;
        
        // MCP tools/call returns { content: [...], isError: bool }
        // We want to return just the content or process it
        Ok(result)
    }
}

pub struct McpClientManager {
    clients: Arc<RwLock<HashMap<String, Arc<McpClient>>>>,
}

impl McpClientManager {
    pub fn new() -> Self {
        Self {
            clients: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register_server(&self, id: String, config: ServerConfig) -> Result<()> {
        let client = Arc::new(McpClient::new(id.clone(), config));
        // Start in background? Or on demand? 
        // Let's start immediately to discover tools
        client.start().await?;
        
        self.clients.write().await.insert(id, client);
        Ok(())
    }

    pub async fn get_client(&self, id: &str) -> Option<Arc<McpClient>> {
        self.clients.read().await.get(id).cloned()
    }
    
    pub async fn list_clients(&self) -> Vec<String> {
        self.clients.read().await.keys().cloned().collect()
    }
}
