# 🤖 Skillz LLM & Advanced Usage Guide

This guide contains technical details for Large Language Models (LLMs) and advanced users building custom tools for Skillz.

## 📝 Writing Script Tools

### JSON-RPC 2.0 Protocol

Scripts communicate via JSON-RPC 2.0 over stdin/stdout:

<div align="center">

### 🏗️ Architecture Overview

<img src="architecture.png" alt="Skillz Architecture Diagram" width="800">

*The diagram shows how scripts communicate with the Skillz host using JSON-RPC 2.0 protocol over stdin/stdout*

</div>

### ⚠️ Important: Use `readline()` not `read()`

> **⚠️ WARNING**  
> Always use `readline()` to read JSON-RPC requests. Using `read()` will block waiting for EOF and cause timeouts!

```python
# ✅ CORRECT - Returns immediately after reading the request
request = json.loads(sys.stdin.readline())

# ❌ WRONG - Blocks waiting for EOF, causes timeout!
request = json.loads(sys.stdin.read())
```

### 🐍 Python Template

```python
#!/usr/bin/env python3
import json
import sys

def main():
    # Read JSON-RPC request (use readline!)
    request = json.loads(sys.stdin.readline())
    
    # Extract arguments
    args = request.get('params', {}).get('arguments', {})
    context = request.get('params', {}).get('context', {})
    
    # Your logic here
    result = {"message": "Hello!", "input": args}
    
    # Return JSON-RPC response
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

### 📗 Node.js Template

```javascript
#!/usr/bin/env node
const readline = require('readline');

const rl = readline.createInterface({
  input: process.stdin,
  output: process.stdout,
  terminal: false
});

rl.on('line', (line) => {
  const request = JSON.parse(line);
  const args = request.params?.arguments || {};
  
  // Your logic here
  const result = { message: "Hello from Node!", input: args };
  
  console.log(JSON.stringify({
    jsonrpc: "2.0",
    result: result,
    id: request.id
  }));
  
  process.exit(0);
});
```

### Execution Context

Scripts receive context information:

```json
{
  "jsonrpc": "2.0",
  "method": "execute",
  "params": {
    "arguments": { "your": "args" },
    "context": {
      "roots": ["/path/to/workspace"],
      "working_directory": "/current/dir",
      "tool_name": "my_tool",
      "tools_dir": "~/.skillz/tools",
      "capabilities": {
        "sampling": false,
        "elicitation": false
      }
    }
  },
  "id": 1
}
```

## 🏷️ Tool Annotations

Annotations help clients understand tool behavior:

```json
{
  "title": "File Reader",
  "readOnlyHint": true,
  "destructiveHint": false,
  "idempotentHint": true,
  "openWorldHint": false
}
```

| Annotation | Meaning |
|------------|---------|
| `title` | Human-readable display name |
| `readOnlyHint` | Tool only reads, doesn't modify state |
| `destructiveHint` | Tool may delete or overwrite data |
| `idempotentHint` | Safe to retry with same arguments |
| `openWorldHint` | Interacts with external systems (network, APIs) |

## 📂 Tool Directory Structure

Each tool is stored in its own directory with a shareable `manifest.json`:

<div align="center">

```tree
~/tools/
├── fibonacci/
│   ├── manifest.json     # Tool metadata
│   ├── fibonacci.wasm    # Compiled binary
│   └── src.rs            # Source code (for recompilation)
├── word_counter/
│   ├── manifest.json
│   └── word_counter.py
└── http_client/
    ├── manifest.json
    ├── http_client.py
    └── env/              # Virtual environment
```

💡 **Tools are fully shareable** - just copy the directory!

</div>

### manifest.json Example

```json
{
  "name": "json_processor",
  "version": "1.0.0",
  "description": "Process JSON with serde",
  "tool_type": "wasm",
  "wasm_dependencies": ["serde@1.0[derive]", "serde_json@1.0"],
  "author": "Your Name",
  "license": "MIT",
  "tags": ["json", "utility"]
}
```

## 💡 Advanced Examples

### 🦀 WASM Tool with Rust Dependencies

```rust
build_tool(
  name: "json_processor",
  description: "Process JSON with serde",
  dependencies: ["serde@1.0[derive]", "serde_json@1.0"],
  code: """
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
struct Data { name: String, value: i32 }

fn main() {
    let data = Data { name: "test".to_string(), value: 42 };
    println!("{}", serde_json::to_string(&data).unwrap());
}
"""
)
```

### 🐍 Register a Script Tool (Python)

```python
register_script(
  name: "word_counter",
  description: "Counts words in text",
  interpreter: "python3",
  input_schema: {
    "type": "object",
    "properties": {"text": {"type": "string"}},
    "required": ["text"]
  },
  output_schema: {
    "type": "object",
    "properties": {"count": {"type": "integer"}}
  },
  annotations: {"readOnlyHint": true},
  code: """#!/usr/bin/env python3
import json, sys

request = json.loads(sys.stdin.readline())
text = request['params']['arguments'].get('text', '')
result = {'count': len(text.split())}
print(json.dumps({'jsonrpc': '2.0', 'result': result, 'id': request['id']}))
"""
)
```

### 📦 Tool with Dependencies

```python
register_script(
  name: "data_analyzer",
  interpreter: "python3",
  dependencies: ["pandas", "numpy"],
  code: "..."
)
```

### 🌐 Import Tools from GitHub

```bash
# Import from a git repository
import_tool(
  source: "https://github.com/user/skillz-json-tools"
)

# Import from a specific branch
import_tool(
  source: "https://github.com/user/repo#main"
)

# Import from a GitHub Gist
import_tool(
  source: "gist:abc123def456"
)

# Or with full gist URL
import_tool(
  source: "https://gist.github.com/user/abc123def456"
)
```

### ⛓️ Create a Pipeline (Chain Tools)

```yaml
# Pipelines are tools that chain other tools together!
create_pipeline(
  name: "process_data",
  description: "Fetch, transform, and format data",
  steps: [
    { name: "fetch", tool: "http_get", args: { url: "$input.url" } },
    { tool: "json_parse", args: { text: "$fetch.body" } },
    { tool: "format_report", args: { data: "$prev" } }
  ]
)

# Run the pipeline like any other tool
call_tool(tool_name: "process_data", arguments: { url: "https://api.example.com/data" })
```

<details>
<summary><b>📘 Variable syntax in pipelines</b></summary>

- `$input.field` - Access pipeline input
- `$prev` - Previous step's entire output
- `$prev.field` - Access field from previous step
- `$step_name.field` - Access field from a named step

</details>

### ⚡ Execute Multiple Tools via Code

```
execute_code(
  language: "python",
  code: """
# All registered tools are available as functions!
fib = fibonacci()
primes = prime_finder()

# Combine results with custom logic
print(f"Fibonacci: {fib}")
print(f"Primes: {primes}")

# Loops, conditionals, anything you need
for i in range(3):
    result = my_calculator(a=i, b=10)
    print(result)
"""
)
```
