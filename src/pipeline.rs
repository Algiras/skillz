//! Pipeline execution - execute pipeline tools

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Result of a single step execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub step_index: usize,
    pub step_name: Option<String>,
    pub tool: String,
    pub success: bool,
    pub output: serde_json::Value,
    pub error: Option<String>,
    pub duration_ms: u64,
}

/// Pipeline executor - resolves variables and evaluates conditions
pub struct PipelineExecutor;

impl PipelineExecutor {
    /// Resolve variable references in arguments
    /// Supports: $input.field, $prev.field, $step_name.field, $prev (whole output)
    pub fn resolve_args(
        args: &serde_json::Value,
        input: &serde_json::Value,
        step_results: &HashMap<String, serde_json::Value>,
        prev_output: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value> {
        match args {
            serde_json::Value::String(s) => {
                // Check for variable reference
                if s.starts_with('$') {
                    Self::resolve_variable(s, input, step_results, prev_output)
                } else {
                    Ok(serde_json::Value::String(s.clone()))
                }
            }
            serde_json::Value::Object(obj) => {
                let mut resolved = serde_json::Map::new();
                for (key, value) in obj {
                    resolved.insert(
                        key.clone(),
                        Self::resolve_args(value, input, step_results, prev_output)?,
                    );
                }
                Ok(serde_json::Value::Object(resolved))
            }
            serde_json::Value::Array(arr) => {
                let resolved: Result<Vec<_>> = arr
                    .iter()
                    .map(|v| Self::resolve_args(v, input, step_results, prev_output))
                    .collect();
                Ok(serde_json::Value::Array(resolved?))
            }
            other => Ok(other.clone()),
        }
    }

    fn resolve_variable(
        var: &str,
        input: &serde_json::Value,
        step_results: &HashMap<String, serde_json::Value>,
        prev_output: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let var = var.trim_start_matches('$');

        // Handle $prev (whole previous output)
        if var == "prev" {
            return prev_output
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("No previous step output available"));
        }

        // Parse path: source.field.subfield...
        let parts: Vec<&str> = var.split('.').collect();
        if parts.is_empty() {
            anyhow::bail!("Invalid variable reference: ${}", var);
        }

        let source = parts[0];
        let path = &parts[1..];

        // Get the source value
        let source_value = match source {
            "input" => input,
            "prev" => prev_output
                .ok_or_else(|| anyhow::anyhow!("No previous step output available"))?,
            step_name => step_results
                .get(step_name)
                .ok_or_else(|| anyhow::anyhow!("Step '{}' not found or not yet executed", step_name))?,
        };

        // Navigate the path
        let mut current = source_value;
        for part in path {
            current = current
                .get(part)
                .ok_or_else(|| anyhow::anyhow!("Field '{}' not found in {}", part, source))?;
        }

        Ok(current.clone())
    }

    /// Evaluate a simple condition
    /// Supports: $var == value, $var != value, $var (truthy check)
    pub fn evaluate_condition(
        condition: &str,
        input: &serde_json::Value,
        step_results: &HashMap<String, serde_json::Value>,
        prev_output: Option<&serde_json::Value>,
    ) -> Result<bool> {
        let condition = condition.trim();

        // Check for equality/inequality
        if condition.contains("==") {
            let parts: Vec<&str> = condition.split("==").map(|s| s.trim()).collect();
            if parts.len() != 2 {
                anyhow::bail!("Invalid condition: {}", condition);
            }

            let left = Self::resolve_condition_value(parts[0], input, step_results, prev_output)?;
            let right = Self::resolve_condition_value(parts[1], input, step_results, prev_output)?;

            return Ok(left == right);
        }

        if condition.contains("!=") {
            let parts: Vec<&str> = condition.split("!=").map(|s| s.trim()).collect();
            if parts.len() != 2 {
                anyhow::bail!("Invalid condition: {}", condition);
            }

            let left = Self::resolve_condition_value(parts[0], input, step_results, prev_output)?;
            let right = Self::resolve_condition_value(parts[1], input, step_results, prev_output)?;

            return Ok(left != right);
        }

        // Truthy check
        let value = Self::resolve_condition_value(condition, input, step_results, prev_output)?;
        Ok(match value {
            serde_json::Value::Bool(b) => b,
            serde_json::Value::Null => false,
            serde_json::Value::String(s) => !s.is_empty(),
            serde_json::Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
            serde_json::Value::Array(a) => !a.is_empty(),
            serde_json::Value::Object(o) => !o.is_empty(),
        })
    }

    fn resolve_condition_value(
        value: &str,
        input: &serde_json::Value,
        step_results: &HashMap<String, serde_json::Value>,
        prev_output: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let value = value.trim();

        // Variable reference
        if value.starts_with('$') {
            return Self::resolve_variable(value, input, step_results, prev_output);
        }

        // Boolean literals
        if value == "true" {
            return Ok(serde_json::Value::Bool(true));
        }
        if value == "false" {
            return Ok(serde_json::Value::Bool(false));
        }

        // Null
        if value == "null" {
            return Ok(serde_json::Value::Null);
        }

        // Number
        if let Ok(n) = value.parse::<i64>() {
            return Ok(serde_json::Value::Number(n.into()));
        }
        if let Ok(n) = value.parse::<f64>() {
            return Ok(serde_json::json!(n));
        }

        // String (with or without quotes)
        let s = value.trim_matches('"').trim_matches('\'');
        Ok(serde_json::Value::String(s.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_input_variable() {
        let input = serde_json::json!({"name": "test", "count": 42});
        let step_results = HashMap::new();

        let resolved = PipelineExecutor::resolve_variable("input.name", &input, &step_results, None).unwrap();
        assert_eq!(resolved, serde_json::json!("test"));
    }

    #[test]
    fn test_resolve_prev_variable() {
        let input = serde_json::json!({});
        let step_results = HashMap::new();
        let prev = serde_json::json!({"result": "success", "data": [1, 2, 3]});

        let resolved = PipelineExecutor::resolve_variable("prev.result", &input, &step_results, Some(&prev)).unwrap();
        assert_eq!(resolved, serde_json::json!("success"));
    }

    #[test]
    fn test_resolve_step_variable() {
        let input = serde_json::json!({});
        let mut step_results = HashMap::new();
        step_results.insert("fetch".to_string(), serde_json::json!({"url": "http://example.com"}));

        let resolved = PipelineExecutor::resolve_variable("fetch.url", &input, &step_results, None).unwrap();
        assert_eq!(resolved, serde_json::json!("http://example.com"));
    }

    #[test]
    fn test_resolve_args_object() {
        let input = serde_json::json!({"text": "hello world"});
        let step_results = HashMap::new();

        let args = serde_json::json!({
            "content": "$input.text",
            "prefix": ">>> "
        });

        let resolved = PipelineExecutor::resolve_args(&args, &input, &step_results, None).unwrap();
        assert_eq!(resolved, serde_json::json!({
            "content": "hello world",
            "prefix": ">>> "
        }));
    }

    #[test]
    fn test_evaluate_condition_equality() {
        let input = serde_json::json!({});
        let step_results = HashMap::new();
        let prev = serde_json::json!({"success": true});

        let result = PipelineExecutor::evaluate_condition("$prev.success == true", &input, &step_results, Some(&prev)).unwrap();
        assert!(result);
    }

    #[test]
    fn test_evaluate_condition_truthy() {
        let input = serde_json::json!({});
        let step_results = HashMap::new();
        let prev = serde_json::json!({"data": [1, 2, 3]});

        let result = PipelineExecutor::evaluate_condition("$prev.data", &input, &step_results, Some(&prev)).unwrap();
        assert!(result);
    }

    #[test]
    fn test_resolve_deeply_nested_variable() {
        let input = serde_json::json!({});
        let mut step_results = HashMap::new();
        step_results.insert("api".to_string(), serde_json::json!({
            "response": {
                "data": {
                    "users": [
                        {"name": "Alice", "email": "alice@example.com"},
                        {"name": "Bob", "email": "bob@example.com"}
                    ]
                }
            }
        }));

        // Test deep nesting like $api.response.data.users
        let resolved = PipelineExecutor::resolve_variable("api.response.data", &input, &step_results, None).unwrap();
        assert!(resolved.get("users").is_some());
    }

    #[test]
    fn test_resolve_prev_whole_output() {
        let input = serde_json::json!({});
        let step_results = HashMap::new();
        let prev = serde_json::json!({"count": 5, "items": ["a", "b", "c"]});

        // Test $prev without field access (whole output)
        let resolved = PipelineExecutor::resolve_variable("prev", &input, &step_results, Some(&prev)).unwrap();
        assert_eq!(resolved["count"], 5);
        assert_eq!(resolved["items"][0], "a");
    }

    #[test]
    fn test_resolve_args_with_nested_variables() {
        let input = serde_json::json!({"config": {"timeout": 30}});
        let mut step_results = HashMap::new();
        step_results.insert("fetch".to_string(), serde_json::json!({
            "body": {"message": "Hello"},
            "status": 200
        }));

        let args = serde_json::json!({
            "data": "$fetch.body",
            "timeout": "$input.config.timeout",
            "static_value": "unchanged"
        });

        let resolved = PipelineExecutor::resolve_args(&args, &input, &step_results, None).unwrap();
        assert_eq!(resolved["data"]["message"], "Hello");
        assert_eq!(resolved["timeout"], 30);
        assert_eq!(resolved["static_value"], "unchanged");
    }

    #[test]
    fn test_resolve_args_array_with_variables() {
        let input = serde_json::json!({"items": ["x", "y", "z"]});
        let step_results = HashMap::new();

        let args = serde_json::json!({
            "list": ["$input.items", "static"],
            "nested": [{"val": "$input.items"}]
        });

        let resolved = PipelineExecutor::resolve_args(&args, &input, &step_results, None).unwrap();
        // $input.items resolves to the array
        assert_eq!(resolved["list"][0], serde_json::json!(["x", "y", "z"]));
        assert_eq!(resolved["list"][1], "static");
    }

    #[test]
    fn test_evaluate_condition_inequality() {
        let input = serde_json::json!({});
        let step_results = HashMap::new();
        let prev = serde_json::json!({"status": 404});

        let result = PipelineExecutor::evaluate_condition("$prev.status != 200", &input, &step_results, Some(&prev)).unwrap();
        assert!(result);
    }

    #[test]
    fn test_evaluate_condition_string_equality() {
        let input = serde_json::json!({});
        let step_results = HashMap::new();
        let prev = serde_json::json!({"status": "success"});

        let result = PipelineExecutor::evaluate_condition("$prev.status == success", &input, &step_results, Some(&prev)).unwrap();
        assert!(result);
    }
}
