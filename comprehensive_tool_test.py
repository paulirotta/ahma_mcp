#!/usr/bin/env python3
"""Comprehensive test of ahma_mcp tools functionality"""

import json
import subprocess
import sys
import time
import threading
import tempfile
import os
from pathlib import Path

def test_tool_execution(tool_name, args=None):
    """Test executing a specific tool through the MCP server"""
    
    # Start the server process
    # Resolve project root dynamically: prefer CARGO_MANIFEST_DIR, else script parent
    project_root = os.environ.get("CARGO_MANIFEST_DIR") or str(Path(__file__).resolve().parent)
    server_bin = str(Path(project_root) / "target" / "release" / "ahma_mcp")
    tools_dir = str(Path(project_root) / "tools")

    proc = subprocess.Popen(
        [server_bin, '--tools-dir', tools_dir],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=1
    )
    
    def read_stderr():
        """Read stderr in background"""
        for line in iter(proc.stderr.readline, ''):
            if "INFO" not in line:  # Reduce log noise
                print(f"SERVER: {line.strip()}", file=sys.stderr)
    
    stderr_thread = threading.Thread(target=read_stderr, daemon=True)
    stderr_thread.start()
    
    try:
        # Give server time to start
        time.sleep(0.5)
        
        # Initialize MCP connection
        init_request = {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {"roots": {"listChanged": False}, "sampling": {}},
                "clientInfo": {"name": "test_client", "version": "0.1.0"}
            }
        }
        
        proc.stdin.write(json.dumps(init_request) + "\n")
        proc.stdin.flush()
        
        # Read init response
        response = proc.stdout.readline()
        if not response.strip():
            return False, "No init response"
        
        # Send initialized notification
        init_notification = {"jsonrpc": "2.0", "method": "notifications/initialized"}
        proc.stdin.write(json.dumps(init_notification) + "\n")
        proc.stdin.flush()
        time.sleep(0.1)
        
        # Execute the tool
        tool_request = {
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": args or {}
            }
        }
        
        print(f"🧪 Testing {tool_name} with args: {args}")
        proc.stdin.write(json.dumps(tool_request) + "\n")
        proc.stdin.flush()
        
        # Read tool response
        tool_response = proc.stdout.readline()
        if tool_response.strip():
            try:
                result = json.loads(tool_response.strip())
                if "result" in result:
                    return True, result["result"]
                elif "error" in result:
                    return False, result["error"]
                else:
                    return False, f"Unexpected response: {result}"
            except json.JSONDecodeError as e:
                return False, f"JSON decode error: {e}"
        else:
            return False, "No tool response"
            
    except Exception as e:
        return False, f"Exception: {e}"
    finally:
        proc.terminate()
        proc.wait(timeout=5)

def run_all_tests():
    """Run comprehensive tests of all tools"""
    
    print("🚀 Starting comprehensive ahma_mcp tools test...\n")
    
    # Create test files for our tests
    test_dir = "/tmp/ahma_mcp_test"
    os.makedirs(test_dir, exist_ok=True)
    
    # Create test files
    with open(f"{test_dir}/test.txt", "w") as f:
        f.write("Hello World\nThis is a test file\nTODO: Add more content\nAnother line\n")
    
    with open(f"{test_dir}/numbers.txt", "w") as f:
        f.write("1\n2\n3\n4\n5\n")
    
    print(f"📁 Created test files in: {test_dir}\n")
    
    tests = [
        # Test echo_run
        {
            "tool": "echo_run",
            "args": {"text": "Hello from ahma_mcp!"},
            "description": "Echo simple text"
        },
        
        # Test ls_run
        {
            "tool": "ls_run", 
            "args": {"path": test_dir},
            "description": "List test directory contents"
        },
        
        # Test cat_run
        {
            "tool": "cat_run",
            "args": {"file": f"{test_dir}/test.txt"},
            "description": "Display test file contents"
        },
        
        # Test grep_run
        {
            "tool": "grep_run",
            "args": {"pattern": "TODO", "file": f"{test_dir}/test.txt"},
            "description": "Search for TODO in test file"
        },
        
        # Test sed_run
        {
            "tool": "sed_run",
            "args": {"expression": "s/Hello/Hi/g", "file": f"{test_dir}/test.txt"},
            "description": "Replace 'Hello' with 'Hi' in test file"
        },
        
        # Test git_status
        {
            "tool": "git_status",
            "args": {},
            "description": "Check git repository status"
        },
    ]
    
    results = []
    
    for test in tests:
        print(f"🧪 {test['description']}")
        success, result = test_tool_execution(test["tool"], test.get("args"))
        
        if success:
            print(f"✅ SUCCESS: {test['tool']}")
            if isinstance(result, dict) and "content" in result:
                content = result["content"][:200] + "..." if len(result["content"]) > 200 else result["content"]
                print(f"📄 Output: {content}")
            else:
                print(f"📄 Result: {str(result)[:200]}")
        else:
            print(f"❌ FAILED: {test['tool']} - {result}")
        
        results.append((test["tool"], success))
        print("-" * 60)
    
    # Summary
    success_count = sum(1 for _, success in results if success)
    print(f"\n📊 TEST SUMMARY: {success_count}/{len(results)} tools working correctly")
    
    for tool, success in results:
        status = "✅" if success else "❌"
        print(f"{status} {tool}")
    
    # Cleanup
    import shutil
    shutil.rmtree(test_dir, ignore_errors=True)
    
    return success_count == len(results)

if __name__ == "__main__":
    success = run_all_tests()
    sys.exit(0 if success else 1)
