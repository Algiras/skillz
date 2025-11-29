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
            if not line:
                break
            try:
                resp = json.loads(line)
                if "method" in resp: # Notification
                    continue
                return resp
            except json.JSONDecodeError:
                continue

    # 1. Initialize
    print("[TEST 1] Initializing...")
    init_resp = send_request("initialize", {
        "protocolVersion": "2024-11-05",
        "capabilities": {},
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
    assert "call_tool" in tool_names # Note: call_tool handles both wasm and script

    # 3. Build a Tool
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

    # 4. Call the new tool
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

    print("\n============================================================")
    print("All tests completed successfully!")
    print("============================================================")

    process.terminate()

if __name__ == "__main__":
    run_test()
