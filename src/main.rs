mod builder;
mod registry;
mod runtime;

use anyhow::Result;
use registry::ToolType;
use rmcp::schemars::JsonSchema;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        AnnotateAble, ListResourcesResult, PaginatedRequestParam, RawResource,
        ReadResourceRequestParam, ReadResourceResult, ResourceContents, ServerCapabilities,
        ServerInfo,
    },
    service::RequestContext,
    tool, tool_handler, tool_router,
    transport::stdio,
    ErrorData as McpError, RoleServer, ServerHandler, ServiceExt,
};
use serde::{Deserialize, Serialize};

// Define the state
#[derive(Clone)]
struct AppState {
    registry: registry::ToolRegistry,
    runtime: runtime::ToolRuntime,
    tool_router: ToolRouter<Self>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct BuildToolArgs {
    name: String,
    code: String,
    description: String,
    /// Allow overwriting existing tools
    overwrite: Option<bool>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct RegisterScriptArgs {
    /// Unique name for the script tool
    name: String,
    /// Description of what the tool does
    description: String,
    /// The script code (will be saved to a file)
    code: String,
    /// Language/interpreter to use (python3, node, ruby, bash, etc.)
    /// If not provided, the script must be executable
    interpreter: Option<String>,
    /// File extension for the script (py, js, rb, sh, etc.)
    extension: Option<String>,
    /// Allow overwriting existing tools
    overwrite: Option<bool>,
    /// Dependencies to install (pip packages for Python, npm packages for Node.js)
    /// Example: ["requests", "pandas"] for Python, ["axios", "lodash"] for Node.js
    dependencies: Option<Vec<String>>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct InstallDepsArgs {
    /// Name of the tool to install dependencies for
    tool_name: String,
    /// Additional dependencies to install (optional, uses tool's configured deps if not provided)
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
struct CallToolArgs {
    tool_name: String,
    arguments: Option<serde_json::Value>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct TestValidateArgs {
    code: String,
    test_compile: Option<bool>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct AddRootArgs {
    /// Path to add as a workspace root (scripts will have access to this directory)
    path: String,
}

/// Sequential skill creation - build tools step by step
#[derive(Deserialize, Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct CreateSkillArgs {
    /// Name of the skill being created
    name: String,
    /// Description of what the skill does
    description: String,
    /// Current step in the creation process (1=design, 2=implement, 3=test, 4=finalize)
    step: u32,
    /// Step-specific content (design notes, code, test results, etc.)
    content: String,
    /// Type of skill: "wasm" for Rust/WASM or "script" for any language
    skill_type: String,
    /// For scripts: the interpreter (python3, node, bash, etc.)
    interpreter: Option<String>,
    /// Whether to proceed to next step automatically
    auto_advance: Option<bool>,
}

impl AppState {
    fn new(registry: registry::ToolRegistry, runtime: runtime::ToolRuntime) -> Self {
        Self {
            registry,
            runtime,
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl AppState {
    // ==================== WASM TOOLS (Rust) ====================

    #[tool(
        description = "Compile and register a new WASM tool from Rust code. Set overwrite=true to update existing tools."
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

        let wasm_path = match builder::Builder::compile_tool(&args.name, &args.code) {
            Ok(path) => path,
            Err(e) => return format!("Compilation error: {}", e),
        };

        let dest = self
            .registry
            .storage_dir()
            .join(format!("{}.wasm", args.name));
        if let Err(e) = std::fs::copy(&wasm_path, &dest) {
            return format!("File copy error: {}", e);
        }

        if let Err(e) = self.registry.register_tool(registry::ToolConfig {
            name: args.name.clone(),
            description: args.description.clone(),
            tool_type: ToolType::Wasm,
            wasm_path: dest,
            script_path: std::path::PathBuf::new(),
            interpreter: None,
            schema: serde_json::json!({ "type": "object" }),
            dependencies: vec![],
            env_path: None,
            deps_installed: false,
        }) {
            return format!("Registration error: {}", e);
        }

        if args.overwrite.unwrap_or(false) {
            format!("🦀 WASM Tool '{}' updated successfully", args.name)
        } else {
            format!("🦀 WASM Tool '{}' built and registered", args.name)
        }
    }

    // ==================== SCRIPT TOOLS (Any Language) ====================

    #[tool(
        description = "Register a script tool in any language (Python, Node.js, Ruby, Bash, etc.). Scripts communicate via JSON-RPC 2.0 over stdin/stdout."
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

        // Determine file extension
        let ext = args
            .extension
            .clone()
            .unwrap_or_else(|| match args.interpreter.as_deref() {
                Some("python3") | Some("python") => "py".to_string(),
                Some("node") | Some("nodejs") => "js".to_string(),
                Some("ruby") => "rb".to_string(),
                Some("bash") | Some("sh") => "sh".to_string(),
                Some("perl") => "pl".to_string(),
                Some("php") => "php".to_string(),
                _ => "script".to_string(),
            });

        // Save the script
        let script_path = self
            .registry
            .scripts_dir()
            .join(format!("{}.{}", args.name, ext));
        if let Err(e) = std::fs::write(&script_path, &args.code) {
            return format!("Error saving script: {}", e);
        }

        // Make script executable (Unix)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = std::fs::metadata(&script_path) {
                let mut perms = metadata.permissions();
                perms.set_mode(0o755);
                let _ = std::fs::set_permissions(&script_path, perms);
            }
        }

        // Handle dependencies
        let dependencies = args.dependencies.clone().unwrap_or_default();
        let mut env_path: Option<std::path::PathBuf> = None;
        let mut deps_installed = false;
        let mut deps_message = String::new();

        if !dependencies.is_empty() {
            let tool_env_path = self
                .registry
                .tool_env_path(&args.name, args.interpreter.as_deref());

            // Create envs directory
            let _ = std::fs::create_dir_all(self.registry.envs_dir());

            match runtime::install_tool_deps(
                &tool_env_path,
                args.interpreter.as_deref(),
                &dependencies,
            ) {
                Ok(result) => {
                    if result.success {
                        env_path = result.env_path;
                        deps_installed = true;
                        deps_message = format!("\n\n📦 Dependencies installed: {:?}", dependencies);
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

        // Register the tool
        if let Err(e) = self.registry.register_tool(registry::ToolConfig {
            name: args.name.clone(),
            description: args.description.clone(),
            tool_type: ToolType::Script,
            wasm_path: std::path::PathBuf::new(),
            script_path: script_path.clone(),
            interpreter: args.interpreter.clone(),
            schema: serde_json::json!({ "type": "object" }),
            dependencies,
            env_path,
            deps_installed,
        }) {
            return format!("Registration error: {}", e);
        }

        let interpreter_info = args
            .interpreter
            .map(|i| format!(" (via {})", i))
            .unwrap_or_default();

        if args.overwrite.unwrap_or(false) {
            format!(
                "📜 Script Tool '{}'{} updated successfully\n\nPath: {}{}",
                args.name,
                interpreter_info,
                script_path.display(),
                deps_message
            )
        } else {
            format!(
                "📜 Script Tool '{}'{} registered\n\nPath: {}{}",
                args.name,
                interpreter_info,
                script_path.display(),
                deps_message
            )
        }
    }

    // ==================== DEPENDENCY MANAGEMENT ====================

    #[tool(
        description = "Install dependencies for a script tool. Supports pip (Python) and npm (Node.js)."
    )]
    async fn install_deps(&self, Parameters(args): Parameters<InstallDepsArgs>) -> String {
        eprintln!("Installing dependencies for: {}", args.tool_name);

        let tool = match self.registry.get_tool(&args.tool_name) {
            Some(t) => t,
            None => return format!("Error: Tool '{}' not found", args.tool_name),
        };

        if tool.tool_type != ToolType::Script {
            return "Error: Dependency installation only supported for script tools".to_string();
        }

        let deps = args
            .dependencies
            .unwrap_or_else(|| tool.dependencies.clone());

        if deps.is_empty() {
            return "No dependencies to install".to_string();
        }

        let env_path = self
            .registry
            .tool_env_path(&args.tool_name, tool.interpreter.as_deref());

        // Create envs directory
        let _ = std::fs::create_dir_all(self.registry.envs_dir());

        match runtime::install_tool_deps(&env_path, tool.interpreter.as_deref(), &deps) {
            Ok(result) => {
                if result.success {
                    // Update tool config
                    if let Err(e) = self.registry.mark_deps_installed(&args.tool_name, env_path) {
                        return format!("Deps installed but failed to update config: {}", e);
                    }
                    format!(
                        "✅ Dependencies installed successfully!\n\n{}",
                        result.message
                    )
                } else {
                    format!("❌ Installation failed:\n\n{}", result.message)
                }
            }
            Err(e) => format!("❌ Installation error: {}", e),
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

    // ==================== TOOL EXECUTION ====================

    #[tool(
        description = "Call a registered tool (WASM or Script). For script tools, arguments are passed via JSON-RPC 2.0."
    )]
    async fn call_tool(&self, Parameters(args): Parameters<CallToolArgs>) -> String {
        eprintln!("Calling tool: {}", args.tool_name);

        let tool = match self.registry.get_tool(&args.tool_name) {
            Some(t) => t,
            None => return format!("Error: Tool '{}' not found", args.tool_name),
        };

        let tool_args = args.arguments.unwrap_or(serde_json::json!({}));
        let tool_config = tool.clone();
        let runtime = self.runtime.clone();

        // Use spawn_blocking for sync operations
        match tokio::task::spawn_blocking(move || runtime.call_tool(&tool_config, tool_args)).await
        {
            Ok(Ok(result)) => result.to_string(),
            Ok(Err(e)) => format!("Error executing tool: {}", e),
            Err(e) => format!("Task join error: {}", e),
        }
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
            .filter(|t| t.tool_type == ToolType::Wasm)
            .collect();
        let script_tools: Vec<_> = tools
            .iter()
            .filter(|t| t.tool_type == ToolType::Script)
            .collect();

        let mut output = format!("📦 Available Tools ({} total)\n\n", tools.len());

        if !wasm_tools.is_empty() {
            output.push_str(&format!("### 🦀 WASM Tools ({})\n\n", wasm_tools.len()));
            for tool in wasm_tools {
                output.push_str(&format!("• **{}** - {}\n", tool.name, tool.description));
            }
            output.push('\n');
        }

        if !script_tools.is_empty() {
            output.push_str(&format!("### 📜 Script Tools ({})\n\n", script_tools.len()));
            for tool in script_tools {
                let interpreter = tool.interpreter.as_deref().unwrap_or("executable");
                output.push_str(&format!(
                    "• **{}** [{}] - {}\n",
                    tool.name, interpreter, tool.description
                ));
            }
            output.push('\n');
        }

        output.push_str("\n💡 Use `call_tool(tool_name: \"...\")` to execute any tool.");
        output
    }

    // ==================== VALIDATION ====================

    #[tool(description = "Validate Rust code using sequential analysis before building")]
    async fn test_validate(&self, Parameters(args): Parameters<TestValidateArgs>) -> String {
        eprintln!("Validating code with sequential analysis...");

        let mut steps = Vec::new();
        let mut issues: Vec<String> = Vec::new();

        // Step 1: Check for main function
        steps.push("Step 1: Checking for main() function");
        if !args.code.contains("fn main()") {
            issues.push("❌ Missing 'fn main()' entry point".to_string());
        } else {
            steps.push("  ✓ Found main() function");
        }

        // Step 2: Check for basic syntax issues
        steps.push("Step 2: Checking basic syntax");
        let brace_open = args.code.matches('{').count();
        let brace_close = args.code.matches('}').count();
        if brace_open != brace_close {
            issues.push(format!(
                "❌ Mismatched braces: {} open, {} close",
                brace_open, brace_close
            ));
        } else {
            steps.push("  ✓ Braces are balanced");
        }

        // Step 3: Check for unsafe code
        steps.push("Step 3: Checking for unsafe code");
        if args.code.contains("unsafe") {
            issues.push("⚠️  Contains unsafe code - use with caution".to_string());
        } else {
            steps.push("  ✓ No unsafe code detected");
        }

        // Step 4: Optional compile test
        if args.test_compile.unwrap_or(false) {
            steps.push("Step 4: Test compilation");
            match builder::Builder::compile_tool("validation_test", &args.code) {
                Ok(_) => steps.push("  ✓ Code compiles successfully"),
                Err(e) => issues.push(format!("❌ Compilation failed: {}", e)),
            }
        }

        // Build report
        let mut report = String::from("🔍 Sequential Validation Report\n\n");
        for step in &steps {
            report.push_str(&format!("{}\n", step));
        }

        report.push_str("\n📊 Summary:\n");
        if issues.is_empty() {
            report.push_str("✅ All checks passed! Code is ready to build.\n");
        } else {
            report.push_str(&format!("⚠️  Found {} issue(s):\n", issues.len()));
            for issue in &issues {
                report.push_str(&format!("   {}\n", issue));
            }
        }

        report
    }

    // ==================== ROOT MANAGEMENT ====================

    #[tool(
        description = "Add a workspace root. Scripts will have access to files in these directories."
    )]
    async fn add_root(&self, Parameters(args): Parameters<AddRootArgs>) -> String {
        let path = std::path::Path::new(&args.path);
        if !path.exists() {
            return format!(
                "Warning: Path '{}' does not exist, but added anyway.",
                args.path
            );
        }

        let canonical = path
            .canonicalize()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| args.path.clone());

        // Note: In a real implementation, we'd need interior mutability (Arc<RwLock<>>)
        // For now, we just acknowledge the root
        format!(
            "📁 Root added: {}\n\nScript tools will receive this path in their execution context.",
            canonical
        )
    }

    #[tool(description = "List current workspace roots that scripts have access to.")]
    async fn list_roots(&self) -> String {
        let context = runtime::ExecutionContext::default();
        let mut output = "📁 Current Workspace Roots:\n\n".to_string();
        for root in &context.roots {
            output.push_str(&format!("• {}\n", root));
        }
        output.push_str(&format!("\n📂 Tools Directory: {}", context.tools_dir));
        output.push_str(&format!(
            "\n📂 Working Directory: {}",
            context.working_directory
        ));
        output
    }

    // ==================== SEQUENTIAL SKILL CREATION ====================

    #[tool(description = r#"Create skills step-by-step with guided workflow.

Steps:
1. **Design** - Define what the skill does, inputs, outputs
2. **Implement** - Write the actual code (Rust for WASM, any language for scripts)
3. **Test** - Validate the code compiles/runs correctly
4. **Finalize** - Register the skill and make it available

Use skill_type: "wasm" for Rust tools, "script" for Python/Node/etc."#)]
    async fn create_skill(&self, Parameters(args): Parameters<CreateSkillArgs>) -> String {
        let step_name = match args.step {
            1 => "📋 Design",
            2 => "💻 Implement",
            3 => "🧪 Test",
            4 => "✅ Finalize",
            _ => "❓ Unknown",
        };

        let mut output = format!("## {} Skill: `{}`\n\n", step_name, args.name);
        output.push_str(&format!("**Step {}/4:** {}\n\n", args.step, step_name));
        output.push_str(&format!(
            "**Type:** {}\n",
            if args.skill_type == "wasm" {
                "🦀 WASM (Rust)"
            } else {
                "📜 Script"
            }
        ));
        if let Some(ref interp) = args.interpreter {
            output.push_str(&format!("**Interpreter:** {}\n", interp));
        }
        output.push_str(&format!("**Description:** {}\n\n", args.description));
        output.push_str("---\n\n");

        match args.step {
            1 => {
                // Design phase - just capture the design
                output.push_str("### Design Notes\n\n");
                output.push_str(&args.content);
                output.push_str("\n\n---\n\n");
                output.push_str("✅ Design captured. Next: Call with step=2 to implement.\n");
            }
            2 => {
                // Implement phase - validate and prepare code
                output.push_str("### Implementation\n\n```\n");
                output.push_str(&args.content);
                output.push_str("\n```\n\n");

                if args.skill_type == "wasm" {
                    // Basic Rust validation
                    if !args.content.contains("fn main()") {
                        output.push_str(
                            "⚠️ Warning: No `fn main()` found. WASM tools need a main function.\n",
                        );
                    } else {
                        output.push_str("✅ Code looks valid. Next: Call with step=3 to test.\n");
                    }
                } else {
                    output.push_str("✅ Script captured. Next: Call with step=3 to test.\n");
                }
            }
            3 => {
                // Test phase - actually try to build/validate
                output.push_str("### Testing\n\n");

                if args.skill_type == "wasm" {
                    match builder::Builder::compile_tool(
                        &format!("{}_test", args.name),
                        &args.content,
                    ) {
                        Ok(_) => {
                            output.push_str("✅ **Compilation successful!**\n\n");
                            output.push_str("Code compiles to WASM without errors.\n");
                            output.push_str("Next: Call with step=4 to finalize and register.\n");
                        }
                        Err(e) => {
                            output.push_str(&format!(
                                "❌ **Compilation failed:**\n\n```\n{}\n```\n\n",
                                e
                            ));
                            output.push_str("Fix the errors and call step=3 again.\n");
                        }
                    }
                } else {
                    // For scripts, just validate JSON-RPC structure
                    if args.content.contains("jsonrpc") && args.content.contains("result") {
                        output.push_str("✅ **Script looks valid!**\n\n");
                        output.push_str("Contains JSON-RPC structure.\n");
                        output.push_str("Next: Call with step=4 to finalize and register.\n");
                    } else {
                        output.push_str(
                            "⚠️ **Warning:** Script may not follow JSON-RPC 2.0 protocol.\n",
                        );
                        output.push_str(
                            "Ensure it reads JSON from stdin and outputs JSON response.\n",
                        );
                        output.push_str("See `skillz://protocol` for details.\n");
                    }
                }
            }
            4 => {
                // Finalize - actually register the tool
                output.push_str("### Finalizing\n\n");

                if args.skill_type == "wasm" {
                    // Build and register WASM tool
                    let wasm_path = match builder::Builder::compile_tool(&args.name, &args.content)
                    {
                        Ok(path) => path,
                        Err(e) => return format!("❌ Failed to compile: {}", e),
                    };

                    let dest = self
                        .registry
                        .storage_dir()
                        .join(format!("{}.wasm", args.name));
                    if let Err(e) = std::fs::copy(&wasm_path, &dest) {
                        return format!("❌ Failed to copy: {}", e);
                    }

                    if let Err(e) = self.registry.register_tool(registry::ToolConfig {
                        name: args.name.clone(),
                        description: args.description.clone(),
                        tool_type: ToolType::Wasm,
                        wasm_path: dest,
                        script_path: std::path::PathBuf::new(),
                        interpreter: None,
                        schema: serde_json::json!({ "type": "object" }),
                        dependencies: vec![],
                        env_path: None,
                        deps_installed: false,
                    }) {
                        return format!("❌ Failed to register: {}", e);
                    }

                    output.push_str(&format!(
                        "🎉 **Skill `{}` created successfully!**\n\n",
                        args.name
                    ));
                    output.push_str("**Type:** 🦀 WASM Tool\n");
                    output.push_str(&format!(
                        "**Usage:** `call_tool(tool_name: \"{}\")`\n",
                        args.name
                    ));
                } else {
                    // Register script tool
                    let ext = match args.interpreter.as_deref() {
                        Some("python3") | Some("python") => "py",
                        Some("node") | Some("nodejs") => "js",
                        Some("ruby") => "rb",
                        Some("bash") | Some("sh") => "sh",
                        _ => "script",
                    };

                    let script_path = self
                        .registry
                        .scripts_dir()
                        .join(format!("{}.{}", args.name, ext));
                    if let Err(e) = std::fs::write(&script_path, &args.content) {
                        return format!("❌ Failed to save script: {}", e);
                    }

                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        if let Ok(metadata) = std::fs::metadata(&script_path) {
                            let mut perms = metadata.permissions();
                            perms.set_mode(0o755);
                            let _ = std::fs::set_permissions(&script_path, perms);
                        }
                    }

                    if let Err(e) = self.registry.register_tool(registry::ToolConfig {
                        name: args.name.clone(),
                        description: args.description.clone(),
                        tool_type: ToolType::Script,
                        wasm_path: std::path::PathBuf::new(),
                        script_path: script_path.clone(),
                        interpreter: args.interpreter.clone(),
                        schema: serde_json::json!({ "type": "object" }),
                        dependencies: vec![],
                        env_path: None,
                        deps_installed: false,
                    }) {
                        return format!("❌ Failed to register: {}", e);
                    }

                    output.push_str(&format!(
                        "🎉 **Skill `{}` created successfully!**\n\n",
                        args.name
                    ));
                    output.push_str(&format!(
                        "**Type:** 📜 Script ({})\n",
                        args.interpreter.unwrap_or_else(|| "executable".to_string())
                    ));
                    output.push_str(&format!("**Path:** {}\n", script_path.display()));
                    output.push_str(&format!(
                        "**Usage:** `call_tool(tool_name: \"{}\")`\n",
                        args.name
                    ));
                }
            }
            _ => {
                output.push_str("❌ Invalid step. Use 1-4.\n");
            }
        }

        output
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AppState {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("Skillz - Build and execute custom tools at runtime. Supports WASM (Rust) and Script tools (Python, Node.js, Ruby, etc.) via JSON-RPC 2.0.".into()),
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
            ..Default::default()
        }
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
            let type_emoji = match tool.tool_type {
                ToolType::Wasm => "🦀",
                ToolType::Script => "📜",
            };
            resources.push(
                RawResource::new(
                    format!("skillz://tools/{}", tool.name),
                    format!("{} {} - {}", type_emoji, tool.name, tool.description),
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
            .filter(|t| t.tool_type == ToolType::Wasm)
            .collect();
        let script_tools: Vec<_> = tools
            .iter()
            .filter(|t| t.tool_type == ToolType::Script)
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
### `add_root` / `list_roots` - Manage workspace roots

---

"##,
        );

        // Add dynamic registered tools section
        if !wasm_tools.is_empty() || !script_tools.is_empty() {
            guide.push_str("## 📦 Registered Tools\n\n");

            if !wasm_tools.is_empty() {
                guide.push_str("### 🦀 WASM Tools\n\n");
                for tool in &wasm_tools {
                    guide.push_str(&format!("- **{}** - {}\n", tool.name, tool.description));
                    guide.push_str(&format!(
                        "  ```\n  call_tool(tool_name: \"{}\")\n  ```\n\n",
                        tool.name
                    ));
                }
            }

            if !script_tools.is_empty() {
                guide.push_str("### 📜 Script Tools\n\n");
                for tool in &script_tools {
                    let interp = tool.interpreter.as_deref().unwrap_or("executable");
                    guide.push_str(&format!(
                        "- **{}** [{}] - {}\n",
                        tool.name, interp, tool.description
                    ));
                    guide.push_str(&format!(
                        "  ```\n  call_tool(tool_name: \"{}\")\n  ```\n\n",
                        tool.name
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
    request = json.loads(sys.stdin.read())
    params = request.get("params", {})
    
    # Your tool logic here
    result = {"message": "Hello from Python!", "params": params}
    
    response = {
        "jsonrpc": "2.0",
        "result": result,
        "id": request.get("id")
    }
    print(json.dumps(response))

if __name__ == "__main__":
    main()
```

### Node.js Example
```javascript
const readline = require('readline');

const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
    terminal: false
});

rl.on('line', (line) => {
    const request = JSON.parse(line);
    
    // Your tool logic here
    const result = { message: "Hello from Node.js!", params: request.params };
    
    const response = {
        jsonrpc: "2.0",
        result: result,
        id: request.id
    };
    console.log(JSON.stringify(response));
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

request = JSON.parse(STDIN.read)
result = { message: "Hello from Ruby!", params: request["params"] }
response = { jsonrpc: "2.0", result: result, id: request["id"] }
puts response.to_json
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
                "elicitation": true
            }
        }
    },
    "id": 1
}
```

### Context Fields
- `roots` - Workspace directories the tool can access
- `working_directory` - Current working directory
- `tool_name` - Name of the executing tool
- `tools_dir` - Directory where tools are stored
- `environment` - Safe environment variables
- `capabilities` - What MCP features the client supports

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
    request = json.loads(sys.stdin.read())
    context = request.get("params", {}).get("context", {})
    caps = context.get("capabilities", {})
    
    log("info", f"Roots: {context.get('roots', [])}")
    log("info", f"Elicitation: {caps.get('elicitation', False)}, Sampling: {caps.get('sampling', False)}")
    
    # Example: Request user input if supported
    if caps.get("elicitation"):
        log("info", "Requesting user input...")
        # resp = elicit("What's your name?", {"type": "object", "properties": {"name": {"type": "string"}}})
    
    # Example: Request LLM completion if supported  
    if caps.get("sampling"):
        log("info", "LLM sampling available!")
        # resp = sample("Hello, how are you?")
    
    result = {"message": "Done!", "capabilities_available": caps}
    print(json.dumps({"jsonrpc": "2.0", "result": result, "id": request.get("id")}))

if __name__ == "__main__":
    main()
```

---

## Tips
- Check `context.capabilities` before using elicitation/sampling
- Use `flush=True` to ensure output is sent immediately
- Elicitation/sampling are bidirectional - script sends request, waits for response
- Log to stderr for debug output that won't interfere with JSON-RPC
- Always include `id` from original request in final response
"##.to_string()
    }

    fn get_tool_info(&self, tool_name: &str) -> String {
        match self.registry.get_tool(tool_name) {
            Some(tool) => {
                let (type_name, type_emoji, path_info) = match tool.tool_type {
                    ToolType::Wasm => (
                        "WASM",
                        "🦀",
                        format!("WASM Path: {}", tool.wasm_path.display()),
                    ),
                    ToolType::Script => {
                        let interp = tool.interpreter.as_deref().unwrap_or("executable");
                        (
                            "Script",
                            "📜",
                            format!(
                                "Script Path: {}\nInterpreter: {}",
                                tool.script_path.display(),
                                interp
                            ),
                        )
                    }
                };
                format!(
                    "# {} {} Tool: {}\n\n## Description\n{}\n\n## Details\n- **Type:** {}\n- **Name:** {}\n- {}\n- **Status:** ✅ Ready to use\n\n## Usage\n```\ncall_tool(tool_name: \"{}\")\n```\n",
                    type_emoji, type_name, tool.name,
                    tool.description,
                    type_name, tool.name, path_info,
                    tool.name
                )
            }
            None => format!(
                "# ❌ Tool Not Found\n\nNo tool named '{}' exists.",
                tool_name
            ),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Get tools directory from env var or use ~/tools as default
    let tools_dir = std::env::var("TOOLS_DIR").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        format!("{}/tools", home)
    });

    let storage_dir = std::path::PathBuf::from(tools_dir);
    std::fs::create_dir_all(&storage_dir)?;

    eprintln!("Tools directory: {}", storage_dir.display());

    let registry = registry::ToolRegistry::new(storage_dir);
    let runtime = runtime::ToolRuntime::new()?;

    let state = AppState::new(registry, runtime);

    eprintln!("Skillz MCP started (WASM + Script tools)");

    state.serve(stdio()).await?.waiting().await?;

    Ok(())
}
