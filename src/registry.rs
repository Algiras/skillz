use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use anyhow::Result;
use std::fs;

/// Tool execution type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ToolType {
    /// WebAssembly module compiled from Rust
    Wasm,
    /// External script/executable with JSON-RPC 2.0 interface
    Script,
}

impl Default for ToolType {
    fn default() -> Self {
        ToolType::Wasm
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
    /// JSON Schema for tool arguments
    pub schema: serde_json::Value,
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
                            let wasm_count = loaded.values().filter(|t| t.tool_type == ToolType::Wasm).count();
                            let script_count = loaded.values().filter(|t| t.tool_type == ToolType::Script).count();
                            eprintln!("Loaded {} tools ({} WASM, {} Script)", loaded.len(), wasm_count, script_count);
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
        
        Self {
            tools,
            storage_dir,
        }
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
}
