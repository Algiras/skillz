use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
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
    /// Pipeline - chains other tools together
    Pipeline,
}

/// Tool annotations - hints about tool behavior for clients
/// Based on MCP specification
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolAnnotations {
    /// Human-readable title for the tool (display name)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// If true, the tool performs read-only operations
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "readOnlyHint"
    )]
    pub read_only_hint: Option<bool>,
    /// If true, the tool may perform destructive updates
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "destructiveHint"
    )]
    pub destructive_hint: Option<bool>,
    /// If true, calling repeatedly with same args has no additional effect
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "idempotentHint"
    )]
    pub idempotent_hint: Option<bool>,
    /// If true, may interact with external systems
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "openWorldHint"
    )]
    pub open_world_hint: Option<bool>,
}

impl ToolAnnotations {
    pub fn from_value(value: serde_json::Value) -> Self {
        serde_json::from_value(value).unwrap_or_default()
    }
}

/// JSON Schema definition for tool inputs/outputs
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolSchema {
    #[serde(rename = "type", default)]
    pub schema_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

impl ToolSchema {
    pub fn any() -> Self {
        Self {
            schema_type: "object".to_string(),
            properties: None,
            required: vec![],
            description: None,
            extra: serde_json::Value::Object(Default::default()),
        }
    }

    pub fn from_value(value: serde_json::Value) -> Self {
        serde_json::from_value(value).unwrap_or_else(|_| Self::any())
    }
}

/// A step in a pipeline tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStep {
    /// Optional name for referencing this step's output
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Tool to execute (can be another pipeline!)
    pub tool: String,
    /// Arguments - use $input.field, $prev, $prev.field, $step_name.field
    #[serde(default)]
    pub args: serde_json::Value,
    /// Continue even if this step fails
    #[serde(default)]
    pub continue_on_error: bool,
    /// Condition to check before running (e.g., "$prev.success == true")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
}

/// Tool manifest - stored as manifest.json in each tool's directory
/// This is the shareable format that people can copy between installations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolManifest {
    /// Tool name (must match directory name)
    pub name: String,
    /// Semantic version (e.g., "1.0.0")
    #[serde(default = "default_version")]
    pub version: String,
    /// What the tool does
    pub description: String,
    /// Tool type: wasm, script, or pipeline
    #[serde(default)]
    pub tool_type: ToolType,
    /// For script tools: the script filename to execute (e.g., "main.py")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_file: Option<String>,
    /// For script tools: interpreter command (python3, node, ruby, bash)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interpreter: Option<String>,
    /// Input schema - JSON Schema for arguments
    #[serde(default)]
    pub input_schema: ToolSchema,
    /// Output schema - JSON Schema for result (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<ToolSchema>,
    /// Behavior hints for clients
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ToolAnnotations>,
    /// Script dependencies (pip/npm packages)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
    /// WASM/Rust dependencies (crates) - format: "name@version" or "name@version[feat1,feat2]"
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wasm_dependencies: Vec<String>,
    /// For pipeline tools: the steps to execute
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pipeline_steps: Vec<PipelineStep>,
    /// Tool author (optional, for sharing)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// License (optional, for sharing)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// Repository URL (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    /// Tags for categorization
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// When the tool was created (ISO 8601)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// When the tool was last updated (ISO 8601)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

impl ToolManifest {
    pub fn new(name: String, description: String, tool_type: ToolType) -> Self {
        let now = chrono_now();
        Self {
            name,
            version: "1.0.0".to_string(),
            description,
            tool_type,
            entry_file: None,
            interpreter: None,
            input_schema: ToolSchema::any(),
            output_schema: None,
            annotations: None,
            dependencies: vec![],
            wasm_dependencies: vec![],
            pipeline_steps: vec![],
            author: None,
            license: None,
            repository: None,
            tags: vec![],
            created_at: Some(now.clone()),
            updated_at: Some(now),
        }
    }

    /// Create a new pipeline manifest
    pub fn new_pipeline(name: String, description: String, steps: Vec<PipelineStep>) -> Self {
        let mut manifest = Self::new(name, description, ToolType::Pipeline);
        manifest.pipeline_steps = steps;
        manifest
    }
}

/// Runtime tool configuration (includes paths resolved at load time)
#[derive(Debug, Clone)]
pub struct ToolConfig {
    /// The manifest data
    pub manifest: ToolManifest,
    /// Directory containing this tool
    pub tool_dir: PathBuf,
    /// Path to WASM file (for Wasm tools)
    pub wasm_path: PathBuf,
    /// Path to script file (for Script tools)
    pub script_path: PathBuf,
    /// Path to virtual environment
    pub env_path: Option<PathBuf>,
    /// Whether dependencies have been installed
    pub deps_installed: bool,
}

impl ToolConfig {
    // Convenience accessors that delegate to manifest
    pub fn name(&self) -> &str {
        &self.manifest.name
    }
    pub fn description(&self) -> &str {
        &self.manifest.description
    }
    pub fn tool_type(&self) -> &ToolType {
        &self.manifest.tool_type
    }
    pub fn interpreter(&self) -> Option<&str> {
        self.manifest.interpreter.as_deref()
    }
    pub fn dependencies(&self) -> &[String] {
        &self.manifest.dependencies
    }
    #[allow(dead_code)]
    pub fn wasm_dependencies(&self) -> &[String] {
        &self.manifest.wasm_dependencies
    }
    pub fn input_schema(&self) -> &ToolSchema {
        &self.manifest.input_schema
    }
    #[allow(dead_code)]
    pub fn output_schema(&self) -> Option<&ToolSchema> {
        self.manifest.output_schema.as_ref()
    }
    #[allow(dead_code)]
    pub fn annotations(&self) -> Option<&ToolAnnotations> {
        self.manifest.annotations.as_ref()
    }
    pub fn pipeline_steps(&self) -> &[PipelineStep] {
        &self.manifest.pipeline_steps
    }
}

/// Get current timestamp in ISO 8601 format
fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // Simple ISO 8601 format without external dependency
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        1970 + secs / 31536000,
        (secs % 31536000) / 2592000 + 1,
        (secs % 2592000) / 86400 + 1,
        (secs % 86400) / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

#[derive(Clone)]
pub struct ToolRegistry {
    tools: Arc<RwLock<HashMap<String, ToolConfig>>>,
    storage_dir: PathBuf,
}

impl ToolRegistry {
    pub fn new(storage_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&storage_dir);

        let registry = Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
            storage_dir,
        };

        // Load tools from directory structure
        registry.load_all_tools();

        // Migrate old format if needed
        registry.migrate_old_format();

        registry
    }

    /// Load/reload all tools from the directory structure
    pub fn reload(&self) {
        self.load_all_tools();
    }

    /// Load all tools from the directory structure
    fn load_all_tools(&self) {
        let mut tools = self.tools.write().unwrap();
        tools.clear();

        // Scan storage directory for tool directories
        if let Ok(entries) = fs::read_dir(&self.storage_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let manifest_path = path.join("manifest.json");
                    if manifest_path.exists() {
                        if let Ok(config) = self.load_tool_from_dir(&path) {
                            tools.insert(config.manifest.name.clone(), config);
                        }
                    }
                }
            }
        }

        let wasm_count = tools
            .values()
            .filter(|t| *t.tool_type() == ToolType::Wasm)
            .count();
        let script_count = tools
            .values()
            .filter(|t| *t.tool_type() == ToolType::Script)
            .count();
        eprintln!(
            "Loaded {} tools ({} WASM, {} Script)",
            tools.len(),
            wasm_count,
            script_count
        );
    }

    /// Load a single tool from its directory
    fn load_tool_from_dir(&self, tool_dir: &Path) -> Result<ToolConfig> {
        let manifest_path = tool_dir.join("manifest.json");
        let content = fs::read_to_string(&manifest_path)?;
        let manifest: ToolManifest = serde_json::from_str(&content)?;

        let tool_name = &manifest.name;

        // Determine file paths based on tool type
        let (wasm_path, script_path) = match manifest.tool_type {
            ToolType::Wasm => {
                let wasm = tool_dir.join(format!("{}.wasm", tool_name));
                (wasm, PathBuf::new())
            }
            ToolType::Script => {
                // Use entry_file if specified, otherwise infer from interpreter
                let script = if let Some(ref entry) = manifest.entry_file {
                    tool_dir.join(entry)
                } else {
                    // Fallback: infer from interpreter
                    let ext = match manifest.interpreter.as_deref() {
                        Some("python3") | Some("python") => "py",
                        Some("node") | Some("nodejs") => "js",
                        Some("ruby") => "rb",
                        Some("bash") | Some("sh") => "sh",
                        Some("perl") => "pl",
                        Some("php") => "php",
                        _ => "script",
                    };
                    tool_dir.join(format!("{}.{}", tool_name, ext))
                };
                (PathBuf::new(), script)
            }
            ToolType::Pipeline => {
                // Pipelines don't have WASM or script files
                (PathBuf::new(), PathBuf::new())
            }
        };

        // Check for environment directory
        let env_path = tool_dir.join("env");
        let env_exists = env_path.exists();

        Ok(ToolConfig {
            manifest,
            tool_dir: tool_dir.to_path_buf(),
            wasm_path,
            script_path,
            env_path: if env_exists { Some(env_path) } else { None },
            deps_installed: env_exists,
        })
    }

    /// Migrate from old single manifest.json format to per-tool directories
    fn migrate_old_format(&self) {
        let old_manifest = self.storage_dir.join("manifest.json");
        let old_scripts_dir = self.storage_dir.join("scripts");

        if !old_manifest.exists() {
            return;
        }

        eprintln!("Migrating from old manifest format...");

        // Read old manifest
        let content = match fs::read_to_string(&old_manifest) {
            Ok(c) => c,
            Err(_) => return,
        };

        // Old format structure
        #[derive(Deserialize)]
        struct OldToolConfig {
            #[allow(dead_code)]
            name: String,
            description: String,
            #[serde(default)]
            tool_type: ToolType,
            #[serde(default)]
            wasm_path: PathBuf,
            #[serde(default)]
            script_path: PathBuf,
            #[serde(default)]
            interpreter: Option<String>,
            #[serde(default)]
            input_schema: Option<serde_json::Value>,
            #[serde(default)]
            output_schema: Option<serde_json::Value>,
            #[serde(default)]
            annotations: Option<serde_json::Value>,
            #[serde(default)]
            dependencies: Vec<String>,
        }

        let old_tools: HashMap<String, OldToolConfig> = match serde_json::from_str(&content) {
            Ok(t) => t,
            Err(_) => return,
        };

        let mut migrated = 0;
        for (name, old) in old_tools {
            let tool_dir = self.storage_dir.join(&name);

            // Skip if already migrated
            if tool_dir.join("manifest.json").exists() {
                continue;
            }

            let _ = fs::create_dir_all(&tool_dir);

            // Create new manifest
            let manifest = ToolManifest {
                name: name.clone(),
                version: "1.0.0".to_string(),
                description: old.description,
                tool_type: old.tool_type.clone(),
                entry_file: None, // Will be set during migration
                interpreter: old.interpreter.clone(),
                input_schema: old
                    .input_schema
                    .map(ToolSchema::from_value)
                    .unwrap_or_else(ToolSchema::any),
                output_schema: old.output_schema.map(ToolSchema::from_value),
                annotations: old.annotations.map(ToolAnnotations::from_value),
                dependencies: old.dependencies,
                wasm_dependencies: vec![],
                pipeline_steps: vec![],
                author: None,
                license: None,
                repository: None,
                tags: vec![],
                created_at: Some(chrono_now()),
                updated_at: Some(chrono_now()),
            };

            // Save manifest
            let manifest_json = serde_json::to_string_pretty(&manifest).unwrap();
            let _ = fs::write(tool_dir.join("manifest.json"), manifest_json);

            // Move tool file
            match old.tool_type {
                ToolType::Wasm => {
                    if old.wasm_path.exists() {
                        let new_path = tool_dir.join(format!("{}.wasm", name));
                        let _ = fs::rename(&old.wasm_path, &new_path);
                    }
                }
                ToolType::Script => {
                    if old.script_path.exists() {
                        let ext = old
                            .script_path
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("script");
                        let new_path = tool_dir.join(format!("{}.{}", name, ext));
                        let _ = fs::rename(&old.script_path, &new_path);
                    }
                }
                ToolType::Pipeline => {
                    // Pipelines don't have files to migrate
                }
            }

            migrated += 1;
        }

        if migrated > 0 {
            eprintln!("Migrated {} tools to new directory format", migrated);

            // Backup and remove old manifest
            let _ = fs::rename(&old_manifest, self.storage_dir.join("manifest.json.bak"));

            // Remove old scripts directory if empty
            if old_scripts_dir.exists() {
                let _ = fs::remove_dir(&old_scripts_dir); // Only removes if empty
            }

            // Reload tools
            self.load_all_tools();
        }
    }

    /// Register a new tool or update existing one
    pub fn register_tool(&self, manifest: ToolManifest, code: &[u8]) -> Result<ToolConfig> {
        match manifest.tool_type {
            ToolType::Wasm => self.register_wasm_tool(manifest, code, ""),
            ToolType::Script => self.register_script_tool(manifest, code),
            ToolType::Pipeline => self.register_pipeline_tool(manifest),
        }
    }

    /// Register a WASM tool with optional source code preservation
    pub fn register_wasm_tool(
        &self,
        manifest: ToolManifest,
        wasm_bytes: &[u8],
        source_code: &str,
    ) -> Result<ToolConfig> {
        let tool_dir = self.storage_dir.join(&manifest.name);
        fs::create_dir_all(&tool_dir)?;

        // Save manifest
        let manifest_json = serde_json::to_string_pretty(&manifest)?;
        fs::write(tool_dir.join("manifest.json"), manifest_json)?;

        // Save WASM binary
        let wasm_path = tool_dir.join(format!("{}.wasm", manifest.name));
        fs::write(&wasm_path, wasm_bytes)?;

        // Save source code for recompilation if provided
        if !source_code.is_empty() {
            fs::write(tool_dir.join("src.rs"), source_code)?;
        }

        let config = ToolConfig {
            manifest,
            tool_dir,
            wasm_path,
            script_path: PathBuf::new(),
            env_path: None,
            deps_installed: false,
        };

        // Update in-memory cache
        let mut tools = self.tools.write().unwrap();
        tools.insert(config.manifest.name.clone(), config.clone());

        Ok(config)
    }

    /// Register a script tool
    fn register_script_tool(&self, manifest: ToolManifest, code: &[u8]) -> Result<ToolConfig> {
        let tool_dir = self.storage_dir.join(&manifest.name);
        fs::create_dir_all(&tool_dir)?;

        // Determine script filename
        let script_filename = if let Some(ref entry) = manifest.entry_file {
            entry.clone()
        } else {
            // Generate filename from interpreter
            let ext = match manifest.interpreter.as_deref() {
                Some("python3") | Some("python") => "py",
                Some("node") | Some("nodejs") => "js",
                Some("ruby") => "rb",
                Some("bash") | Some("sh") => "sh",
                Some("perl") => "pl",
                Some("php") => "php",
                _ => "script",
            };
            format!("{}.{}", manifest.name, ext)
        };

        // Update manifest with entry_file if not set
        let mut manifest = manifest;
        if manifest.entry_file.is_none() {
            manifest.entry_file = Some(script_filename.clone());
        }

        // Save manifest
        let manifest_json = serde_json::to_string_pretty(&manifest)?;
        fs::write(tool_dir.join("manifest.json"), manifest_json)?;

        // Save script file
        let script_path = tool_dir.join(&script_filename);
        fs::write(&script_path, code)?;

        // Make executable on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&script_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script_path, perms)?;
        }

        let config = ToolConfig {
            manifest,
            tool_dir,
            wasm_path: PathBuf::new(),
            script_path,
            env_path: None,
            deps_installed: false,
        };

        // Update in-memory cache
        let mut tools = self.tools.write().unwrap();
        tools.insert(config.manifest.name.clone(), config.clone());

        Ok(config)
    }

    /// Register a pipeline tool (no code, just manifest with steps)
    fn register_pipeline_tool(&self, manifest: ToolManifest) -> Result<ToolConfig> {
        let tool_dir = self.storage_dir.join(&manifest.name);
        fs::create_dir_all(&tool_dir)?;

        // Save manifest
        let manifest_json = serde_json::to_string_pretty(&manifest)?;
        fs::write(tool_dir.join("manifest.json"), manifest_json)?;

        let config = ToolConfig {
            manifest,
            tool_dir,
            wasm_path: PathBuf::new(),
            script_path: PathBuf::new(),
            env_path: None,
            deps_installed: true, // Pipelines don't need dependency installation
        };

        // Update in-memory cache
        let mut tools = self.tools.write().unwrap();
        tools.insert(config.manifest.name.clone(), config.clone());

        Ok(config)
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

    /// Get tool directory for a specific tool
    #[allow(dead_code)]
    pub fn tool_dir(&self, name: &str) -> PathBuf {
        self.storage_dir.join(name)
    }

    /// Get environment path for a specific tool
    pub fn tool_env_path(&self, tool_name: &str) -> PathBuf {
        self.storage_dir.join(tool_name).join("env")
    }

    /// Update tool's dependency status
    pub fn mark_deps_installed(&self, tool_name: &str) -> Result<()> {
        let mut tools = self.tools.write().unwrap();
        if let Some(tool) = tools.get_mut(tool_name) {
            tool.deps_installed = true;
            let env_path = tool.tool_dir.join("env");
            tool.env_path = Some(env_path);
        }
        Ok(())
    }

    /// Update the manifest for an existing tool
    #[allow(dead_code)]
    pub fn update_manifest(&self, name: &str, manifest: ToolManifest) -> Result<()> {
        let tool_dir = self.storage_dir.join(name);
        let manifest_path = tool_dir.join("manifest.json");

        let mut updated = manifest;
        updated.updated_at = Some(chrono_now());

        let json = serde_json::to_string_pretty(&updated)?;
        fs::write(manifest_path, json)?;

        // Reload tool
        if let Ok(config) = self.load_tool_from_dir(&tool_dir) {
            let mut tools = self.tools.write().unwrap();
            tools.insert(name.to_string(), config);
        }

        Ok(())
    }

    /// Delete a tool and its directory
    pub fn delete_tool(&self, name: &str) -> Result<bool> {
        let mut tools = self.tools.write().unwrap();
        if tools.remove(name).is_some() {
            let tool_dir = self.storage_dir.join(name);
            if tool_dir.exists() {
                fs::remove_dir_all(&tool_dir)?;
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Reload a tool from disk (for hot reload)
    pub fn reload_tool(&self, name: &str) -> Result<()> {
        let tool_dir = self.storage_dir.join(name);
        if !tool_dir.exists() {
            anyhow::bail!("Tool directory does not exist: {}", tool_dir.display());
        }
        
        // Load the tool from its directory
        self.load_tool_from_dir(&tool_dir)?;
        eprintln!("Reloaded tool: {}", name);
        Ok(())
    }

    /// Unload a tool from memory (but keep files on disk)
    pub fn unload_tool(&self, name: &str) {
        let mut tools = self.tools.write().unwrap();
        tools.remove(name);
        eprintln!("Unloaded tool: {}", name);
    }
}
