use serde_json::Value;
use std::env;
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

fn socket_path() -> PathBuf {
    if let Ok(p) = env::var("VIBE_SOCKET_PATH") {
        return PathBuf::from(p);
    }
    dirs_home()
        .join(".config")
        .join("vibe-island")
        .join("claude.sock")
}

fn dirs_home() -> PathBuf {
    if let Ok(h) = env::var("HOME") {
        return PathBuf::from(h);
    }
    // Fallback: try getpwuid
    PathBuf::from("/tmp")
}

fn detect_agent() -> String {
    if let Ok(a) = env::var("VIBE_AGENT") {
        if !a.is_empty() {
            return a;
        }
    }

    let ppid = libc_getppid();

    if let Ok(output) = Command::new("ps")
        .args(["-p", &ppid.to_string(), "-o", "comm="])
        .output()
    {
        let name = String::from_utf8_lossy(&output.stdout).trim().to_lowercase();
        if name.contains("opencode") {
            return "opencode".to_string();
        }
        if name.contains("codex") {
            return "codex".to_string();
        }
    }

    "claude-code".to_string()
}

fn enrich_payload(payload: &mut Value) {
    if payload.get("session_id").is_none() {
        let sid = env::var("VIBE_SESSION_ID")
            .unwrap_or_else(|_| format!("cli-{}", std::process::id()));
        payload["session_id"] = Value::String(sid);
    }

    if payload.get("agent").is_none() {
        payload["agent"] = Value::String(detect_agent());
    }

    if payload.get("cwd").is_none() {
        if let Ok(cwd) = env::current_dir() {
            payload["cwd"] = Value::String(cwd.to_string_lossy().into_owned());
        }
    }

    let env_fields = [
        ("TERM_PROGRAM", "term_program"),
        ("ITERM_SESSION_ID", "iterm_session_id"),
        ("TMUX_PANE", "tmux_pane"),
        ("__CFBundleIdentifier", "cf_bundle_id"),
    ];

    for (env_key, json_key) in env_fields {
        if payload.get(json_key).is_none() {
            if let Ok(val) = env::var(env_key) {
                if !val.is_empty() {
                    payload[json_key] = Value::String(val);
                }
            }
        }
    }

    // TMUX — just presence indicates tmux is active
    if payload.get("tmux").is_none() {
        if env::var("TMUX").is_ok() {
            payload["tmux"] = Value::Bool(true);
        }
    }

    // Try to get TTY path
    if payload.get("tty").is_none() {
        if let Ok(tty) = ttyname() {
            payload["tty"] = Value::String(tty);
        }
    }
}

fn ttyname() -> Result<String, String> {
    // Try /dev/tty (controlling terminal)
    let output = Command::new("tty")
        .stdin(std::process::Stdio::inherit())
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if name != "not a tty" && !name.is_empty() {
            return Ok(name);
        }
    }
    Err("no tty".to_string())
}

fn main() {
    let mut args = env::args().skip(1).collect::<Vec<_>>();

    // Handle flags for Gemini-style hooks
    let mut flag_end = false;
    let mut flag_exit = false;
    let mut flag_waiting = false;
    let mut flag_running = false;

    args.retain(|a| {
        match a.as_str() {
            "--end" => { flag_end = true; false }
            "--exit" => { flag_exit = true; false }
            "--waiting" => { flag_waiting = true; false }
            "--running" => { flag_running = true; false }
            _ => true,
        }
    });

    // Read JSON from stdin
    let mut input = Vec::new();
    if !atty() {
        io::stdin().read_to_end(&mut input).ok();
    }

    let mut payload: Value = if input.is_empty() {
        serde_json::json!({})
    } else {
        match serde_json::from_slice(&input) {
            Ok(v) => v,
            Err(_) => {
                let text = String::from_utf8_lossy(&input);
                serde_json::json!({ "message": text.trim() })
            }
        }
    };

    // Apply flags
    if flag_end {
        payload["stop_hook_active"] = Value::Bool(true);
    }
    if flag_exit {
        payload["exit_session"] = Value::Bool(true);
    }
    if flag_waiting {
        payload["tool_name"] = payload.get("tool_name")
            .cloned()
            .unwrap_or(Value::String("Gemini Action".to_string()));
    }
    if flag_running {
        payload["tool_response"] = payload.get("tool_response")
            .cloned()
            .unwrap_or(Value::Object(serde_json::Map::new()));
    }

    enrich_payload(&mut payload);

    let sock = socket_path();
    let payload_bytes = serde_json::to_vec(&payload).unwrap_or_default();

    let mut stream = match UnixStream::connect(&sock) {
        Ok(s) => s,
        Err(_) => {
            // Fallback: try netcat
            let _ = Command::new("nc")
                .arg("-U")
                .arg(&sock)
                .stdin(std::process::Stdio::piped())
                .spawn()
                .and_then(|mut child| {
                    if let Some(ref mut stdin) = child.stdin {
                        let _ = stdin.write_all(&payload_bytes);
                        let _ = stdin.write_all(b"\n");
                    }
                    child.wait()
                });
            return;
        }
    };

    stream
        .set_read_timeout(Some(Duration::from_secs(86400)))
        .ok();
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .ok();

    if let Err(_) = stream.write_all(&payload_bytes) {
        return;
    }
    if let Err(_) = stream.write_all(b"\n") {
        return;
    }
    let _ = stream.flush();

    // For PreToolUse events (has tool_name but no tool_response): wait for response
    let is_blocking = payload.get("tool_name").is_some()
        && payload.get("tool_response").is_none()
        && payload.get("stop_hook_active").is_none();

    if is_blocking {
        let mut response = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    response.extend_from_slice(&buf[..n]);
                    if response.contains(&b'\n') {
                        break;
                    }
                }
                Err(_) => break,
            }
        }

        let resp_str = String::from_utf8_lossy(&response).trim().to_string();
        if !resp_str.is_empty() {
            if let Ok(resp) = serde_json::from_str::<Value>(&resp_str) {
                if resp.get("blocked").and_then(|v| v.as_bool()).unwrap_or(false) {
                    let reason = resp.get("reason")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Denied by user");
                    let msg = serde_json::json!({"type": "block", "message": reason});
                    let _ = io::stdout().write_all(serde_json::to_string(&msg).unwrap().as_bytes());
                    let _ = io::stdout().write_all(b"\n");
                    std::process::exit(2);
                }
            }
        }
    }
}

fn atty() -> bool {
    unsafe { libc_isatty(0) }
}

#[link(name = "c")]
extern "C" {
    fn isatty(fd: i32) -> i32;
    fn getppid() -> u32;
}

unsafe fn libc_isatty(fd: i32) -> bool {
    isatty(fd) == 1
}

fn libc_getppid() -> u32 {
    unsafe { getppid() }
}
