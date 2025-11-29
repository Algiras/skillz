use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

/// Tool execution type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ToolType {
    /// WebAssembly module compiled from Rust
    #[default]
    Wasm,
    /// External script/executable with JSON-RPC 2.0 interface
    Script,
}

/// Tool annotations - hints about tool behavior for clients
/// Based on MCP specification: https://modelcontextprotocol.io/specification/2025-06-18/schema
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolAnnotations {
    /// Human-readable title for the tool (display name)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// If true, the tool performs read-only operations (doesn't modify state)
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "readOnlyHint"
    )]
    pub read_only_hint: Option<bool>,
    /// If true, the tool may perform destructive updates (delete, overwrite)
    /// Only meaningful when read_only_hint is false
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "destructiveHint"
    )]
    pub destructive_hint: Option<bool>,
    /// If true, calling the tool repeatedly with same args has no additional effect
    /// Only meaningful when read_only_hint is false
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "idempotentHint"
    )]
    pub idempotent_hint: Option<bool>,
    /// If true, this tool may interact with external systems (network, APIs, etc.)
    /// If false, the tool's domain of interaction is closed/local
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "openWorldHint"
    )]
    pub open_world_hint: Option<bool>,
}

impl ToolAnnotations {
    /// Create annotations for a read-only tool
    pub fn read_only() -> Self {
        Self {
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            ..Default::default()
        }
    }

    /// Create annotations for a tool that modifies state
    pub fn read_write(destructive: bool, idempotent: bool) -> Self {
        Self {
            read_only_hint: Some(false),
            destructive_hint: Some(destructive),
            idempotent_hint: Some(idempotent),
            ..Default::default()
        }
    }

    /// Create annotations for a tool that accesses external systems
    pub fn open_world() -> Self {
        Self {
            open_world_hint: Some(true),
            ..Default::default()
        }
    }

    /// Create from JSON value
    pub fn from_value(value: serde_json::Value) -> Self {
        serde_json::from_value(value).unwrap_or_default()
    }
}

/// JSON Schema definition for tool inputs/outputs
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolSchema {
    /// JSON Schema type (e.g., "object", "string", "number")
    #[serde(rename = "type", default)]
    pub schema_type: String,
    /// Schema properties (for object type)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<serde_json::Value>,
    /// Required properties
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required: Vec<String>,
    /// Schema description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Additional JSON Schema fields
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

impl ToolSchema {
    /// Create an empty/any schema
    pub fn any() -> Self {
        Self {
            schema_type: "object".to_string(),
            properties: None,
            required: vec![],
            description: None,
            extra: serde_json::Value::Object(Default::default()),
        }
    }

    /// Create schema from JSON value
    pub fn from_value(value: serde_json::Value) -> Self {
        serde_json::from_value(value).unwrap_or_else(|_| Self::any())
    }

    /// Convert to JSON value
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::json!({"type": "object"}))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfig {
    pub name: String,
    pub description: String,
    /// Tool type determines how it's executed
    #[serde(default)]
    pub tool_type: ToolType,
    /// Path to WASM file (for Wasm tools)
    #[serde(default)]
    pub wasm_path: PathBuf,
    /// Path to script/executable (for Script tools)
    #[serde(default)]
    pub script_path: PathBuf,
    /// Command to run the script (e.g., "python3", "node", "ruby")
    /// If empty, the script is executed directly (must be executable)
    #[serde(default)]
    pub interpreter: Option<String>,
    /// Input schema - JSON Schema describing expected arguments
    #[serde(default)]
    pub input_schema: ToolSchema,
    /// Output schema - JSON Schema describing structured output (optional, for structured responses)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<ToolSchema>,
    /// Tool annotations - hints about behavior (readOnly, destructive, etc.)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ToolAnnotations>,
    /// Dependencies for script tools (pip packages, npm modules, etc.)
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Path to virtual environment (for Python) or node_modules (for Node.js)
    #[serde(default)]
    pub env_path: Option<PathBuf>,
    /// Whether dependencies have been installed
    #[serde(default)]
    pub deps_installed: bool,
}

#[derive(Clone)]
pub struct ToolRegistry {
    tools: Arc<RwLock<HashMap<String, ToolConfig>>>,
    storage_dir: PathBuf,
}

impl ToolRegistry {
    pub fn new(storage_dir: PathBuf) -> Self {
        let manifest_path = storage_dir.join("manifest.json");

        // Load existing tools from manifest if it exists
        let tools = if manifest_path.exists() {
            match fs::read_to_string(&manifest_path) {
                Ok(content) => {
                    match serde_json::from_str::<HashMap<String, ToolConfig>>(&content) {
                        Ok(loaded) => {
                            let wasm_count = loaded
                                .values()
                                .filter(|t| t.tool_type == ToolType::Wasm)
                                .count();
                            let script_count = loaded
                                .values()
                                .filter(|t| t.tool_type == ToolType::Script)
                                .count();
                            eprintln!(
                                "Loaded {} tools ({} WASM, {} Script)",
                                loaded.len(),
                                wasm_count,
                                script_count
                            );
                            Arc::new(RwLock::new(loaded))
                        }
                        Err(e) => {
                            eprintln!("Failed to parse manifest: {}", e);
                            Arc::new(RwLock::new(HashMap::new()))
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to read manifest: {}", e);
                    Arc::new(RwLock::new(HashMap::new()))
                }
            }
        } else {
            Arc::new(RwLock::new(HashMap::new()))
        };

        // Create scripts directory
        let scripts_dir = storage_dir.join("scripts");
        let _ = fs::create_dir_all(&scripts_dir);

        Self { tools, storage_dir }
    }

    fn save_manifest(&self) -> Result<()> {
        let manifest_path = self.storage_dir.join("manifest.json");
        let tools = self.tools.read().unwrap();
        let json = serde_json::to_string_pretty(&*tools)?;
        fs::write(manifest_path, json)?;
        Ok(())
    }

    pub fn register_tool(&self, config: ToolConfig) -> Result<()> {
        let mut tools = self.tools.write().unwrap();
        tools.insert(config.name.clone(), config);
        drop(tools); // Release lock before saving
        self.save_manifest()?;
        Ok(())
    }

    pub fn get_tool(&self, name: &str) -> Option<ToolConfig> {
        self.tools.read().unwrap().get(name).cloned()
    }

    pub fn list_tools(&self) -> Vec<ToolConfig> {
        self.tools.read().unwrap().values().cloned().collect()
    }

    pub fn storage_dir(&self) -> &PathBuf {
        &self.storage_dir
    }

    pub fn scripts_dir(&self) -> PathBuf {
        self.storage_dir.join("scripts")
    }

    /// Get directory for tool environments (venvs, node_modules)
    pub fn envs_dir(&self) -> PathBuf {
        self.storage_dir.join("envs")
    }

    /// Get environment path for a specific tool
    pub fn tool_env_path(&self, tool_name: &str, interpreter: Option<&str>) -> PathBuf {
        let envs_dir = self.envs_dir();
        match interpreter {
            Some("python3") | Some("python") => envs_dir.join(format!("{}_venv", tool_name)),
            Some("node") | Some("nodejs") => envs_dir.join(format!("{}_node", tool_name)),
            _ => envs_dir.join(tool_name),
        }
    }

    /// Update tool's dependency status
    pub fn mark_deps_installed(&self, tool_name: &str, env_path: PathBuf) -> Result<()> {
        let mut tools = self.tools.write().unwrap();
        if let Some(tool) = tools.get_mut(tool_name) {
            tool.deps_installed = true;
            tool.env_path = Some(env_path);
        }
        drop(tools);
        self.save_manifest()?;
        Ok(())
    }

    /// Delete a tool and its environment
    pub fn delete_tool(&self, name: &str) -> Result<bool> {
        let mut tools = self.tools.write().unwrap();
        if let Some(tool) = tools.remove(name) {
            // Clean up files
            if tool.tool_type == ToolType::Wasm && tool.wasm_path.exists() {
                let _ = fs::remove_file(&tool.wasm_path);
            }
            if tool.tool_type == ToolType::Script && tool.script_path.exists() {
                let _ = fs::remove_file(&tool.script_path);
            }
            if let Some(env_path) = tool.env_path {
                if env_path.exists() {
                    let _ = fs::remove_dir_all(&env_path);
                }
            }
            drop(tools);
            self.save_manifest()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
