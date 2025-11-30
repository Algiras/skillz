import subprocess
import json
import sys
import os
import time

def run_test():
    print("Starting MCP Server...")
    # Path to the release binary
    server_path = "./target/release/skillz"
    
    process = subprocess.Popen(
        [server_path],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=sys.stderr,
        text=True,
        bufsize=0
    )

    def send_request(method, params=None, req_id=1):
        req = {
            "jsonrpc": "2.0",
            "method": method,
            "id": req_id
        }
        if params:
            req["params"] = params
        
        json_req = json.dumps(req)
        process.stdin.write(json_req + "\n")
        process.stdin.flush()
        
        # Read response
        while True:
            line = process.stdout.readline()
            print(f"RAW: {line!r}", flush=True)
            if not line:
                break
            try:
                resp = json.loads(line)
                if "method" in resp: # Notification or Request from server
                    # For now, we just print notifications and ignore them
                    # If it's a request (has id), we should handle it, but for this simple test we might fail if we don't expect it
                    if "id" in resp:
                        print(f"Received request from server: {resp}", flush=True)
                        # If we get a request while waiting for a response, it's likely elicitation/sampling
                        # We are not handling it in this simple loop, so we might hang or fail
                        # But we are only testing 'memory' which is handled by host
                        pass
                    continue
                return resp
            except json.JSONDecodeError:
                print(f"Failed to parse JSON: {line!r}", flush=True)
                continue

    # 1. Initialize
    print("[TEST 1] Initializing...")
    init_resp = send_request("initialize", {
        "protocolVersion": "2024-11-05",
        "capabilities": {
            "sampling": {},
            "elicitation": {},
            "roots": {"listChanged": True}
        },
        "clientInfo": {"name": "test-client", "version": "1.0"}
    }, 1)
    assert "result" in init_resp
    send_request("notifications/initialized", None, 2)

    # 2. List Tools
    print("[TEST 2] Listing tools...")
    list_resp = send_request("tools/list", None, 3)
    tools = list_resp["result"]["tools"]
    tool_names = [t["name"] for t in tools]
    print(f"Found tools: {tool_names}")
    assert "build_tool" in tool_names
    assert "register_script" in tool_names

    # 3. Build a WASM Tool
    print("[TEST 3] Building 'hello_world' tool...")
    code = r"""
    fn main() {
        println!("Hello from dynamically built WASM!");
    }
    """
    build_resp = send_request("tools/call", {
        "name": "build_tool",
        "arguments": {
            "name": "hello_world",
            "code": code,
            "description": "A simple hello world tool"
        }
    }, 4)
    
    if "error" in build_resp:
        print(f"Build failed: {build_resp['error']}")
        sys.exit(1)
        
    print("Build response:", build_resp)

    # 4. Call the new WASM tool
    print("[TEST 4] Calling 'hello_world' tool...")
    call_resp = send_request("tools/call", {
        "name": "call_tool",
        "arguments": {
            "tool_name": "hello_world",
            "arguments": {}
        }
    }, 5)
    
    print("Call result:", call_resp)
    content = call_resp["result"]["content"][0]["text"]
    assert "Hello from dynamically built WASM!" in content

    # 5. Register Script Tool (test_features.py)
    print("[TEST 5] Registering 'test_features' script tool...")
    with open("test_features.py", "r") as f:
        script_code = f.read()
        
    reg_resp = send_request("tools/call", {
        "name": "register_script",
        "arguments": {
            "name": "test_features",
            "code": script_code,
            "interpreter": "python3",
            "description": "Test features script"
        }
    }, 6)
    
    if "error" in reg_resp:
        print(f"Registration failed: {reg_resp['error']}")
        sys.exit(1)
    print("Registration response:", reg_resp)

    # 6. Call Script Tool to test features (Memory)
    print("[TEST 6] Calling 'test_features' tool (memory test)...")
    feat_resp = send_request("tools/call", {
        "name": "call_tool",
        "arguments": {
            "tool_name": "test_features",
            "arguments": {"test": "memory"}
        }
    }, 7)
    
    print("Feature test result:", feat_resp)
    if "error" in feat_resp:
        print(f"Feature test failed: {feat_resp['error']}")
        sys.exit(1)
        
    content = feat_resp["result"]["content"][0]["text"]
    assert "✅ Memory set/get works" in content

    # 7. Test Resources (Server Feature)
    print("[TEST 7] Testing Resources...", flush=True)
    res_list = send_request("resources/list", {}, 8)
    print("Resources:", res_list, flush=True)
    assert "result" in res_list
    assert "resources" in res_list["result"]
    
    # 8. Test Prompts (Server Feature)
    print("[TEST 8] Testing Prompts...", flush=True)
    prompts_list = send_request("prompts/list", {}, 9)
    print("Prompts:", prompts_list, flush=True)
    assert "result" in prompts_list

    # 9. Test Pipeline
    print("[TEST 9] Testing Pipeline...", flush=True)
    # Create a pipeline that calls hello_world
    pipeline_resp = send_request("tools/call", {
        "name": "pipeline",
        "arguments": {
            "action": "create",
            "name": "test_pipeline",
            "description": "A test pipeline",
            "steps": [
                {"tool": "hello_world", "args": {}}
            ]
        }
    }, 10)
    print("Pipeline creation:", pipeline_resp, flush=True)
    assert "result" in pipeline_resp

    # Call the pipeline
    pipe_call_resp = send_request("tools/call", {
        "name": "call_tool",
        "arguments": {
            "tool_name": "test_pipeline",
            "arguments": {}
        }
    }, 11)
    print("Pipeline call result:", pipe_call_resp, flush=True)
    content = pipe_call_resp["result"]["content"][0]["text"]
    assert "Hello from dynamically built WASM!" in content

    # 10. Test Execute Code
    print("[TEST 10] Testing Execute Code...", flush=True)
    exec_resp = send_request("tools/call", {
        "name": "execute_code",
        "arguments": {
            "language": "python",
            "code": "print('Hello from execute_code')"
        }
    }, 12)
    print("Execute code result:", exec_resp, flush=True)
    content = exec_resp["result"]["content"][0]["text"]
    assert "Hello from execute_code" in content
    
    print("\n============================================================", flush=True)
    print("All tests completed successfully!", flush=True)
    print("============================================================", flush=True)

    process.terminate()

if __name__ == "__main__":
    run_test()
