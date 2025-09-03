#!/usr/bin/env cargo +stable -Zscript
```cargo
[dependencies]
tokio = { version = "1.0", features = ["full"] }
serde_json = "1.0"
```

use std::process::{Command, Stdio};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::time::{timeout, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Test the shell setup script manually
    let setup_script = r#"
# Portable minimal shell setup for async_cargo_mcp (macOS bash 3.2 compatible)
set +e

json_escape_file() {
    # Use jq -Rs . to JSON-encode entire file contents
    jq -Rs . 2>/dev/null || {
        # Fallback: basic escaping (quotes and newlines)
        sed 's/\\/\\\\/g; s/"/\\"/g; s/$/\\n/' | tr -d '\n' | sed 's/\\n$//' | sed 's/^/"/;s/$/"/'
    }
}

execute_command() {
    cmd_json="$1"
    id=$(echo "$cmd_json" | jq -r '.id')
    working_dir=$(echo "$cmd_json" | jq -r '.working_dir')
    
    # Safely read command and arguments into a bash array
    # This is the critical security change to prevent command injection
    mapfile -t cmd_array < <(echo "$cmd_json" | jq -r '.command[]')

    cd "$working_dir" 2>/dev/null || {
        echo '{"id":"'"$id"'","exit_code":1,"stdout":"","stderr":"Failed to change directory","duration_ms":0}'
        return
    }

    start_time=$(date +%s)
    temp_stdout=$(mktemp)
    temp_stderr=$(mktemp)

    # Execute command directly, with each part as a separate argument
    "${cmd_array[@]}" >"$temp_stdout" 2>"$temp_stderr"
    exit_code=$?
    end_time=$(date +%s)
    duration=$(((end_time - start_time)*1000))

    stdout_json=$(cat "$temp_stdout" | json_escape_file)
    stderr_json=$(cat "$temp_stderr" | json_escape_file)
    rm -f "$temp_stdout" "$temp_stderr"
    echo '{"id":"'"$id"'","exit_code":'"$exit_code"',"stdout":'"$stdout_json"',"stderr":'"$stderr_json"',"duration_ms":'"$duration"'}'
}

echo "SHELL_READY"

while IFS= read -r line; do
    [ -z "$line" ] && continue
    if [ "$line" = "HEALTH_CHECK" ]; then
        echo "HEALTHY"
    elif [ "$line" = "SHUTDOWN" ]; then
        break
    else
        execute_command "$line"
    fi
done
"#;

    println!("Starting shell process...");

    // Spawn bash process
    let mut process = Command::new("bash")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(".")
        .spawn()?;

    let mut stdin = process.stdin.take().unwrap();
    let stdout = process.stdout.take().unwrap();
    let mut stdout_reader = BufReader::new(stdout);

    // Send setup script
    stdin.write_all(setup_script.as_bytes()).await?;
    stdin.flush().await?;

    // Wait for ready signal
    let mut ready_line = String::new();
    stdout_reader.read_line(&mut ready_line).await?;
    println!("Shell ready response: '{}'", ready_line.trim());

    if ready_line.trim() != "SHELL_READY" {
        panic!("Shell not ready: '{}'", ready_line.trim());
    }

    // Test simple ls command
    let test_cmd = r#"{"id":"test_ls","working_dir":".","command":["ls"]}"#;
    println!("Sending command: {}", test_cmd);
    
    stdin.write_all(test_cmd.as_bytes()).await?;
    stdin.write_all(b"\n").await?;
    stdin.flush().await?;

    // Read response with timeout
    let response_future = async {
        let mut response_line = String::new();
        stdout_reader.read_line(&mut response_line).await?;
        Ok::<String, std::io::Error>(response_line)
    };

    match timeout(Duration::from_secs(10), response_future).await {
        Ok(Ok(response)) => {
            println!("Shell response: {}", response.trim());
            
            // Parse and pretty print JSON
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&response) {
                println!("Parsed JSON: {}", serde_json::to_string_pretty(&parsed)?);
            }
        }
        Ok(Err(e)) => println!("IO error: {}", e),
        Err(_) => println!("Timeout waiting for response"),
    }

    // Shutdown
    stdin.write_all(b"SHUTDOWN\n").await?;
    stdin.flush().await?;

    Ok(())
}
