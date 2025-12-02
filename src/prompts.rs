use rmcp::model::{GetPromptResult, Prompt, PromptArgument, PromptMessage, PromptMessageRole};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// A Skill/Prompt definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPrompt {
    pub name: String,
    pub description: String,
    pub arguments: Vec<SkillPromptArg>,
    /// Template messages with {{argument}} placeholders
    pub template: Vec<PromptTemplate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPromptArg {
    pub name: String,
    pub description: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTemplate {
    pub role: String, // "user" or "assistant"
    pub content: String,
}

impl SkillPrompt {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            arguments: Vec::new(),
            template: Vec::new(),
        }
    }

    pub fn with_arg(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        required: bool,
    ) -> Self {
        self.arguments.push(SkillPromptArg {
            name: name.into(),
            description: description.into(),
            required,
        });
        self
    }

    pub fn with_message(mut self, role: &str, content: impl Into<String>) -> Self {
        self.template.push(PromptTemplate {
            role: role.to_string(),
            content: content.into(),
        });
        self
    }

    /// Render the prompt with given arguments
    pub fn render(&self, args: &HashMap<String, String>) -> GetPromptResult {
        let messages: Vec<PromptMessage> = self
            .template
            .iter()
            .map(|t| {
                let mut content = t.content.clone();
                for (key, value) in args {
                    content = content.replace(&format!("{{{{{}}}}}", key), value);
                }
                let role = match t.role.as_str() {
                    "assistant" => PromptMessageRole::Assistant,
                    _ => PromptMessageRole::User,
                };
                PromptMessage::new_text(role, content)
            })
            .collect();

        GetPromptResult {
            description: Some(self.description.clone()),
            messages,
        }
    }

    /// Convert to MCP Prompt for listing
    pub fn to_mcp_prompt(&self) -> Prompt {
        Prompt {
            name: self.name.clone(),
            description: Some(self.description.clone()),
            arguments: Some(
                self.arguments
                    .iter()
                    .map(|a| PromptArgument {
                        name: a.name.clone(),
                        description: Some(a.description.clone()),
                        required: Some(a.required),
                        title: None,
                    })
                    .collect(),
            ),
            title: None,
            icons: None,
        }
    }
}

/// Manages prompts/skills
#[derive(Clone)]
pub struct PromptRegistry {
    prompts: Arc<RwLock<HashMap<String, SkillPrompt>>>,
}

impl Default for PromptRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptRegistry {
    pub fn new() -> Self {
        Self {
            prompts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn new_with_defaults() -> Self {
        let registry = Self::new();
        registry.register_builtin_prompts();
        registry
    }

    fn register_builtin_prompts(&self) {
        // Prompt for creating a WASM tool
        self.register(
            SkillPrompt::new(
                "create_wasm_tool",
                "Generate a WASM tool from a description. Creates Rust code that compiles to WebAssembly.",
            )
            .with_arg("name", "Name for the tool (snake_case)", true)
            .with_arg("description", "What the tool should do", true)
            .with_arg("input_example", "Example input JSON (optional)", false)
            .with_message(
                "user",
                r#"Create a WASM tool for Skillz with the following requirements:

**Name:** {{name}}
**Description:** {{description}}
**Example Input:** {{input_example}}

Generate a complete Rust implementation that:
1. Parses JSON input from command line args or stdin
2. Implements the core logic
3. Outputs JSON result to stdout

Use build_tool() to register it with appropriate input_schema and annotations.

Example format:
```
build_tool(
  name: "{{name}}",
  description: "{{description}}",
  code: "fn main() { ... }",
  input_schema: { "type": "object", "properties": {...} },
  annotations: { "readOnlyHint": true }
)
```"#,
            ),
        );

        // Prompt for creating a Python script tool
        self.register(
            SkillPrompt::new(
                "create_python_tool",
                "Generate a Python script tool with proper JSON-RPC 2.0 protocol handling.",
            )
            .with_arg("name", "Name for the tool (snake_case)", true)
            .with_arg("description", "What the tool should do", true)
            .with_arg(
                "dependencies",
                "Python packages needed (comma-separated)",
                false,
            )
            .with_message(
                "user",
                r#"Create a Python script tool for Skillz with the following requirements:

**Name:** {{name}}
**Description:** {{description}}
**Dependencies:** {{dependencies}}

Generate a complete Python script that:
1. Uses `sys.stdin.readline()` (NOT `read()`) to read JSON-RPC request
2. Extracts arguments from `request['params']['arguments']`
3. Implements the core logic
4. Returns proper JSON-RPC 2.0 response with `result` and `id`
5. Uses `sys.stdout.flush()` after printing

Use register_script() to register it:
```
register_script(
  name: "{{name}}",
  description: "{{description}}",
  interpreter: "python3",
  dependencies: [...],
  input_schema: { "type": "object", "properties": {...} },
  code: "..."
)
```"#,
            ),
        );

        // Prompt for creating a pipeline
        self.register(
            SkillPrompt::new(
                "create_pipeline",
                "Generate a pipeline that chains multiple tools together.",
            )
            .with_arg("name", "Name for the pipeline (snake_case)", true)
            .with_arg("description", "What the pipeline should accomplish", true)
            .with_arg("tools", "Tools to chain (comma-separated)", true)
            .with_message(
                "user",
                r#"Create a pipeline for Skillz that chains these tools:

**Name:** {{name}}
**Description:** {{description}}
**Tools to use:** {{tools}}

Generate a pipeline definition that:
1. Chains the tools in a logical order
2. Uses variable resolution ($input, $prev, $step_name) to pass data
3. Names important steps for later reference
4. Handles errors appropriately

Use pipeline() to create it:
```
pipeline(
  action: "create",
  name: "{{name}}",
  description: "{{description}}",
  steps: [
    { name: "step1", tool: "first_tool", args: { input: "$input.data" } },
    { name: "step2", tool: "second_tool", args: { data: "$step1.output" } },
    { tool: "final_tool", args: { result: "$prev" } }
  ]
)
```

Variable syntax:
- `$input.field` - Access pipeline input
- `$prev` - Previous step's entire output  
- `$prev.field` - Field from previous step
- `$step_name.field` - Field from a named step"#,
            ),
        );

        // Prompt for importing an MCP server
        self.register(
            SkillPrompt::new(
                "import_mcp_server",
                "Import an external MCP server and expose its tools under a namespace.",
            )
            .with_arg("name", "Namespace for the server's tools", true)
            .with_arg("package", "NPM or PyPI package name", true)
            .with_arg(
                "runner",
                "Package runner: npx, uvx, or command path",
                false,
            )
            .with_message(
                "user",
                r#"Import an external MCP server into Skillz:

**Namespace:** {{name}}
**Package:** {{package}}
**Runner:** {{runner}}

Use import_mcp() to register the server:
```
import_mcp(
  name: "{{name}}",
  command: "{{runner}}",
  args: ["{{package}}"],
  description: "Tools from {{package}}"
)
```

After importing, all tools from the server will be available as `{{name}}_toolname`.

Common runners:
- `uvx` - Python packages (recommended for Python MCP servers)
- `npx` - NPM packages (for Node.js MCP servers)
- Direct path - For locally installed servers

Example servers:
- `uvx mcp-server-time` → time_get_current_time, time_convert_time
- `npx @anthropic/mcp-server-memory` → memory_store, memory_retrieve"#,
            ),
        );

        // Prompt for analyzing and improving a tool
        self.register(
            SkillPrompt::new(
                "improve_tool",
                "Analyze an existing tool and suggest improvements.",
            )
            .with_arg("tool_name", "Name of the tool to analyze", true)
            .with_message(
                "user",
                r#"Analyze the Skillz tool "{{tool_name}}" and suggest improvements:

1. First, use `call_tool(tool_name: "{{tool_name}}")` with sample inputs to understand its behavior
2. Review its manifest with the skillz://tool/{{tool_name}} resource
3. Suggest improvements for:
   - Error handling
   - Input validation
   - Performance
   - Documentation (description, input_schema)
   - Annotations (readOnlyHint, destructiveHint, etc.)

Provide concrete code changes using build_tool() or register_script() with overwrite: true"#,
            ),
        );

        // Prompt for creating a tool from an API
        self.register(
            SkillPrompt::new(
                "create_api_tool",
                "Create a tool that wraps an external API.",
            )
            .with_arg("name", "Name for the tool", true)
            .with_arg("api_url", "Base URL of the API", true)
            .with_arg("description", "What the API does", true)
            .with_message(
                "user",
                r#"Create a Python tool that wraps this API:

**Name:** {{name}}
**API URL:** {{api_url}}
**Description:** {{description}}

Generate a tool that:
1. Uses the `requests` library to call the API
2. Handles authentication via SKILLZ_* environment variables
3. Parses and returns JSON responses
4. Includes proper error handling for HTTP errors
5. Has a well-defined input_schema

Use register_script() with dependencies: ["requests"]:
```
register_script(
  name: "{{name}}",
  description: "{{description}}",
  interpreter: "python3",
  dependencies: ["requests"],
  input_schema: { ... },
  code: "..."
)
```

Access secrets via: `request['params']['context']['environment']['SKILLZ_API_KEY']`"#,
            ),
        );
    }

    pub fn register(&self, prompt: SkillPrompt) {
        let mut prompts = self.prompts.write().unwrap();
        prompts.insert(prompt.name.clone(), prompt);
    }

    pub fn get(&self, name: &str) -> Option<SkillPrompt> {
        self.prompts.read().unwrap().get(name).cloned()
    }

    pub fn list(&self) -> Vec<Prompt> {
        self.prompts
            .read()
            .unwrap()
            .values()
            .map(|p| p.to_mcp_prompt())
            .collect()
    }

    pub fn list_prompts_result(&self) -> rmcp::model::ListPromptsResult {
        rmcp::model::ListPromptsResult {
            prompts: self.list(),
            next_cursor: None,
        }
    }

    pub fn get_prompt_result(
        &self,
        name: &str,
        args: Option<HashMap<String, String>>,
    ) -> Result<GetPromptResult, String> {
        let prompt = self
            .get(name)
            .ok_or_else(|| format!("Prompt '{}' not found", name))?;
        Ok(prompt.render(&args.unwrap_or_default()))
    }
}
