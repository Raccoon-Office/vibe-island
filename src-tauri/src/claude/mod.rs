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
    #[serde(default)]
    pub term_program: String,
    #[serde(default)]
    pub iterm_session_id: String,
    #[serde(default)]
    pub tmux_pane: String,
    #[serde(default)]
    pub tty: String,
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
    #[serde(default)]
    term_program: String,
    #[serde(default)]
    iterm_session_id: String,
    #[serde(default)]
    tmux_pane: String,
    #[serde(default)]
    tty: String,
}

pub fn socket_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("vibe-island")
        .join("claude.sock")
}

fn bridge_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("vibe-island")
        .join("vibe-bridge")
}

fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("vibe-island")
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

    deploy_bridge_binary();

    let hook_socket = path.clone();
    let bridge = bridge_path();
    tokio::spawn(async move {
        if let Err(e) = setup_and_verify_hooks(&hook_socket, &bridge).await {
            warn!("Hook setup/verify failed: {}", e);
        }
    });

    let _repair_handle = app_handle.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
        loop {
            interval.tick().await;
            let sock = socket_path();
            let brg = bridge_path();
            if let Err(e) = verify_and_repair_hooks(&sock, &brg).await {
                debug!("Periodic hook repair: {}", e);
            }
        }
    });

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

fn deploy_bridge_binary() {
    let dest = bridge_path();
    let config = config_dir();

    if let Err(e) = std::fs::create_dir_all(&config) {
        warn!("Failed to create config dir: {}", e);
        return;
    }

    if dest.exists() {
        return;
    }

    if let Ok(exe_dir) = std::env::current_exe() {
        if let Some(parent) = exe_dir.parent() {
            let bundled = parent.join("../Resources/resources/vibe-bridge");
            if bundled.exists() {
                if let Err(e) = std::fs::copy(&bundled, &dest) {
                    warn!("Failed to copy bundled bridge: {}", e);
                } else {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        if let Ok(mut perms) = std::fs::metadata(&dest).map(|m| m.permissions()) {
                            perms.set_mode(0o755);
                            let _ = std::fs::set_permissions(&dest, perms);
                        }
                    }
                    info!("Deployed bridge binary to {:?}", dest);
                    return;
                }
            }

            let sibling = parent.join("resources").join("vibe-bridge");
            if sibling.exists() {
                if let Err(e) = std::fs::copy(&sibling, &dest) {
                    warn!("Failed to copy sibling bridge: {}", e);
                } else {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        if let Ok(mut perms) = std::fs::metadata(&dest).map(|m| m.permissions()) {
                            perms.set_mode(0o755);
                            let _ = std::fs::set_permissions(&dest, perms);
                        }
                    }
                    info!("Deployed bridge binary from resources: {:?}", dest);
                    return;
                }
            }
        }
    }

    if let Ok(output) = std::process::Command::new("which")
        .arg("vibe-bridge")
        .output()
    {
        if output.status.success() {
            let found = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !found.is_empty() {
                if let Err(e) = std::os::unix::fs::symlink(&found, &dest) {
                    warn!("Failed to symlink bridge: {}", e);
                } else {
                    info!("Symlinked bridge from PATH: {} -> {:?}", found, dest);
                }
            }
        }
    }
}

async fn handle_connection(stream: UnixStream, app_handle: AppHandle) {
    let (reader, writer) = stream.into_split();
    let mut reader = tokio::io::BufReader::new(reader);
    let mut line = String::new();

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

    let agent_name = hook.agent.clone().unwrap_or_else(|| "claude-code".to_string());
    let terminal = detect_terminal_from_env(&hook);
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

    {
        let mut sessions = sessions_arc.lock().await;

        if let Some(s) = sessions.iter_mut().find(|s| s.id == resolved_id) {
            let mut changed = false;
            if s.agent == "claude-code" && agent_name != "claude-code" {
                s.agent = agent_name.clone();
                changed = true;
            }
            if s.term_program.is_empty() && !hook.term_program.is_empty() {
                s.term_program = hook.term_program.clone();
                changed = true;
            }
            if s.iterm_session_id.is_empty() && !hook.iterm_session_id.is_empty() {
                s.iterm_session_id = hook.iterm_session_id.clone();
                changed = true;
            }
            if s.tmux_pane.is_empty() && !hook.tmux_pane.is_empty() {
                s.tmux_pane = hook.tmux_pane.clone();
                changed = true;
            }
            if s.tty.is_empty() && !hook.tty.is_empty() {
                s.tty = hook.tty.clone();
                changed = true;
            }
            if changed {
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
                terminal: terminal.clone(),
                tab_id: format!("tab-{}", resolved_id),
                started_at: now_secs(),
                last_activity: now_secs(),
                term_program: hook.term_program.clone(),
                iterm_session_id: hook.iterm_session_id.clone(),
                tmux_pane: hook.tmux_pane.clone(),
                tty: hook.tty.clone(),
            };
            sessions.push(session.clone());
            drop(sessions);
            let _ = app_handle.emit("ipc-event", crate::ipc::IPCEvent::SessionStarted { session });
        }
    }

    if hook.stop_hook_active.is_some() {
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
    } else if hook.tool_name.is_some() && hook.tool_response.is_none() {
        let tool_name = hook.tool_name.unwrap();
        let message = format_tool_message(&tool_name, &hook.tool_input);

        let session_clone = {
            let mut sessions = sessions_arc.lock().await;
            if let Some(s) = sessions.iter_mut().find(|s| s.id == resolved_id) {
                s.status = "waiting".to_string();
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
        let session_clone = {
            let mut sessions = sessions_arc.lock().await;
            if let Some(s) = sessions.iter_mut().find(|s| s.id == resolved_id) {
                s.last_activity = now_secs();
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
        let session_clone = {
            let mut sessions = sessions_arc.lock().await;
            if let Some(s) = sessions.iter_mut().find(|s| s.id == resolved_id) {
                s.status = "waiting".to_string();
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

fn detect_terminal_from_env(hook: &ClaudeHookInput) -> String {
    if !hook.term_program.is_empty() {
        match hook.term_program.as_str() {
            "Ghostty" => return "Ghostty".to_string(),
            "iTerm.app" => return "iTerm2".to_string(),
            "Apple_Terminal" => return "Terminal".to_string(),
            "VSCode" => return "VSCode".to_string(),
            "cursor" => return "Cursor".to_string(),
            "windsurf" => return "Windsurf".to_string(),
            "Zed" => return "Zed".to_string(),
            other => return other.to_string(),
        }
    }
    detect_terminal()
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

async fn setup_and_verify_hooks(
    socket_path: &PathBuf,
    bridge_bin: &PathBuf,
) -> Result<(), String> {
    let home = dirs::home_dir().ok_or("Cannot find home directory")?;

    let claude_settings = home.join(".claude").join("settings.json");
    setup_claude_hooks(&claude_settings, bridge_bin).await?;

    let gemini_settings = home.join(".gemini").join("settings.json");
    setup_gemini_hooks(&gemini_settings, bridge_bin).await?;

    setup_opencode_plugin()?;

    verify_and_repair_hooks(socket_path, bridge_bin).await?;

    Ok(())
}

async fn verify_and_repair_hooks(
    _socket_path: &PathBuf,
    installed_bridge: &PathBuf,
) -> Result<(), String> {
    let home = dirs::home_dir().ok_or("Cannot find home directory")?;

    let claude_settings = home.join(".claude").join("settings.json");
    if claude_settings.exists() {
        verify_agent_hooks(&claude_settings, installed_bridge, "claude").await?;
    }

    let gemini_settings = home.join(".gemini").join("settings.json");
    if gemini_settings.exists() {
        verify_agent_hooks(&gemini_settings, installed_bridge, "gemini").await?;
    }

    let brg = bridge_path();
    if !brg.exists() {
        deploy_bridge_binary();
    }

    Ok(())
}

async fn verify_agent_hooks(
    settings_path: &PathBuf,
    installed_bridge: &PathBuf,
    agent: &str,
) -> Result<(), String> {
    let content = tokio::fs::read_to_string(settings_path)
        .await
        .map_err(|e| e.to_string())?;

    let mut settings: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| e.to_string())?;

    let settings_obj = settings
        .as_object_mut()
        .ok_or("settings.json is not a JSON object")?;

    let hooks = settings_obj
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));

    let hooks_obj = hooks
        .as_object_mut()
        .ok_or("hooks is not a JSON object")?;

    let mut changed = false;

    let bridge_str = installed_bridge.to_string_lossy();
    let has_bridge = hooks_obj.values().any(|arr| {
        arr.as_array().map(|a| {
            a.iter().any(|entry| {
                entry
                    .get("hooks")
                    .and_then(|h| h.as_array())
                    .map(|h| {
                        h.iter().any(|c| {
                            c.get("command")
                                .and_then(|v| v.as_str())
                                .map(|s| s.contains("vibe-bridge"))
                                .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
    });

    if has_bridge {
        return Ok(());
    }

    let has_stale = hooks_obj.values().any(|arr| {
        arr.as_array().map(|a| {
            a.iter().any(|entry| {
                entry
                    .get("hooks")
                    .and_then(|h| h.as_array())
                    .map(|h| {
                        h.iter().any(|c| {
                            c.get("command")
                                .and_then(|v| v.as_str())
                                .map(|s| s.contains("vibe-island") && s.contains("python3"))
                                .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
    });

    if has_stale {
        info!("Found stale python3 hooks for {}, replacing with bridge binary", agent);

        for (_event, entries) in hooks_obj.iter_mut() {
            if let Some(arr) = entries.as_array_mut() {
                arr.retain(|entry| {
                    entry
                        .get("hooks")
                        .and_then(|h| h.as_array())
                        .map(|h| {
                            !h.iter().any(|c| {
                                c.get("command")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.contains("vibe-island") && s.contains("python3"))
                                    .unwrap_or(false)
                            })
                        })
                        .unwrap_or(true)
                });
            }
        }
        changed = true;
    }

    let required_events = if agent == "claude" {
        vec![
            ("PreToolUse", true),
            ("PostToolUse", true),
            ("Stop", false),
            ("Notification", false),
        ]
    } else {
        vec![
            ("SessionStart", false),
            ("BeforeTool", false),
            ("AfterTool", false),
            ("AfterAgent", false),
            ("SessionEnd", false),
        ]
    };

    let cmd = if agent == "gemini" {
        format!("VIBE_AGENT=gemini {}", bridge_str)
    } else {
        bridge_str.to_string()
    };

    for (event, needs_matcher) in &required_events {
        let has_event = hooks_obj
            .get(*event)
            .and_then(|arr| arr.as_array())
            .map(|a| {
                a.iter().any(|entry| {
                    entry
                        .get("hooks")
                        .and_then(|h| h.as_array())
                        .map(|h| {
                            h.iter().any(|c| {
                                c.get("command")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.contains("vibe-bridge") || s.contains("vibe-island"))
                                    .unwrap_or(false)
                            })
                        })
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);

        if !has_event {
            let arr = hooks_obj
                .entry(*event)
                .or_insert_with(|| serde_json::json!([]));

            if let Some(a) = arr.as_array_mut() {
                let event_cmd = if agent == "gemini" {
                    match *event {
                        "BeforeTool" => format!("{} --waiting", cmd),
                        "AfterTool" => format!("{} --running", cmd),
                        "AfterAgent" => format!("{} --end", cmd),
                        "SessionEnd" => format!("{} --exit", cmd),
                        _ => cmd.clone(),
                    }
                } else {
                    cmd.clone()
                };

                let entry = if *needs_matcher {
                    serde_json::json!({
                        "matcher": "",
                        "hooks": [{ "type": "command", "command": event_cmd }]
                    })
                } else {
                    serde_json::json!({
                        "hooks": [{ "type": "command", "command": event_cmd }]
                    })
                };
                a.push(entry);
                changed = true;
            }
        }
    }

    if changed {
        tokio::fs::write(
            settings_path,
            serde_json::to_string_pretty(&settings).unwrap(),
        )
        .await
        .map_err(|e| e.to_string())?;
        info!("Repaired hooks for {} in {:?}", agent, settings_path);
    }

    Ok(())
}

async fn setup_claude_hooks(
    settings_path: &PathBuf,
    bridge_bin: &PathBuf,
) -> Result<(), String> {
    if !settings_path.exists() {
        return Ok(());
    }

    let config = config_dir();
    std::fs::create_dir_all(&config).map_err(|e| e.to_string())?;

    let content = tokio::fs::read_to_string(settings_path)
        .await
        .map_err(|e| e.to_string())?;

    let mut settings: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| e.to_string())?;

    let settings_obj = settings
        .as_object_mut()
        .ok_or("settings.json is not a JSON object")?;

    if let Some(hooks) = settings_obj.get_mut("hooks").and_then(|h| h.as_object_mut()) {
        hooks.remove("permissionRequest");
        hooks.remove("sessionStart");
        hooks.remove("sessionEnd");
    }

    let cmd = bridge_bin.to_string_lossy().to_string();

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
                                    .map(|s| s.contains("vibe-bridge") || s.contains("vibe-island"))
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

async fn setup_gemini_hooks(
    settings_path: &PathBuf,
    bridge_bin: &PathBuf,
) -> Result<(), String> {
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

    let bridge_str = bridge_bin.to_string_lossy();

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
        let cmd = if flag.is_empty() {
            format!("VIBE_AGENT=gemini {}", bridge_str)
        } else {
            format!("VIBE_AGENT=gemini {} {}", bridge_str, flag)
        };
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
                                    .map(|s| s.contains("vibe-bridge") || s.contains("vibe-island"))
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

fn setup_opencode_plugin() -> Result<(), String> {
    let home = dirs::home_dir().ok_or("Cannot find home directory")?;
    let plugin_dir = home.join(".config").join("opencode").join("plugins");
    std::fs::create_dir_all(&plugin_dir).map_err(|e| e.to_string())?;

    let plugin_path = plugin_dir.join("vibe-island.js");
    let sock = socket_path();

    let plugin_code = format!(
        r#"// Vibe Island OpenCode Plugin — auto-generated
const net = require('net');
const path = require('path');
const os = require('os');

const SOCKET_PATH = {socket_path_repr};

function getSessionId() {{
  return 'opencode-' + process.pid;
}}

function sendEvent(payload) {{
  return new Promise((resolve) => {{
    const client = net.createConnection(SOCKET_PATH, () => {{
      const data = JSON.stringify(payload) + '\n';
      client.write(data);
    }});
    let response = '';
    client.on('data', (chunk) => {{
      response += chunk.toString();
      if (response.includes('\n')) {{
        client.end();
        try {{
          resolve(JSON.parse(response.trim()));
        }} catch (e) {{
          resolve({{}});
        }}
      }}
    }});
    client.on('error', () => resolve({{}}));
    client.on('close', () => resolve({{}}));
    setTimeout(() => {{ client.destroy(); resolve({{}}); }}, 5000);
  }});
}}

function sendNonBlocking(payload) {{
  try {{
    const client = net.createConnection(SOCKET_PATH, () => {{
      client.write(JSON.stringify(payload) + '\n');
      setTimeout(() => client.end(), 100);
    }});
    client.on('error', () => {{}});
  }} catch (e) {{}}
}}

module.exports = {{
  name: 'vibe-island',

  async onSessionStart(context) {{
    sendNonBlocking({{
      session_id: getSessionId(),
      agent: 'opencode',
      cwd: context.cwd || process.cwd(),
      term_program: process.env.TERM_PROGRAM || '',
      iterm_session_id: process.env.ITERM_SESSION_ID || '',
      tmux_pane: process.env.TMUX_PANE || '',
      tty: '',
    }});
  }},

  async onSessionEnd() {{
    sendNonBlocking({{
      session_id: getSessionId(),
      agent: 'opencode',
      stop_hook_active: true,
    }});
  }},

  async onBeforeTool(context) {{
    const resp = await sendEvent({{
      session_id: getSessionId(),
      agent: 'opencode',
      tool_name: context.tool || 'Unknown',
      tool_input: context.input || {{}},
      cwd: context.cwd || process.cwd(),
      term_program: process.env.TERM_PROGRAM || '',
      iterm_session_id: process.env.ITERM_SESSION_ID || '',
      tmux_pane: process.env.TMUX_PANE || '',
    }});
    if (resp && resp.blocked) {{
      throw new Error(resp.reason || 'Blocked by Vibe Island');
    }}
  }},

  async onAfterTool(context) {{
    sendNonBlocking({{
      session_id: getSessionId(),
      agent: 'opencode',
      tool_name: context.tool || 'Unknown',
      tool_response: context.result || {{}},
      cwd: context.cwd || process.cwd(),
    }});
  }},
}};
"#,
        socket_path_repr = serde_json::to_string(&sock.to_string_lossy()).unwrap()
    );

    let existing = std::fs::read_to_string(&plugin_path).unwrap_or_default();
    if existing != plugin_code {
        std::fs::write(&plugin_path, plugin_code).map_err(|e| e.to_string())?;
        info!("OpenCode plugin written to {:?}", plugin_path);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_session() -> Session {
        Session {
            id: "cli-12345".to_string(),
            agent: "claude-code".to_string(),
            title: "test-project".to_string(),
            cwd: "/Users/test/project".to_string(),
            status: "running".to_string(),
            terminal: "iTerm2".to_string(),
            tab_id: "tab-cli-12345".to_string(),
            started_at: 1000,
            last_activity: 1100,
            term_program: "iTerm.app".to_string(),
            iterm_session_id: "w0t0p0:ABC-123".to_string(),
            tmux_pane: String::new(),
            tty: "/dev/ttys003".to_string(),
        }
    }

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
        let session = test_session();
        let json = serde_json::to_string(&session).unwrap();
        assert!(json.contains("\"tabId\":\"tab-cli-12345\""));
        assert!(json.contains("\"startedAt\":1000"));
        assert!(json.contains("\"lastActivity\":1100"));
        assert!(json.contains("\"termProgram\":\"iTerm.app\""));
        assert!(json.contains("\"itermSessionId\":\"w0t0p0:ABC-123\""));
        assert!(json.contains("\"tty\":\"/dev/ttys003\""));
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
        assert_eq!(session.term_program, "");
        assert_eq!(session.iterm_session_id, "");
        assert_eq!(session.tmux_pane, "");
        assert_eq!(session.tty, "");
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
    fn test_bridge_path() {
        let path = bridge_path();
        assert!(path.to_string_lossy().contains("vibe-island"));
        assert!(path.to_string_lossy().ends_with("vibe-bridge"));
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
            "cwd": "/Users/test",
            "term_program": "iTerm.app",
            "iterm_session_id": "w0t0p0:DEF-456",
            "tmux_pane": "%1",
            "tty": "/dev/ttys005"
        }"#;

        let input: ClaudeHookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.session_id, "cli-123");
        assert_eq!(input.agent, Some("claude-code".to_string()));
        assert_eq!(input.tool_name, Some("Write".to_string()));
        assert!(input.tool_input.is_some());
        assert!(input.tool_response.is_none());
        assert!(input.stop_hook_active.is_none());
        assert_eq!(input.term_program, "iTerm.app");
        assert_eq!(input.iterm_session_id, "w0t0p0:DEF-456");
        assert_eq!(input.tmux_pane, "%1");
        assert_eq!(input.tty, "/dev/ttys005");
    }

    #[test]
    fn test_claude_hook_input_stop_event() {
        let json = r#"{
            "session_id": "cli-456",
            "stop_hook_active": true
        }"#;

        let input: ClaudeHookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.stop_hook_active, Some(true));
        assert_eq!(input.term_program, "");
        assert_eq!(input.iterm_session_id, "");
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

    #[test]
    fn test_detect_terminal_from_env_hook() {
        let hook = ClaudeHookInput {
            session_id: "test".to_string(),
            agent: None,
            tool_name: None,
            tool_input: None,
            tool_response: None,
            stop_hook_active: None,
            message: None,
            cwd: None,
            term_program: "Ghostty".to_string(),
            iterm_session_id: String::new(),
            tmux_pane: String::new(),
            tty: String::new(),
        };
        assert_eq!(detect_terminal_from_env(&hook), "Ghostty");
    }

    #[test]
    fn test_detect_terminal_from_env_empty() {
        std::env::set_var("TERM_PROGRAM", "iTerm.app");
        let hook = ClaudeHookInput {
            session_id: "test".to_string(),
            agent: None,
            tool_name: None,
            tool_input: None,
            tool_response: None,
            stop_hook_active: None,
            message: None,
            cwd: None,
            term_program: String::new(),
            iterm_session_id: String::new(),
            tmux_pane: String::new(),
            tty: String::new(),
        };
        assert_eq!(detect_terminal_from_env(&hook), "iTerm2");
    }
}
