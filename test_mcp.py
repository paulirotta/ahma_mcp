#!/usr/bin/env python3
"""Test MCP protocol communication with ahma_mcp server"""

import json
import subprocess
import sys
import time
import threading

def test_mcp_communication():
    """Test MCP protocol with ahma_mcp server"""
    
    # Start the server process
    proc = subprocess.Popen(
        ['./target/release/ahma_mcp', '--tools-dir', 'tools'],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=1
    )
    
    def read_stderr():
        """Read stderr in background to capture server logs"""
        for line in iter(proc.stderr.readline, ''):
            print(f"SERVER: {line.strip()}", file=sys.stderr)
    
    stderr_thread = threading.Thread(target=read_stderr, daemon=True)
    stderr_thread.start()
    
    try:
        # Give server time to start
        time.sleep(1)
        
        # Send initialization request
        init_request = {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "roots": {"listChanged": False},
                    "sampling": {}
                },
                "clientInfo": {
                    "name": "test_client",
                    "version": "0.1.0"
                }
            }
        }
        
        print("Sending initialization request...")
        request_json = json.dumps(init_request)
        print(f"Request: {request_json}")
        proc.stdin.write(request_json + "\n")
        proc.stdin.flush()
        
        # Read initialization response
        print("Waiting for init response...")
        response_line = proc.stdout.readline()
        if response_line.strip():
            print(f"Init response: {response_line.strip()}")
            
            try:
                init_response = json.loads(response_line.strip())
                if "result" in init_response:
                    print("✅ Server initialized successfully!")
                    
                    # Send initialized notification
                    initialized_notification = {
                        "jsonrpc": "2.0",
                        "method": "notifications/initialized"
                    }
                    
                    print("Sending initialized notification...")
                    proc.stdin.write(json.dumps(initialized_notification) + "\n")
                    proc.stdin.flush()
                    time.sleep(0.1)
                    
                    # Now try to list tools
                    tools_request = {
                        "jsonrpc": "2.0",
                        "id": 2,
                        "method": "tools/list",
                        "params": {}
                    }
                    
                    print("Sending tools/list request...")
                    proc.stdin.write(json.dumps(tools_request) + "\n")
                    proc.stdin.flush()
                    
                    # Read tools response
                    print("Waiting for tools response...")
                    tools_response = proc.stdout.readline()
                    if tools_response.strip():
                        print(f"Tools response: {tools_response.strip()}")
                        
                        try:
                            tools_data = json.loads(tools_response.strip())
                            if "result" in tools_data and "tools" in tools_data["result"]:
                                print(f"\n✅ Found {len(tools_data['result']['tools'])} tools:")
                                for tool in tools_data['result']['tools']:
                                    print(f"  - {tool['name']}: {tool.get('description', 'No description')}")
                                return True
                            else:
                                print(f"❌ Unexpected tools response format: {tools_data}")
                        except json.JSONDecodeError as e:
                            print(f"❌ Failed to parse tools response: {e}")
                    else:
                        print("❌ No tools response received")
                else:
                    print(f"❌ Init failed: {init_response}")
            except json.JSONDecodeError as e:
                print(f"❌ Failed to parse init response: {e}")
        else:
            print("❌ No init response received")
            
    except Exception as e:
        print(f"❌ Error: {e}")
    finally:
        proc.terminate()
        proc.wait(timeout=5)
    
    return False

if __name__ == "__main__":
    success = test_mcp_communication()
    sys.exit(0 if success else 1)
