use crate::AppState;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub agent: String,
    pub title: String,
    pub cwd: String,
    pub status: String,
    pub terminal: String,
    pub tab_id: String,
    pub started_at: u64,
    pub last_activity: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequest {
    pub id: String,
    pub session_id: String,
    pub request_type: String,
    pub tool_name: Option<String>,
    pub message: String,
    pub options: Option<Vec<String>>,
    pub timestamp: u64,
}

/// Matches the JSON Claude Code pipes to hook commands via stdin.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct ClaudeHookInput {
    session_id: String,
    agent: Option<String>,
    tool_name: Option<String>,
    tool_input: Option<serde_json::Value>,
    tool_response: Option<serde_json::Value>,
    stop_hook_active: Option<bool>,
    message: Option<String>,
    cwd: Option<String>,
}

pub fn socket_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("vibe-island")
        .join("claude.sock")
}

pub async fn start_bridge(
    app_handle: AppHandle,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("Starting Claude Code bridge");

    let path = socket_path();

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if path.exists() {
        std::fs::remove_file(&path)?;
    }

    let listener = UnixListener::bind(&path)?;
    info!("Listening on {:?}", path);

    // Setup hooks in the background — non-fatal if it fails.
    let hook_socket = path.clone();
    tokio::spawn(async move {
        let home = match dirs::home_dir() {
            Some(h) => h,
            None => return,
        };

        // Setup / verify Claude Code hooks
        let claude_settings = home.join(".claude").join("settings.json");
        if let Err(e) = setup_claude_hooks(&claude_settings, &hook_socket).await {
            warn!("Claude hook setup failed: {}", e);
        }

        // Setup / verify Gemini hooks
        let gemini_settings = home.join(".gemini").join("settings.json");
        if let Err(e) = setup_gemini_hooks(&gemini_settings).await {
            warn!("Gemini hook setup failed: {}", e);
        }
    });

    // Periodic heartbeat: detect dead sessions whose parent process has exited.
    let heartbeat_handle = app_handle.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
        loop {
            interval.tick().await;
            let state = heartbeat_handle.state::<AppState>();
            let sessions_arc = state.sessions.clone();
            let mut sessions = sessions_arc.lock().await;
            let mut dead_ids = Vec::new();
            for session in sessions.iter() {
                if let Some(pid) = session.id.strip_prefix("cli-") {
                    if let Ok(pid_num) = pid.parse::<u32>() {
                        if !is_pid_alive(pid_num) {
                            dead_ids.push(session.id.clone());
                        }
                    }
                }
            }
            for sid in dead_ids {
                info!("Cleaning up dead session: {}", sid);
                sessions.retain(|s| s.id != sid);
                let _ = heartbeat_handle.emit("ipc-event", crate::ipc::IPCEvent::SessionEnded { session_id: sid });
            }
        }
    });

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let handle = app_handle.clone();
                    tokio::spawn(handle_connection(stream, handle));
                }
                Err(e) => error!("Accept error: {}", e),
            }
        }
    });

    Ok(())
}

async fn handle_connection(stream: UnixStream, app_handle: AppHandle) {
    let (reader, writer) = stream.into_split();
    let mut reader = tokio::io::BufReader::new(reader);
    let mut line = String::new();

    // Each hook invocation sends exactly one JSON line then waits for a response.
    line.clear();
    match reader.read_line(&mut line).await {
        Ok(0) | Err(_) => return,
        Ok(_) => {}
    }

    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }

    debug!("Hook input: {}", trimmed);

    let hook: ClaudeHookInput = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(e) => {
            error!("Failed to parse hook input: {} — {}", e, trimmed);
            return;
        }
    };

    let sessions_arc = {
        let state = app_handle.state::<AppState>();
        state.sessions.clone()
    };

    // Normalize session_id: if we already have a running session from the same
    // agent on the same terminal, reuse that session_id to avoid duplicates.
    let agent_name = hook.agent.clone().unwrap_or_else(|| "claude-code".to_string());
    let terminal = detect_terminal();
    let resolved_id = {
        let sessions = sessions_arc.lock().await;
        let exact = sessions.iter().any(|s| s.id == hook.session_id);
        if exact {
            hook.session_id.clone()
        } else if let Some(existing) = sessions.iter().find(|s| {
            s.agent == agent_name && s.terminal == terminal && s.status != "completed"
        }) {
            existing.id.clone()
        } else {
            hook.session_id.clone()
        }
    };

    // Ensure a Session entry exists.
    {
        let mut sessions = sessions_arc.lock().await;
        
        if let Some(s) = sessions.iter_mut().find(|s| s.id == resolved_id) {
            if s.agent == "claude-code" && agent_name != "claude-code" {
                s.agent = agent_name;
                let session_clone = s.clone();
                drop(sessions);
                let _ = app_handle.emit("ipc-event", crate::ipc::IPCEvent::SessionUpdated { session: session_clone });
            }
        } else {
            let title = hook.cwd.as_ref()
                .and_then(|cwd| std::path::Path::new(cwd).file_name())
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| agent_name.clone());

            let session = Session {
                id: resolved_id.clone(),
                agent: agent_name,
                title,
                cwd: hook.cwd.clone().unwrap_or_default(),
                status: "running".to_string(),
                terminal: detect_terminal(),
                tab_id: format!("tab-{}", resolved_id),
                started_at: now_secs(),
                last_activity: now_secs(),
            };
            sessions.push(session.clone());
            drop(sessions);
            let _ = app_handle.emit("ipc-event", crate::ipc::IPCEvent::SessionStarted { session });
        }
    }

    if hook.stop_hook_active.is_some() {
        // ── Stop hook ──────────────────────────────────────────────────────────
        let session_id = resolved_id.clone();
        let session_clone = {
            let mut sessions = sessions_arc.lock().await;
            if let Some(s) = sessions.iter_mut().find(|s| s.id == session_id) {
                s.status = "completed".to_string();
                Some(s.clone())
            } else {
                None
            }
        };

        if let Some(session) = session_clone {
            let _ = app_handle.emit("ipc-event", crate::ipc::IPCEvent::SessionUpdated { session });
        }

        let mut w = writer;
        let _ = w.write_all(b"{}\n").await;
    }
 else if hook.tool_name.is_some() && hook.tool_response.is_none() {
        // ── PreToolUse: notify and let user handle in terminal ────
        let tool_name = hook.tool_name.unwrap();
        let message = format_tool_message(&tool_name, &hook.tool_input);

        let session_clone = {
            let mut sessions = sessions_arc.lock().await;
            if let Some(s) = sessions.iter_mut().find(|s| s.id == resolved_id) {                s.status = "waiting".to_string();
                s.title = message.clone();
                s.last_activity = now_secs();
                Some(s.clone())
            } else {
                None
            }
        };

        if let Some(session) = session_clone {
            let _ = app_handle.emit("ipc-event", crate::ipc::IPCEvent::SessionUpdated { session });
        }

        let mut w = writer;
        let _ = w.write_all(b"{}\n").await;
    } else if hook.tool_name.is_some() && hook.tool_response.is_some() {
        // ── PostToolUse ────────────────────────────────────────────────────────
        let session_clone = {
            let mut sessions = sessions_arc.lock().await;
            if let Some(s) = sessions.iter_mut().find(|s| s.id == resolved_id) {                s.last_activity = now_secs();
                s.status = "running".to_string();
                Some(s.clone())
            } else {
                None
            }
        };

        if let Some(session) = session_clone {
            let _ = app_handle.emit("ipc-event", crate::ipc::IPCEvent::SessionUpdated { session });
        }

        let mut w = writer;
        let _ = w.write_all(b"{}\n").await;
    } else if hook.message.is_some() {
        // ── Notification ───────────────────────────────────────────────────────
        let session_clone = {
            let mut sessions = sessions_arc.lock().await;
            if let Some(s) = sessions.iter_mut().find(|s| s.id == resolved_id) {                s.status = "waiting".to_string();
                Some(s.clone())
            } else {
                None
            }
        };

        if let Some(session) = session_clone {
            let _ = app_handle.emit("ipc-event", crate::ipc::IPCEvent::SessionUpdated { session });
        }

        let mut w = writer;
        let _ = w.write_all(b"{}\n").await;
    } else {
        let mut w = writer;
        let _ = w.write_all(b"{}\n").await;
    }
}

fn format_tool_message(tool_name: &str, tool_input: &Option<serde_json::Value>) -> String {
    if let Some(input) = tool_input {
        if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
            return format!("{} {}", tool_name, path);
        }
        if let Some(cmd) = input.get("command").and_then(|v| v.as_str()) {
            return format!("{}: {}", tool_name, cmd);
        }
    }
    format!("Use tool: {}", tool_name)
}

fn detect_terminal() -> String {
    match std::env::var("TERM_PROGRAM").as_deref() {
        Ok("Ghostty") => "Ghostty".to_string(),
        Ok("iTerm.app") => "iTerm2".to_string(),
        Ok("Apple_Terminal") => "Terminal".to_string(),
        Ok("VSCode") => "VSCode".to_string(),
        Ok("cursor") => "Cursor".to_string(),
        Ok("windsurf") => "Windsurf".to_string(),
        Ok("Zed") => "Zed".to_string(),
        Ok(other) if !other.is_empty() => other.to_string(),
        _ => "Unknown".to_string(),
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn is_pid_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_terminal_ghostty() {
        std::env::set_var("TERM_PROGRAM", "Ghostty");
        assert_eq!(detect_terminal(), "Ghostty");
    }

    #[test]
    fn test_detect_terminal_iterm() {
        std::env::set_var("TERM_PROGRAM", "iTerm.app");
        assert_eq!(detect_terminal(), "iTerm2");
    }

    #[test]
    fn test_detect_terminal_apple_terminal() {
        std::env::set_var("TERM_PROGRAM", "Apple_Terminal");
        assert_eq!(detect_terminal(), "Terminal");
    }

    #[test]
    fn test_detect_terminal_vscode() {
        std::env::set_var("TERM_PROGRAM", "VSCode");
        assert_eq!(detect_terminal(), "VSCode");
    }

    #[test]
    fn test_detect_terminal_cursor() {
        std::env::set_var("TERM_PROGRAM", "cursor");
        assert_eq!(detect_terminal(), "Cursor");
    }

    #[test]
    fn test_detect_terminal_windsurf() {
        std::env::set_var("TERM_PROGRAM", "windsurf");
        assert_eq!(detect_terminal(), "Windsurf");
    }

    #[test]
    fn test_detect_terminal_zed() {
        std::env::set_var("TERM_PROGRAM", "Zed");
        assert_eq!(detect_terminal(), "Zed");
    }

    #[test]
    fn test_detect_terminal_unknown_app() {
        std::env::set_var("TERM_PROGRAM", "WezTerm");
        assert_eq!(detect_terminal(), "WezTerm");
    }

    #[test]
    fn test_detect_terminal_empty_env() {
        std::env::remove_var("TERM_PROGRAM");
        assert_eq!(detect_terminal(), "Unknown");
    }

    #[test]
    fn test_session_serialization_camel_case() {
        let session = Session {
            id: "test-id".to_string(),
            agent: "gemini".to_string(),
            title: "Test Title".to_string(),
            cwd: "/Users/test/dir".to_string(),
            status: "running".to_string(),
            terminal: "iTerm2".to_string(),
            tab_id: "tab-1".to_string(),
            started_at: 1000,
            last_activity: 1100,
        };

        let json = serde_json::to_string(&session).unwrap();
        assert!(json.contains("\"tabId\":\"tab-1\""));
        assert!(json.contains("\"startedAt\":1000"));
        assert!(json.contains("\"lastActivity\":1100"));
    }

    #[test]
    fn test_session_deserialization() {
        let json = r#"{
            "id": "cli-999",
            "agent": "opencode",
            "title": "my-app",
            "cwd": "/home/user/my-app",
            "status": "waiting",
            "terminal": "Ghostty",
            "tabId": "tab-cli-999",
            "startedAt": 5000,
            "lastActivity": 5100
        }"#;

        let session: Session = serde_json::from_str(json).unwrap();
        assert_eq!(session.id, "cli-999");
        assert_eq!(session.agent, "opencode");
        assert_eq!(session.status, "waiting");
        assert_eq!(session.terminal, "Ghostty");
        assert_eq!(session.tab_id, "tab-cli-999");
        assert_eq!(session.started_at, 5000);
    }

    #[test]
    fn test_permission_request_serialization() {
        let req = PermissionRequest {
            id: "req-1".to_string(),
            session_id: "cli-100".to_string(),
            request_type: "tool_use".to_string(),
            tool_name: Some("Write".to_string()),
            message: "Allow file write?".to_string(),
            options: Some(vec!["Allow once".to_string(), "Deny".to_string()]),
            timestamp: 9999,
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"sessionId\":\"cli-100\""));
        assert!(json.contains("\"requestType\":\"tool_use\""));
        assert!(json.contains("\"toolName\":\"Write\""));
    }

    #[test]
    fn test_format_tool_message_with_path() {
        let input = serde_json::json!({"path": "/Users/test/file.ts"});
        let msg = format_tool_message("Write", &Some(input));
        assert_eq!(msg, "Write /Users/test/file.ts");
    }

    #[test]
    fn test_format_tool_message_with_command() {
        let input = serde_json::json!({"command": "git status"});
        let msg = format_tool_message("Bash", &Some(input));
        assert_eq!(msg, "Bash: git status");
    }

    #[test]
    fn test_format_tool_message_fallback() {
        let input = serde_json::json!({"query": "search term"});
        let msg = format_tool_message("Search", &Some(input));
        assert_eq!(msg, "Use tool: Search");
    }

    #[test]
    fn test_format_tool_message_none_input() {
        let msg = format_tool_message("Unknown", &None);
        assert_eq!(msg, "Use tool: Unknown");
    }

    #[test]
    fn test_socket_path() {
        let path = socket_path();
        assert!(path.to_string_lossy().contains("vibe-island"));
        assert!(path.to_string_lossy().ends_with("claude.sock"));
    }

    #[test]
    fn test_now_secs_returns_reasonable_value() {
        let t = now_secs();
        assert!(t > 1700000000);
    }

    #[test]
    fn test_is_pid_alive_current_process() {
        let pid = std::process::id();
        assert!(is_pid_alive(pid));
    }

    #[test]
    fn test_is_pid_alive_dead_pid() {
        assert!(!is_pid_alive(999999999));
    }

    #[test]
    fn test_claude_hook_input_deserialization() {
        let json = r#"{
            "session_id": "cli-123",
            "agent": "claude-code",
            "tool_name": "Write",
            "tool_input": {"path": "/test.txt"},
            "cwd": "/Users/test"
        }"#;

        let input: ClaudeHookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.session_id, "cli-123");
        assert_eq!(input.agent, Some("claude-code".to_string()));
        assert_eq!(input.tool_name, Some("Write".to_string()));
        assert!(input.tool_input.is_some());
        assert!(input.tool_response.is_none());
        assert!(input.stop_hook_active.is_none());
    }

    #[test]
    fn test_claude_hook_input_stop_event() {
        let json = r#"{
            "session_id": "cli-456",
            "stop_hook_active": true
        }"#;

        let input: ClaudeHookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.stop_hook_active, Some(true));
    }

    #[test]
    fn test_claude_hook_input_notification() {
        let json = r#"{
            "session_id": "cli-789",
            "message": "Task completed"
        }"#;

        let input: ClaudeHookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.message, Some("Task completed".to_string()));
    }
}

/// Write `~/.config/vibe-island/hook.py` and register it under the correct
/// Claude Code hook event names in `settings.json`.
async fn setup_claude_hooks(
    settings_path: &PathBuf,
    socket_path: &PathBuf,
) -> Result<(), String> {
    // ── 1. Write the hook script ─────────────────────────────────────────────
    let script_dir = dirs::home_dir()
        .ok_or("Cannot find home directory")?
        .join(".config")
        .join("vibe-island");

    std::fs::create_dir_all(&script_dir).map_err(|e| e.to_string())?;

    let script_path = script_dir.join("hook.py");
    let socket_str = socket_path.to_string_lossy();

    // The script reads Claude's JSON from stdin, sends it to our socket, and
    // waits for a response so PreToolUse can block if the user denies.
    let script = format!(
        r#"#!/usr/bin/env python3
import sys, socket, os, json, argparse, subprocess

SOCKET = "{socket}"

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--end", action="store_true")
    parser.add_argument("--exit", action="store_true")
    parser.add_argument("--waiting", action="store_true")
    parser.add_argument("--running", action="store_true")
    args, unknown = parser.parse_known_args()

    agent = os.environ.get('VIBE_AGENT')
    if not agent:
        try:
            ppid = os.getppid()
            pname = subprocess.check_output(['ps', '-p', str(ppid), '-o', 'comm=']).decode().strip()
            if 'opencode' in pname.lower():
                agent = 'opencode'
            elif 'codex' in pname.lower():
                agent = 'codex'
            else:
                agent = 'claude-code'
        except Exception:
            agent = 'claude-code'

    data = b""
    if not sys.stdin.isatty():
        data = sys.stdin.buffer.read().strip()

    payload = {{}}
    if data:
        try:
            payload = json.loads(data)
        except json.JSONDecodeError:
            payload = {{
                "message": data.decode('utf-8', errors='ignore'),
            }}

    if "session_id" not in payload:
        payload["session_id"] = os.environ.get("VIBE_SESSION_ID", f"cli-{{os.getppid()}}")

    if args.end:
        payload["stop_hook_active"] = True
    if args.exit:
        payload["exit_session"] = True
    if args.waiting:
        payload["tool_name"] = payload.get("tool_name", "Gemini Action")
    if args.running:
        payload["tool_response"] = payload.get("tool_response", {{}})

    if "agent" not in payload:
        payload["agent"] = agent

    if "cwd" not in payload:
        payload["cwd"] = os.getcwd()

    try:
        s = socket.socket(socket.AF_UNIX)
        s.settimeout(5)
        s.connect(SOCKET)
        s.sendall(json.dumps(payload).encode('utf-8') + b"\n")

        if payload.get("tool_name") and not payload.get("tool_response"):
            buf = b""
            while b"\n" not in buf:
                chunk = s.recv(4096)
                if not chunk: break
                buf += chunk
            resp = json.loads(buf.strip() or b"{{}}")
            if resp.get("blocked"):
                reason = resp.get("reason", "Denied by user")
                print(json.dumps({{"type": "block", "message": reason}}))
                sys.exit(2)
        s.close()
    except Exception:
        pass
    sys.exit(0)

if __name__ == "__main__":
    main()
"#,
        socket = socket_str
    );

    tokio::fs::write(&script_path, script)
        .await
        .map_err(|e| e.to_string())?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&script_path)
            .map_err(|e| e.to_string())?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script_path, perms).map_err(|e| e.to_string())?;
    }

    info!("Hook script written to {:?}", script_path);

    // ── 2. Update settings.json ──────────────────────────────────────────────
    if !settings_path.exists() {
        return Ok(());
    }

    let content = tokio::fs::read_to_string(settings_path)
        .await
        .map_err(|e| e.to_string())?;

    let mut settings: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| e.to_string())?;

    let settings_obj = settings
        .as_object_mut()
        .ok_or("settings.json is not a JSON object")?;

    // Remove stale keys from the old setup_claude_hooks.
    if let Some(hooks) = settings_obj.get_mut("hooks").and_then(|h| h.as_object_mut()) {
        hooks.remove("permissionRequest");
        hooks.remove("sessionStart");
        hooks.remove("sessionEnd");
    }

    let cmd = format!("python3 {}", script_path.display());

    let hooks = settings_obj
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));

    let hooks_obj = hooks.as_object_mut().ok_or("hooks is not a JSON object")?;

    for (event, needs_matcher) in [
        ("PreToolUse", true),
        ("PostToolUse", true),
        ("Stop", false),
        ("Notification", false),
    ] {
        let arr = hooks_obj
            .entry(event)
            .or_insert_with(|| serde_json::json!([]));

        // Only append if our command isn't already registered.
        let already = arr
            .as_array()
            .map(|a| {
                a.iter().any(|entry| {
                    entry
                        .get("hooks")
                        .and_then(|h| h.as_array())
                        .map(|h| {
                            h.iter().any(|c| {
                                c.get("command")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.contains("vibe-island"))
                                    .unwrap_or(false)
                            })
                        })
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);

        if !already {
            if let Some(a) = arr.as_array_mut() {
                let entry = if needs_matcher {
                    serde_json::json!({
                        "matcher": "",
                        "hooks": [{ "type": "command", "command": cmd }]
                    })
                } else {
                    serde_json::json!({
                        "hooks": [{ "type": "command", "command": cmd }]
                    })
                };
                a.push(entry);
            }
        }
    }

    tokio::fs::write(
        settings_path,
        serde_json::to_string_pretty(&settings).unwrap(),
    )
    .await
    .map_err(|e| e.to_string())?;

    info!("Claude hooks configured in {:?}", settings_path);
    Ok(())
}

async fn setup_gemini_hooks(settings_path: &PathBuf) -> Result<(), String> {
    if !settings_path.exists() {
        return Ok(());
    }

    let content = tokio::fs::read_to_string(settings_path)
        .await
        .map_err(|e| e.to_string())?;

    let mut settings: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| e.to_string())?;

    let hooks_obj = settings
        .as_object_mut()
        .ok_or("settings.json is not a JSON object")?
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or("hooks is not a JSON object")?
        .to_owned();

    let vibe_events = [
        ("SessionStart", ""),
        ("BeforeTool", "--waiting"),
        ("AfterTool", "--running"),
        ("AfterAgent", "--end"),
        ("SessionEnd", "--exit"),
    ];

    let mut hooks = serde_json::Value::Object(hooks_obj);
    let mut changed = false;

    for (event, flag) in vibe_events {
        let cmd = format!("VIBE_AGENT=gemini python3 $HOME/.config/vibe-island/hook.py {}", flag);
        let arr = hooks
            .as_object_mut()
            .unwrap()
            .entry(event)
            .or_insert_with(|| serde_json::json!([]));

        let already = arr
            .as_array()
            .map(|a| {
                a.iter().any(|entry| {
                    entry
                        .get("hooks")
                        .and_then(|h| h.as_array())
                        .map(|h| {
                            h.iter().any(|c| {
                                c.get("command")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.contains("vibe-island"))
                                    .unwrap_or(false)
                            })
                        })
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);

        if !already {
            if let Some(a) = arr.as_array_mut() {
                a.push(serde_json::json!({
                    "hooks": [{ "type": "command", "command": cmd, "timeout": 5000 }]
                }));
                changed = true;
            }
        }
    }

    if changed {
        settings.as_object_mut().unwrap().insert("hooks".to_string(), hooks);
        tokio::fs::write(
            settings_path,
            serde_json::to_string_pretty(&settings).unwrap(),
        )
        .await
        .map_err(|e| e.to_string())?;
        info!("Gemini hooks configured in {:?}", settings_path);
    }

    Ok(())
}
