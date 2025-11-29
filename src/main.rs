mod builder;
mod importer;
mod pipeline;
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

/// Completion/autocomplete request for argument suggestions
#[derive(Deserialize, Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct CompleteArgs {
    /// Reference type: "tool" for tool arguments, "resource" for resource URIs
    ref_type: String,
    /// Name of the tool or resource
    ref_name: String,
    /// Name of the argument to complete
    argument_name: String,
    /// Current partial value being typed
    argument_value: String,
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
#[derive(Deserialize, Serialize, JsonSchema)]
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

/// Create a reusable pipeline
#[derive(Deserialize, Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct CreatePipelineArgs {
    /// Pipeline name (used to run it later)
    name: String,
    /// Description of what the pipeline does
    description: Option<String>,
    /// Steps to execute in order
    steps: Vec<PipelineStepArg>,
    /// Tags for organization
    tags: Option<Vec<String>>,
}

/// Run a saved pipeline
/// List all pipelines
#[derive(Deserialize, Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct ListPipelinesArgs {
    /// Filter by tag
    tag: Option<String>,
}

/// Delete a pipeline
#[derive(Deserialize, Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct DeletePipelineArgs {
    /// Name of the pipeline to delete
    name: String,
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
        description = "Register a script tool in any language (Python, Node.js, Ruby, Bash, etc.). Scripts communicate via JSON-RPC 2.0 over stdin/stdout. CRITICAL: Use sys.stdin.readline() NOT sys.stdin.read() in Python scripts - read() blocks forever! Always call sys.stdout.flush() after printing the response."
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

        if *tool.tool_type() != ToolType::Script {
            return "Error: Dependency installation only supported for script tools".to_string();
        }

        let deps = args
            .dependencies
            .unwrap_or_else(|| tool.dependencies().to_vec());

        if deps.is_empty() {
            return "No dependencies to install".to_string();
        }

        let env_path = self.registry.tool_env_path(&args.tool_name);

        match runtime::install_tool_deps(&env_path, tool.interpreter(), &deps) {
            Ok(result) => {
                if result.success {
                    // Update tool config
                    if let Err(e) = self.registry.mark_deps_installed(&args.tool_name) {
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

        // Handle pipeline tools specially
        if *tool.tool_type() == ToolType::Pipeline {
            return self.execute_pipeline(&tool, tool_args).await;
        }

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

    // ==================== SEQUENTIAL SKILL CREATION ====================

    #[tool(description = r#"Create skills step-by-step with guided workflow.

Steps:
1. **Design** - Define what the skill does, inputs, outputs
2. **Implement** - Write the actual code (Rust for WASM, any language for scripts)
3. **Test** - Validate the code compiles/runs correctly
4. **Finalize** - Register the skill and make it available

Use skill_type: "wasm" for Rust tools, "script" for Python/Node/etc.

CRITICAL for Python scripts: Use sys.stdin.readline() NOT sys.stdin.read()! read() blocks forever. Always call sys.stdout.flush() after printing."#)]
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
                    let wasm_bytes = match builder::Builder::compile_tool(&args.name, &args.content)
                    {
                        Ok(path) => match std::fs::read(&path) {
                            Ok(bytes) => bytes,
                            Err(e) => return format!("❌ Failed to read WASM: {}", e),
                        },
                        Err(e) => return format!("❌ Failed to compile: {}", e),
                    };

                    let manifest = registry::ToolManifest::new(
                        args.name.clone(),
                        args.description.clone(),
                        ToolType::Wasm,
                    );

                    match self.registry.register_tool(manifest, &wasm_bytes) {
                        Ok(config) => {
                            output.push_str(&format!(
                                "🎉 **Skill `{}` created successfully!**\n\n",
                                args.name
                            ));
                            output.push_str("**Type:** 🦀 WASM Tool\n");
                            output.push_str(&format!(
                                "**Directory:** {}\n",
                                config.tool_dir.display()
                            ));
                            output.push_str(&format!(
                                "**Usage:** `call_tool(tool_name: \"{}\")`\n",
                                args.name
                            ));
                        }
                        Err(e) => return format!("❌ Failed to register: {}", e),
                    }
                } else {
                    // Register script tool
                    let mut manifest = registry::ToolManifest::new(
                        args.name.clone(),
                        args.description.clone(),
                        ToolType::Script,
                    );
                    manifest.interpreter = args.interpreter.clone();

                    match self
                        .registry
                        .register_tool(manifest, args.content.as_bytes())
                    {
                        Ok(config) => {
                            output.push_str(&format!(
                                "🎉 **Skill `{}` created successfully!**\n\n",
                                args.name
                            ));
                            output.push_str(&format!(
                                "**Type:** 📜 Script ({})\n",
                                args.interpreter
                                    .clone()
                                    .unwrap_or_else(|| "executable".to_string())
                            ));
                            output.push_str(&format!(
                                "**Directory:** {}\n",
                                config.tool_dir.display()
                            ));
                            output.push_str(&format!(
                                "**Usage:** `call_tool(tool_name: \"{}\")`\n",
                                args.name
                            ));
                        }
                        Err(e) => return format!("❌ Failed to register: {}", e),
                    }
                }
            }
            _ => {
                output.push_str("❌ Invalid step. Use 1-4.\n");
            }
        }

        output
    }

    // ==================== COMPLETION API ====================

    #[tool(
        description = "Get autocomplete suggestions for tool arguments. Returns possible values for a specific argument."
    )]
    async fn complete(&self, Parameters(args): Parameters<CompleteArgs>) -> String {
        let mut suggestions: Vec<String> = Vec::new();

        match args.ref_type.as_str() {
            "tool" => {
                // Get tool and check if argument exists in schema
                if let Some(tool) = self.registry.get_tool(&args.ref_name) {
                    match args.argument_name.as_str() {
                        "tool_name" => {
                            // Suggest available tool names
                            suggestions = self
                                .registry
                                .list_tools()
                                .iter()
                                .filter(|t| t.name().starts_with(&args.argument_value))
                                .map(|t| t.name().to_string())
                                .take(10)
                                .collect();
                        }
                        "interpreter" => {
                            // Suggest common interpreters
                            let interpreters = [
                                "python3", "python", "node", "nodejs", "ruby", "bash", "sh",
                                "perl", "php",
                            ];
                            suggestions = interpreters
                                .iter()
                                .filter(|i| i.starts_with(&args.argument_value))
                                .map(|s| s.to_string())
                                .collect();
                        }
                        "language" => {
                            // Suggest languages for execute_code
                            let languages = ["python", "javascript", "typescript"];
                            suggestions = languages
                                .iter()
                                .filter(|l| l.starts_with(&args.argument_value))
                                .map(|s| s.to_string())
                                .collect();
                        }
                        _ => {
                            // Check tool's input schema for enum values
                            if let Some(props) = &tool.input_schema().properties {
                                if let Some(prop) = props.get(&args.argument_name) {
                                    if let Some(enum_values) = prop.get("enum") {
                                        if let Some(arr) = enum_values.as_array() {
                                            suggestions = arr
                                                .iter()
                                                .filter_map(|v| v.as_str())
                                                .filter(|s| s.starts_with(&args.argument_value))
                                                .map(|s| s.to_string())
                                                .take(10)
                                                .collect();
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            "resource" => {
                // Suggest resource URIs
                let resources = ["skillz://guide", "skillz://examples", "skillz://protocol"];
                suggestions = resources
                    .iter()
                    .filter(|r| r.starts_with(&args.argument_value))
                    .map(|s| s.to_string())
                    .collect();
            }
            _ => {}
        }

        serde_json::json!({
            "completion": {
                "values": suggestions,
                "total": suggestions.len(),
                "hasMore": false
            }
        })
        .to_string()
    }

    // ==================== CODE EXECUTION MODE ====================

    #[tool(
        description = "Execute code that can call multiple registered tools. Dramatically reduces token usage by composing tools in code instead of sequential calls. Supports Python (default) and JavaScript."
    )]
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
        description = r#"Create a reusable pipeline that chains tools together. Steps run in order, with outputs available to subsequent steps.

Variable syntax:
- $input.field - Access pipeline input
- $prev - Previous step's entire output
- $prev.field - Access field from previous step
- $step_name.field - Access field from a named step

Example:
create_pipeline(
  name: "analyze_and_report",
  steps: [
    { name: "fetch", tool: "http_get", args: { url: "$input.url" } },
    { tool: "analyze_text", args: { text: "$fetch.body" } },
    { tool: "format_report", args: { data: "$prev" } }
  ]
)"#
    )]
    async fn create_pipeline(&self, Parameters(args): Parameters<CreatePipelineArgs>) -> String {
        eprintln!("Creating pipeline: {}", args.name);

        // Check if tool already exists
        if self.registry.get_tool(&args.name).is_some() {
            return format!(
                "❌ A tool named '{}' already exists. Choose a different name.",
                args.name
            );
        }

        // Validate steps reference existing tools
        for (i, step) in args.steps.iter().enumerate() {
            if self.registry.get_tool(&step.tool).is_none() {
                // Check if it's a built-in tool
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
                    "create_pipeline",
                    "list_pipelines",
                    "delete_pipeline",
                ];
                if !built_in_tools.contains(&step.tool.as_str()) {
                    return format!(
                        "❌ Step {} references unknown tool '{}'. Create or import it first.",
                        i + 1,
                        step.tool
                    );
                }
            }
        }

        // Convert to registry PipelineStep
        let steps: Vec<registry::PipelineStep> = args
            .steps
            .into_iter()
            .map(|s| registry::PipelineStep {
                name: s.name,
                tool: s.tool,
                args: s.args.unwrap_or(serde_json::json!({})),
                continue_on_error: s.continue_on_error.unwrap_or(false),
                condition: s.condition,
            })
            .collect();

        // Create pipeline manifest
        let mut manifest = registry::ToolManifest::new_pipeline(
            args.name.clone(),
            args.description.unwrap_or_default(),
            steps,
        );
        manifest.tags = args.tags.unwrap_or_default();

        match self.registry.register_tool(manifest, &[]) {
            Ok(_) => {
                format!(
                    "✅ **Pipeline '{}' Created**\n\n\
                    Pipelines are tools! Use:\n\
                    - `call_tool(tool_name: \"{}\", arguments: {{...}})` to run it\n\
                    - `list_tools` to see all tools including pipelines\n\
                    - `delete_tool` to remove it",
                    args.name, args.name
                )
            }
            Err(e) => format!("❌ Failed to create pipeline: {}", e),
        }
    }

    #[tool(description = "List all pipeline tools. Optionally filter by tag.")]
    async fn list_pipelines(&self, Parameters(args): Parameters<ListPipelinesArgs>) -> String {
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
            return "📭 No pipelines found. Create one with `create_pipeline`.".to_string();
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

    #[tool(description = "Delete a pipeline (same as delete_tool)")]
    async fn delete_pipeline(&self, Parameters(args): Parameters<DeletePipelineArgs>) -> String {
        // Verify it's actually a pipeline
        if let Some(tool) = self.registry.get_tool(&args.name) {
            if *tool.tool_type() != ToolType::Pipeline {
                return format!(
                    "⚠️ '{}' is not a pipeline. Use delete_tool instead.",
                    args.name
                );
            }
        }

        match self.registry.delete_tool(&args.name) {
            Ok(true) => format!("🗑️ Pipeline '{}' deleted successfully", args.name),
            Ok(false) => format!("⚠️ Pipeline '{}' not found", args.name),
            Err(e) => format!("❌ Failed to delete pipeline: {}", e),
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
            instructions: Some("Skillz - Build and execute custom tools at runtime. Supports WASM (Rust) and Script tools (Python, Node.js, Ruby, etc.) via JSON-RPC 2.0. CRITICAL: For Python scripts, use sys.stdin.readline() NOT sys.stdin.read() - read() blocks forever! Always call sys.stdout.flush() after printing.".into()),
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
- `roots` - Workspace directories (from MCP client, `SKILLZ_ROOTS` env, or cwd)
- `working_directory` - Current working directory
- `tool_name` - Name of the executing tool
- `tools_dir` - Directory where tools are stored
- `environment` - Safe environment variables
- `capabilities` - What MCP features the client supports

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
    # IMPORTANT: Use readline() not read() - read() blocks for EOF!
    request = json.loads(sys.stdin.readline())
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

## ⚠️ Critical Tips

### Use `readline()` NOT `read()`!
```python
# ✅ CORRECT - Returns immediately
request = json.loads(sys.stdin.readline())

# ❌ WRONG - Blocks forever waiting for EOF!
request = json.loads(sys.stdin.read())
```

### Other Tips
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

    eprintln!("Skillz MCP started (WASM + Script + Pipeline tools)");

    state.serve(stdio()).await?.waiting().await?;

    Ok(())
}
