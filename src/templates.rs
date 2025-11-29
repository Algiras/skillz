//! Tool Templates - Pre-built skeletons for common tool patterns
//!
//! Templates accelerate tool creation by providing working starting points
//! that can be customized for specific use cases.
//!
//! Templates are stored in `TOOLS_DIR/templates/` as JSON files.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// A tool template with placeholders
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    /// Template name
    pub name: String,
    /// Description of what this template creates
    pub description: String,
    /// Template category
    pub category: TemplateCategory,
    /// Tool type this template creates
    pub tool_type: TemplateToolType,
    /// Template variables with descriptions
    pub variables: Vec<TemplateVariable>,
    /// The template code with {{variable}} placeholders
    pub code: String,
    /// Default dependencies
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Example usage
    #[serde(default)]
    pub example: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TemplateCategory {
    Api,
    Data,
    File,
    Utility,
    Integration,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TemplateToolType {
    Python,
    Node,
    Wasm,
    Pipeline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateVariable {
    /// Variable name (used as {{name}} in template)
    pub name: String,
    /// Description of what this variable is for
    pub description: String,
    /// Default value if not provided
    #[serde(default)]
    pub default: Option<String>,
    /// Whether this variable is required
    #[serde(default = "default_true")]
    pub required: bool,
}

fn default_true() -> bool {
    true
}

impl Template {
    /// Render the template with provided variables
    pub fn render(&self, vars: &HashMap<String, String>) -> Result<String, String> {
        let mut code = self.code.clone();

        // Check required variables
        for var in &self.variables {
            let value = vars.get(&var.name).or(var.default.as_ref());

            if var.required && value.is_none() {
                return Err(format!("Missing required variable: {}", var.name));
            }

            if let Some(val) = value {
                code = code.replace(&format!("{{{{{}}}}}", var.name), val);
            }
        }

        Ok(code)
    }

    /// Get list of variable names
    pub fn variable_names(&self) -> Vec<&str> {
        self.variables.iter().map(|v| v.name.as_str()).collect()
    }
}

/// Built-in templates
pub fn builtin_templates() -> Vec<Template> {
    vec![
        // API Client Template
        Template {
            name: "api_client".to_string(),
            description: "HTTP API client with error handling".to_string(),
            category: TemplateCategory::Api,
            tool_type: TemplateToolType::Python,
            variables: vec![
                TemplateVariable {
                    name: "tool_name".to_string(),
                    description: "Name for the tool".to_string(),
                    default: None,
                    required: true,
                },
                TemplateVariable {
                    name: "base_url".to_string(),
                    description: "Base URL for the API".to_string(),
                    default: None,
                    required: true,
                },
                TemplateVariable {
                    name: "description".to_string(),
                    description: "Tool description".to_string(),
                    default: Some("API client tool".to_string()),
                    required: false,
                },
            ],
            code: r#"#!/usr/bin/env python3
"""{{description}}"""
import json
import sys
import requests

def main():
    request = json.loads(sys.stdin.readline())
    args = request.get('params', {}).get('arguments', {})
    
    base_url = "{{base_url}}"
    endpoint = args.get('endpoint', '/')
    method = args.get('method', 'GET').upper()
    data = args.get('data')
    headers = args.get('headers', {})
    
    try:
        url = f"{base_url}{endpoint}"
        
        if method == 'GET':
            resp = requests.get(url, headers=headers, params=data)
        elif method == 'POST':
            resp = requests.post(url, headers=headers, json=data)
        elif method == 'PUT':
            resp = requests.put(url, headers=headers, json=data)
        elif method == 'DELETE':
            resp = requests.delete(url, headers=headers)
        else:
            raise ValueError(f"Unsupported method: {method}")
        
        result = {
            "status_code": resp.status_code,
            "headers": dict(resp.headers),
            "body": resp.json() if resp.headers.get('content-type', '').startswith('application/json') else resp.text
        }
        
        response = {"jsonrpc": "2.0", "result": result, "id": request.get("id")}
    except Exception as e:
        response = {"jsonrpc": "2.0", "error": {"code": -1, "message": str(e)}, "id": request.get("id")}
    
    print(json.dumps(response))
    sys.stdout.flush()

if __name__ == "__main__":
    main()
"#.to_string(),
            dependencies: vec!["requests".to_string()],
            example: Some(r#"Use template: template(action: "use", name: "api_client", variables: {"tool_name": "github_api", "base_url": "https://api.github.com"})"#.to_string()),
        },

        // Data Processor Template
        Template {
            name: "data_processor".to_string(),
            description: "Process and transform data with pandas".to_string(),
            category: TemplateCategory::Data,
            tool_type: TemplateToolType::Python,
            variables: vec![
                TemplateVariable {
                    name: "tool_name".to_string(),
                    description: "Name for the tool".to_string(),
                    default: None,
                    required: true,
                },
                TemplateVariable {
                    name: "description".to_string(),
                    description: "Tool description".to_string(),
                    default: Some("Data processing tool".to_string()),
                    required: false,
                },
            ],
            code: r#"#!/usr/bin/env python3
"""{{description}}"""
import json
import sys
import pandas as pd
from io import StringIO

def main():
    request = json.loads(sys.stdin.readline())
    args = request.get('params', {}).get('arguments', {})
    
    try:
        data = args.get('data')
        operation = args.get('operation', 'describe')
        
        # Parse data (CSV string, JSON, or list of dicts)
        if isinstance(data, str):
            df = pd.read_csv(StringIO(data))
        elif isinstance(data, list):
            df = pd.DataFrame(data)
        else:
            df = pd.DataFrame([data])
        
        if operation == 'describe':
            result = df.describe().to_dict()
        elif operation == 'head':
            n = args.get('n', 5)
            result = df.head(n).to_dict('records')
        elif operation == 'tail':
            n = args.get('n', 5)
            result = df.tail(n).to_dict('records')
        elif operation == 'filter':
            column = args.get('column')
            value = args.get('value')
            result = df[df[column] == value].to_dict('records')
        elif operation == 'sort':
            column = args.get('column')
            ascending = args.get('ascending', True)
            result = df.sort_values(column, ascending=ascending).to_dict('records')
        elif operation == 'groupby':
            column = args.get('column')
            agg = args.get('agg', 'count')
            result = df.groupby(column).agg(agg).to_dict()
        else:
            result = {"rows": len(df), "columns": list(df.columns)}
        
        response = {"jsonrpc": "2.0", "result": result, "id": request.get("id")}
    except Exception as e:
        response = {"jsonrpc": "2.0", "error": {"code": -1, "message": str(e)}, "id": request.get("id")}
    
    print(json.dumps(response))
    sys.stdout.flush()

if __name__ == "__main__":
    main()
"#.to_string(),
            dependencies: vec!["pandas".to_string()],
            example: Some(r#"Use template: template(action: "use", name: "data_processor", variables: {"tool_name": "csv_analyzer"})"#.to_string()),
        },

        // File Handler Template
        Template {
            name: "file_handler".to_string(),
            description: "Read, write, and manipulate files".to_string(),
            category: TemplateCategory::File,
            tool_type: TemplateToolType::Python,
            variables: vec![
                TemplateVariable {
                    name: "tool_name".to_string(),
                    description: "Name for the tool".to_string(),
                    default: None,
                    required: true,
                },
                TemplateVariable {
                    name: "allowed_extensions".to_string(),
                    description: "Comma-separated allowed file extensions".to_string(),
                    default: Some(".txt,.json,.csv,.md".to_string()),
                    required: false,
                },
                TemplateVariable {
                    name: "description".to_string(),
                    description: "Tool description".to_string(),
                    default: Some("File handling tool".to_string()),
                    required: false,
                },
            ],
            code: r#"#!/usr/bin/env python3
"""{{description}}"""
import json
import sys
import os

ALLOWED_EXTENSIONS = "{{allowed_extensions}}".split(',')

def main():
    request = json.loads(sys.stdin.readline())
    args = request.get('params', {}).get('arguments', {})
    context = request.get('params', {}).get('context', {})
    
    try:
        operation = args.get('operation', 'read')
        path = args.get('path')
        
        if not path:
            raise ValueError("path is required")
        
        # Security: check extension
        ext = os.path.splitext(path)[1].lower()
        if ext not in ALLOWED_EXTENSIONS:
            raise ValueError(f"Extension {ext} not allowed. Allowed: {ALLOWED_EXTENSIONS}")
        
        # Resolve path relative to roots
        roots = context.get('roots', [])
        if roots and not os.path.isabs(path):
            path = os.path.join(roots[0], path)
        
        if operation == 'read':
            with open(path, 'r') as f:
                result = {"content": f.read(), "path": path}
        elif operation == 'write':
            content = args.get('content', '')
            with open(path, 'w') as f:
                f.write(content)
            result = {"written": len(content), "path": path}
        elif operation == 'append':
            content = args.get('content', '')
            with open(path, 'a') as f:
                f.write(content)
            result = {"appended": len(content), "path": path}
        elif operation == 'exists':
            result = {"exists": os.path.exists(path), "path": path}
        elif operation == 'info':
            stat = os.stat(path)
            result = {
                "path": path,
                "size": stat.st_size,
                "modified": stat.st_mtime,
                "is_file": os.path.isfile(path),
                "is_dir": os.path.isdir(path)
            }
        else:
            raise ValueError(f"Unknown operation: {operation}")
        
        response = {"jsonrpc": "2.0", "result": result, "id": request.get("id")}
    except Exception as e:
        response = {"jsonrpc": "2.0", "error": {"code": -1, "message": str(e)}, "id": request.get("id")}
    
    print(json.dumps(response))
    sys.stdout.flush()

if __name__ == "__main__":
    main()
"#.to_string(),
            dependencies: vec![],
            example: Some(r#"Use template: template(action: "use", name: "file_handler", variables: {"tool_name": "config_reader", "allowed_extensions": ".json,.yaml,.toml"})"#.to_string()),
        },

        // Web Scraper Template
        Template {
            name: "web_scraper".to_string(),
            description: "Scrape and parse web pages".to_string(),
            category: TemplateCategory::Integration,
            tool_type: TemplateToolType::Python,
            variables: vec![
                TemplateVariable {
                    name: "tool_name".to_string(),
                    description: "Name for the tool".to_string(),
                    default: None,
                    required: true,
                },
                TemplateVariable {
                    name: "description".to_string(),
                    description: "Tool description".to_string(),
                    default: Some("Web scraping tool".to_string()),
                    required: false,
                },
            ],
            code: r#"#!/usr/bin/env python3
"""{{description}}"""
import json
import sys
import requests
from bs4 import BeautifulSoup

def main():
    request = json.loads(sys.stdin.readline())
    args = request.get('params', {}).get('arguments', {})
    
    try:
        url = args.get('url')
        if not url:
            raise ValueError("url is required")
        
        selector = args.get('selector')  # CSS selector
        extract = args.get('extract', 'text')  # text, html, attr
        attr_name = args.get('attr')  # for extract=attr
        
        resp = requests.get(url, headers={'User-Agent': 'Mozilla/5.0'})
        resp.raise_for_status()
        
        soup = BeautifulSoup(resp.text, 'html.parser')
        
        if selector:
            elements = soup.select(selector)
            if extract == 'text':
                result = [el.get_text(strip=True) for el in elements]
            elif extract == 'html':
                result = [str(el) for el in elements]
            elif extract == 'attr' and attr_name:
                result = [el.get(attr_name) for el in elements]
            else:
                result = [el.get_text(strip=True) for el in elements]
        else:
            result = {
                "title": soup.title.string if soup.title else None,
                "links": [a.get('href') for a in soup.find_all('a', href=True)][:20],
                "text_preview": soup.get_text()[:500]
            }
        
        response = {"jsonrpc": "2.0", "result": {"data": result, "url": url}, "id": request.get("id")}
    except Exception as e:
        response = {"jsonrpc": "2.0", "error": {"code": -1, "message": str(e)}, "id": request.get("id")}
    
    print(json.dumps(response))
    sys.stdout.flush()

if __name__ == "__main__":
    main()
"#.to_string(),
            dependencies: vec!["requests".to_string(), "beautifulsoup4".to_string()],
            example: Some(r#"Use template: template(action: "use", name: "web_scraper", variables: {"tool_name": "news_scraper"})"#.to_string()),
        },

        // WASM Calculator Template
        Template {
            name: "calculator".to_string(),
            description: "Mathematical calculator with custom operations".to_string(),
            category: TemplateCategory::Utility,
            tool_type: TemplateToolType::Wasm,
            variables: vec![
                TemplateVariable {
                    name: "tool_name".to_string(),
                    description: "Name for the tool".to_string(),
                    default: None,
                    required: true,
                },
                TemplateVariable {
                    name: "description".to_string(),
                    description: "Tool description".to_string(),
                    default: Some("Calculator tool".to_string()),
                    required: false,
                },
            ],
            code: r#"use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Input {
    operation: String,
    a: f64,
    b: Option<f64>,
}

#[derive(Serialize)]
struct Output {
    result: f64,
    operation: String,
}

fn main() {
    let input: Value = serde_json::from_reader(std::io::stdin()).unwrap();
    let args: Input = serde_json::from_value(input).unwrap();
    
    let result = match args.operation.as_str() {
        "add" => args.a + args.b.unwrap_or(0.0),
        "subtract" => args.a - args.b.unwrap_or(0.0),
        "multiply" => args.a * args.b.unwrap_or(1.0),
        "divide" => args.a / args.b.unwrap_or(1.0),
        "sqrt" => args.a.sqrt(),
        "pow" => args.a.powf(args.b.unwrap_or(2.0)),
        "abs" => args.a.abs(),
        "round" => args.a.round(),
        "floor" => args.a.floor(),
        "ceil" => args.a.ceil(),
        _ => args.a,
    };
    
    let output = Output {
        result,
        operation: args.operation,
    };
    
    println!("{}", serde_json::to_string(&output).unwrap());
}
"#.to_string(),
            dependencies: vec!["serde@1.0[derive]".to_string(), "serde_json@1.0".to_string()],
            example: Some(r#"Use template: template(action: "use", name: "calculator", variables: {"tool_name": "math_calc"})"#.to_string()),
        },

        // Interactive Tool Template (with elicitation)
        Template {
            name: "interactive".to_string(),
            description: "Interactive tool that asks user questions".to_string(),
            category: TemplateCategory::Utility,
            tool_type: TemplateToolType::Python,
            variables: vec![
                TemplateVariable {
                    name: "tool_name".to_string(),
                    description: "Name for the tool".to_string(),
                    default: None,
                    required: true,
                },
                TemplateVariable {
                    name: "prompt_message".to_string(),
                    description: "Message to show when asking for input".to_string(),
                    default: Some("Please provide your input:".to_string()),
                    required: false,
                },
                TemplateVariable {
                    name: "description".to_string(),
                    description: "Tool description".to_string(),
                    default: Some("Interactive tool".to_string()),
                    required: false,
                },
            ],
            code: r#"#!/usr/bin/env python3
"""{{description}}"""
import json
import sys

def main():
    request = json.loads(sys.stdin.readline())
    args = request.get('params', {}).get('arguments', {})
    context = request.get('params', {}).get('context', {})
    capabilities = context.get('capabilities', {})
    
    try:
        # Check if elicitation is supported
        if not capabilities.get('elicitation'):
            response = {
                "jsonrpc": "2.0",
                "error": {"code": -1, "message": "Elicitation not supported by client"},
                "id": request.get("id")
            }
            print(json.dumps(response))
            sys.stdout.flush()
            return
        
        # Request user input via elicitation
        elicit_request = {
            "jsonrpc": "2.0",
            "method": "elicitation/create",
            "params": {
                "message": "{{prompt_message}}",
                "requestedSchema": {
                    "type": "object",
                    "properties": {
                        "input": {
                            "type": "string",
                            "description": "Your input"
                        }
                    },
                    "required": ["input"]
                }
            },
            "id": "elicit_1"
        }
        print(json.dumps(elicit_request))
        sys.stdout.flush()
        
        # Read elicitation response
        elicit_response = json.loads(sys.stdin.readline())
        
        if "error" in elicit_response:
            raise Exception(elicit_response["error"].get("message", "Elicitation failed"))
        
        user_input = elicit_response.get("result", {}).get("content", {}).get("input", "")
        
        # Process the input (customize this part)
        result = {
            "received": user_input,
            "processed": user_input.upper(),  # Example processing
            "length": len(user_input)
        }
        
        response = {"jsonrpc": "2.0", "result": result, "id": request.get("id")}
    except Exception as e:
        response = {"jsonrpc": "2.0", "error": {"code": -1, "message": str(e)}, "id": request.get("id")}
    
    print(json.dumps(response))
    sys.stdout.flush()

if __name__ == "__main__":
    main()
"#.to_string(),
            dependencies: vec![],
            example: Some(r#"Use template: template(action: "use", name: "interactive", variables: {"tool_name": "user_prompt", "prompt_message": "What would you like to do?"})"#.to_string()),
        },

        // Pipeline Template
        Template {
            name: "pipeline_fetch_process".to_string(),
            description: "Pipeline that fetches data and processes it".to_string(),
            category: TemplateCategory::Data,
            tool_type: TemplateToolType::Pipeline,
            variables: vec![
                TemplateVariable {
                    name: "tool_name".to_string(),
                    description: "Name for the pipeline".to_string(),
                    default: None,
                    required: true,
                },
                TemplateVariable {
                    name: "fetch_tool".to_string(),
                    description: "Tool to use for fetching data".to_string(),
                    default: None,
                    required: true,
                },
                TemplateVariable {
                    name: "process_tool".to_string(),
                    description: "Tool to use for processing data".to_string(),
                    default: None,
                    required: true,
                },
                TemplateVariable {
                    name: "description".to_string(),
                    description: "Pipeline description".to_string(),
                    default: Some("Fetch and process pipeline".to_string()),
                    required: false,
                },
            ],
            code: r#"{
  "name": "{{tool_name}}",
  "description": "{{description}}",
  "steps": [
    {
      "name": "fetch",
      "tool": "{{fetch_tool}}",
      "args": "$input"
    },
    {
      "name": "process", 
      "tool": "{{process_tool}}",
      "args": {
        "data": "$fetch"
      }
    }
  ]
}"#.to_string(),
            dependencies: vec![],
            example: Some(r#"Use template: template(action: "use", name: "pipeline_fetch_process", variables: {"tool_name": "fetch_and_analyze", "fetch_tool": "http_client", "process_tool": "data_processor"})"#.to_string()),
        },
    ]
}

/// Template registry - stores templates in memory and on filesystem
#[derive(Clone)]
pub struct TemplateRegistry {
    templates: HashMap<String, Template>,
    templates_dir: PathBuf,
}

impl Default for TemplateRegistry {
    fn default() -> Self {
        Self::new(PathBuf::from("templates"))
    }
}

impl TemplateRegistry {
    pub fn new(templates_dir: PathBuf) -> Self {
        let mut registry = Self {
            templates: HashMap::new(),
            templates_dir,
        };

        // Load builtin templates first
        for t in builtin_templates() {
            registry.templates.insert(t.name.clone(), t);
        }

        // Load custom templates from filesystem (overwrite builtins if same name)
        if let Err(e) = registry.load_from_disk() {
            eprintln!("Warning: Failed to load templates from disk: {}", e);
        }

        registry
    }

    /// Load templates from the templates directory
    fn load_from_disk(&mut self) -> Result<()> {
        if !self.templates_dir.exists() {
            fs::create_dir_all(&self.templates_dir)?;
            return Ok(());
        }

        for entry in fs::read_dir(&self.templates_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().map(|e| e == "json").unwrap_or(false) {
                match self.load_template(&path) {
                    Ok(template) => {
                        eprintln!("Loaded template: {}", template.name);
                        self.templates.insert(template.name.clone(), template);
                    }
                    Err(e) => {
                        eprintln!("Warning: Failed to load template {:?}: {}", path, e);
                    }
                }
            }
        }

        Ok(())
    }

    /// Load a single template from a file
    fn load_template(&self, path: &Path) -> Result<Template> {
        let content = fs::read_to_string(path)?;
        let template: Template = serde_json::from_str(&content)?;
        Ok(template)
    }

    /// Save a template to disk
    pub fn save(&self, template: &Template) -> Result<PathBuf> {
        fs::create_dir_all(&self.templates_dir)?;

        let filename = format!("{}.json", template.name);
        let path = self.templates_dir.join(&filename);

        let content = serde_json::to_string_pretty(template)?;
        fs::write(&path, content)?;

        Ok(path)
    }

    /// Create and save a new template
    pub fn create(&mut self, template: Template) -> Result<()> {
        self.save(&template)?;
        self.templates.insert(template.name.clone(), template);
        Ok(())
    }

    /// Delete a template (only custom ones, not builtins)
    pub fn delete(&mut self, name: &str) -> Result<bool> {
        // Check if it's a builtin
        let builtin_names: Vec<_> = builtin_templates().iter().map(|t| t.name.clone()).collect();
        if builtin_names.contains(&name.to_string()) {
            anyhow::bail!("Cannot delete builtin template '{}'", name);
        }

        // Remove from disk
        let path = self.templates_dir.join(format!("{}.json", name));
        if path.exists() {
            fs::remove_file(&path)?;
        }

        // Remove from memory
        Ok(self.templates.remove(name).is_some())
    }

    /// Reload templates from disk
    #[allow(dead_code)]
    pub fn reload(&mut self) -> Result<()> {
        // Keep builtins
        self.templates.clear();
        for t in builtin_templates() {
            self.templates.insert(t.name.clone(), t);
        }

        // Reload custom
        self.load_from_disk()
    }

    pub fn get(&self, name: &str) -> Option<&Template> {
        self.templates.get(name)
    }

    pub fn list(&self) -> Vec<&Template> {
        self.templates.values().collect()
    }

    pub fn list_by_category(&self, category: &TemplateCategory) -> Vec<&Template> {
        self.templates
            .values()
            .filter(|t| &t.category == category)
            .collect()
    }

    /// Check if a template is builtin or custom
    pub fn is_builtin(&self, name: &str) -> bool {
        builtin_templates().iter().any(|t| t.name == name)
    }

    /// Get the templates directory
    pub fn templates_dir(&self) -> &Path {
        &self.templates_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_render() {
        let template = Template {
            name: "test".to_string(),
            description: "Test template".to_string(),
            category: TemplateCategory::Utility,
            tool_type: TemplateToolType::Python,
            variables: vec![
                TemplateVariable {
                    name: "name".to_string(),
                    description: "Tool name".to_string(),
                    default: None,
                    required: true,
                },
                TemplateVariable {
                    name: "greeting".to_string(),
                    description: "Greeting".to_string(),
                    default: Some("Hello".to_string()),
                    required: false,
                },
            ],
            code: "print('{{greeting}}, {{name}}!')".to_string(),
            dependencies: vec![],
            example: None,
        };

        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "World".to_string());

        let result = template.render(&vars).unwrap();
        assert_eq!(result, "print('Hello, World!')");
    }

    #[test]
    fn test_missing_required_variable() {
        let template = Template {
            name: "test".to_string(),
            description: "Test".to_string(),
            category: TemplateCategory::Utility,
            tool_type: TemplateToolType::Python,
            variables: vec![TemplateVariable {
                name: "required_var".to_string(),
                description: "Required".to_string(),
                default: None,
                required: true,
            }],
            code: "{{required_var}}".to_string(),
            dependencies: vec![],
            example: None,
        };

        let vars = HashMap::new();
        let result = template.render(&vars);
        assert!(result.is_err());
    }

    #[test]
    fn test_builtin_templates() {
        let templates = builtin_templates();
        assert!(!templates.is_empty());

        // Check all templates have required fields
        for t in &templates {
            assert!(!t.name.is_empty());
            assert!(!t.description.is_empty());
            assert!(!t.code.is_empty());
        }
    }

    #[test]
    fn test_template_registry() {
        let temp_dir = std::env::temp_dir().join("skillz_template_test");
        let registry = TemplateRegistry::new(temp_dir);

        // Should have builtin templates
        assert!(!registry.list().is_empty());

        // Should find api_client template
        assert!(registry.get("api_client").is_some());
    }

    #[test]
    fn test_template_create_and_delete() {
        let temp_dir = std::env::temp_dir().join("skillz_template_test_create");
        let _ = std::fs::remove_dir_all(&temp_dir);
        let mut registry = TemplateRegistry::new(temp_dir.clone());

        // Create a custom template
        let template = Template {
            name: "test_custom".to_string(),
            description: "Test custom template".to_string(),
            category: TemplateCategory::Custom,
            tool_type: TemplateToolType::Python,
            variables: vec![TemplateVariable {
                name: "tool_name".to_string(),
                description: "Tool name".to_string(),
                default: None,
                required: true,
            }],
            code: "print('{{tool_name}}')".to_string(),
            dependencies: vec![],
            example: None,
        };

        registry
            .create(template)
            .expect("Failed to create template");

        // Should exist now
        assert!(registry.get("test_custom").is_some());

        // Should be on disk
        assert!(temp_dir.join("test_custom.json").exists());

        // Should not be builtin
        assert!(!registry.is_builtin("test_custom"));

        // Delete it
        registry.delete("test_custom").expect("Failed to delete");
        assert!(registry.get("test_custom").is_none());

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
