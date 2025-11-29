#!/usr/bin/env python3
"""Test script for Skillz MCP features: logging, progress, elicitation, sampling"""

import sys
import json

def send_request(method, params=None):
    """Send a JSON-RPC request to the host"""
    request = {
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params or {}
    }
    print(json.dumps(request), flush=True)
    response = json.loads(sys.stdin.readline())
    return response.get("result")

def send_notification(method, params=None):
    """Send a JSON-RPC notification (no response expected)"""
    notification = {
        "jsonrpc": "2.0",
        "method": method,
        "params": params or {}
    }
    print(json.dumps(notification), flush=True)

def main():
    # Read the incoming request
    request = json.loads(sys.stdin.readline())
    args = request.get("params", {}).get("arguments", {})
    test_type = args.get("test", "all")
    
    results = []
    
    # Test logging
    if test_type in ["all", "logging"]:
        send_notification("logging/message", {
            "level": "info",
            "message": "Testing logging from script"
        })
        send_notification("logging/message", {
            "level": "warning",
            "message": "This is a warning log"
        })
        results.append("✅ Sent log messages")
    
    # Test progress
    if test_type in ["all", "progress"]:
        for i in range(5):
            send_notification("progress/update", {
                "current": i + 1,
                "total": 5,
                "message": f"Step {i + 1} of 5"
            })
        results.append("✅ Sent progress updates")
    
    # Test elicitation
    if test_type in ["all", "elicitation"]:
        result = send_request("elicitation/create", {
            "message": "Please enter your name:",
            "schema": {
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Your name"
                    }
                },
                "required": ["name"]
            }
        })
        if result:
            if result.get("action") == "accept":
                results.append(f"✅ Elicitation accepted: {result.get('content')}")
            elif result.get("action") == "decline":
                results.append("⚠️ User declined elicitation")
            elif result.get("action") == "cancel":
                results.append("⚠️ User cancelled elicitation")
            else:
                results.append(f"❌ Elicitation error: {result}")
        else:
            results.append("❌ No elicitation response")
    
    # Test sampling
    if test_type in ["all", "sampling"]:
        result = send_request("sampling/createMessage", {
            "messages": [
                {"role": "user", "content": {"type": "text", "text": "Say hello in one word"}}
            ],
            "maxTokens": 10
        })
        if result:
            if "error" in result:
                results.append(f"❌ Sampling error: {result.get('error')}")
            else:
                results.append(f"✅ Sampling result: {result}")
        else:
            results.append("❌ No sampling response")
    
    # Test memory
    if test_type in ["all", "memory"]:
        # Set a value
        send_request("memory/set", {"key": "test_key", "value": "test_value"})
        
        # Get it back
        get_result = send_request("memory/get", {"key": "test_key"})
        if get_result and get_result.get("value") == "test_value":
            results.append("✅ Memory set/get works")
        else:
            results.append(f"❌ Memory error: {get_result}")
        
        # List keys
        list_result = send_request("memory/list", {})
        results.append(f"✅ Memory keys: {list_result}")
    
    # Send final response
    response = {
        "jsonrpc": "2.0",
        "id": request.get("id"),
        "result": {
            "content": [{"type": "text", "text": "\n".join(results)}]
        }
    }
    print(json.dumps(response), flush=True)

if __name__ == "__main__":
    main()

