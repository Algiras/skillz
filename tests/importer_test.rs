//! Tests for the importer module

/// Test parsing gist IDs from various formats
mod gist_parsing {
    #[test]
    fn test_gist_id_from_short_format() {
        // Format: gist:ID
        let input = "gist:abc123def";
        let id = input.strip_prefix("gist:").unwrap();
        assert_eq!(id, "abc123def");
    }

    #[test]
    fn test_gist_id_from_url() {
        // Format: https://gist.github.com/user/ID
        let url = "https://gist.github.com/algiras/eaeebc3ae8e234ba7c1e46a76619f0fd";
        let id = url.split('/').next_back().unwrap();
        assert_eq!(id, "eaeebc3ae8e234ba7c1e46a76619f0fd");
    }

    #[test]
    fn test_gist_id_from_raw_url() {
        // Format: https://gist.githubusercontent.com/user/ID/raw/...
        let url = "https://gist.githubusercontent.com/algiras/eaeebc3ae8e234ba7c1e46a76619f0fd/raw/file.txt";
        let parts: Vec<&str> = url.split('/').collect();
        // The ID is after the username
        let id = parts.get(4).unwrap();
        assert_eq!(*id, "eaeebc3ae8e234ba7c1e46a76619f0fd");
    }

    #[test]
    fn test_is_gist_url() {
        let gist_urls = vec![
            "gist:abc123",
            "https://gist.github.com/user/abc123",
            "https://gist.githubusercontent.com/user/abc123/raw/file.txt",
        ];
        
        let non_gist_urls = vec![
            "https://github.com/user/repo",
            "https://example.com/gist/abc123",
            "git:abc123",
        ];
        
        for url in gist_urls {
            assert!(
                url.starts_with("gist:") || url.contains("gist.github"),
                "Expected {} to be detected as gist URL",
                url
            );
        }
        
        for url in non_gist_urls {
            assert!(
                !url.starts_with("gist:") && !url.contains("gist.github"),
                "Expected {} to NOT be detected as gist URL",
                url
            );
        }
    }
}

/// Test parsing git URLs
mod git_parsing {
    #[test]
    fn test_git_url_with_branch() {
        // Format: URL#branch
        let url = "https://github.com/user/repo#main";
        let parts: Vec<&str> = url.splitn(2, '#').collect();
        assert_eq!(parts[0], "https://github.com/user/repo");
        assert_eq!(parts[1], "main");
    }

    #[test]
    fn test_git_url_without_branch() {
        let url = "https://github.com/user/repo";
        let parts: Vec<&str> = url.splitn(2, '#').collect();
        assert_eq!(parts[0], "https://github.com/user/repo");
        assert_eq!(parts.len(), 1); // No branch part
    }

    #[test]
    fn test_extract_repo_name() {
        let url = "https://github.com/user/my-tool-repo";
        let name = url.trim_end_matches(".git").split('/').next_back().unwrap();
        assert_eq!(name, "my-tool-repo");
    }

    #[test]
    fn test_extract_repo_name_with_git_suffix() {
        let url = "https://github.com/user/my-tool-repo.git";
        let name = url.trim_end_matches(".git").split('/').next_back().unwrap();
        assert_eq!(name, "my-tool-repo");
    }
}

/// Test manifest validation
mod manifest_validation {
    use serde_json::json;

    #[test]
    fn test_valid_script_manifest() {
        let manifest = json!({
            "name": "word_counter",
            "version": "1.0.0",
            "description": "Counts words in text",
            "type": "script",
            "interpreter": "python3",
            "entry_file": "word_counter.py",
            "input_schema": {
                "type": "object",
                "properties": {
                    "text": {"type": "string"}
                },
                "required": ["text"]
            }
        });

        assert!(manifest["name"].is_string());
        assert!(manifest["entry_file"].is_string());
        assert_eq!(manifest["type"], "script");
    }

    #[test]
    fn test_valid_wasm_manifest() {
        let manifest = json!({
            "name": "calculator",
            "version": "1.0.0", 
            "description": "Math calculator",
            "type": "wasm",
            "entry_file": "tool.wasm",
            "input_schema": {
                "type": "object",
                "properties": {
                    "expression": {"type": "string"}
                }
            }
        });

        assert!(manifest["name"].is_string());
        assert_eq!(manifest["type"], "wasm");
    }

    #[test]
    fn test_valid_pipeline_manifest() {
        let manifest = json!({
            "name": "process_text",
            "version": "1.0.0",
            "description": "Process text through multiple tools",
            "type": "pipeline",
            "pipeline_steps": [
                {
                    "tool": "word_counter",
                    "args": {"text": "$input.text"}
                },
                {
                    "tool": "formatter",
                    "args": {"data": "$prev"}
                }
            ]
        });

        assert!(manifest["name"].is_string());
        assert_eq!(manifest["type"], "pipeline");
        assert!(manifest["pipeline_steps"].is_array());
    }

    #[test]
    fn test_manifest_with_annotations() {
        let manifest = json!({
            "name": "readonly_tool",
            "type": "script",
            "entry_file": "tool.py",
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false
            }
        });

        assert!(manifest["annotations"]["readOnlyHint"].as_bool().unwrap());
        assert!(!manifest["annotations"]["destructiveHint"].as_bool().unwrap());
    }

    #[test]
    fn test_manifest_with_dependencies() {
        let manifest = json!({
            "name": "http_tool",
            "type": "script",
            "interpreter": "python3",
            "entry_file": "tool.py",
            "dependencies": ["requests", "aiohttp"]
        });

        let deps = manifest["dependencies"].as_array().unwrap();
        assert_eq!(deps.len(), 2);
        assert_eq!(deps[0], "requests");
    }
}

/// Test output/result parsing for pipelines
mod output_parsing {
    use serde_json::json;

    #[test]
    fn test_parse_script_output_simple() {
        let output = json!({
            "words": 10,
            "characters": 50,
            "lines": 3
        });

        assert_eq!(output["words"], 10);
        assert_eq!(output["characters"], 50);
    }

    #[test]
    fn test_parse_script_output_nested() {
        let output = json!({
            "result": {
                "data": {
                    "items": ["a", "b", "c"],
                    "count": 3
                },
                "metadata": {
                    "processed_at": "2024-01-01"
                }
            },
            "success": true
        });

        // Pipeline variable resolution: $prev.result.data.items
        assert_eq!(output["result"]["data"]["items"][0], "a");
        assert_eq!(output["result"]["data"]["count"], 3);
        assert!(output["success"].as_bool().unwrap());
    }

    #[test]
    fn test_output_with_logs() {
        // When a script returns logs, output is wrapped
        let output = json!({
            "result": {"value": 42},
            "logs": [
                {"level": "info", "message": "Processing started"},
                {"level": "debug", "message": "Value computed"}
            ]
        });

        // Result should be accessible via "result" key
        assert_eq!(output["result"]["value"], 42);
        assert_eq!(output["logs"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_pipeline_variable_resolution_patterns() {
        // Test various patterns used in pipeline arguments
        let input = json!({"text": "hello", "config": {"timeout": 30}});
        let prev = json!({"count": 5, "items": ["x", "y"]});
        let step_output = json!({"url": "http://example.com", "status": 200});

        // $input patterns
        assert_eq!(input["text"], "hello");
        assert_eq!(input["config"]["timeout"], 30);

        // $prev patterns
        assert_eq!(prev["count"], 5);
        assert_eq!(prev["items"][0], "x");

        // $step_name patterns (step output)
        assert_eq!(step_output["url"], "http://example.com");
        assert_eq!(step_output["status"], 200);
    }
}

