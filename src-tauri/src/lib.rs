#![allow(unexpected_cfgs)]
mod ipc;
mod claude;
mod terminal;

use std::collections::HashMap;
use std::sync::Arc;
use tauri::{Manager, State};
use tauri::WebviewWindow;
use tokio::net::unix::OwnedWriteHalf;
use tokio::sync::Mutex;
use tokio::io::AsyncWriteExt;
use tracing::{error, info, Level};
use tracing_subscriber::fmt::format::FmtSpan;

pub struct AppState {
    pub sessions: Arc<Mutex<Vec<claude::Session>>>,
    pub pending_permissions: Arc<Mutex<Vec<claude::PermissionRequest>>>,
    // TODO: Wire into handle_connection when PreToolUse approval UI is implemented.
    // Currently PreToolUse always auto-approves.
    pub pending_connections: Arc<Mutex<HashMap<String, OwnedWriteHalf>>>,
}

fn setup_logging() {
    let log_dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("VibeIsland")
        .join("logs");

    std::fs::create_dir_all(&log_dir).ok();

    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_span_events(FmtSpan::CLOSE)
        .with_writer(std::io::stdout)
        .init();
}

#[tauri::command]
async fn send_permission_response(
    state: State<'_, AppState>,
    request_id: String,
    approved: bool,
    _response: Option<String>,
) -> Result<(), String> {
    info!(
        "Permission {} for request: {}",
        if approved { "approved" } else { "denied" },
        request_id
    );

    // Remove from pending list.
    state.pending_permissions.lock().await.retain(|p| p.id != request_id);

    // Write result back to the blocked hook process.
    if let Some(mut writer) = state.pending_connections.lock().await.remove(&request_id) {
        let payload: &[u8] = if approved {
            b"{}\n"
        } else {
            b"{\"blocked\":true,\"reason\":\"Denied by user\"}\n"
        };
        let _ = writer.write_all(payload).await;
        let _ = writer.flush().await;
    }

    Ok(())
}

#[tauri::command]
async fn jump_to_terminal(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    info!("Jumping to terminal for session: {}", session_id);

    let sessions = state.sessions.lock().await;
    let session = sessions
        .iter()
        .find(|s| s.id == session_id)
        .ok_or("Session not found")?;

    terminal::jump_to_session(&session.terminal, &session.tab_id)
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
async fn get_sessions(state: State<'_, AppState>) -> Result<Vec<claude::Session>, String> {
    Ok(state.sessions.lock().await.clone())
}

pub fn run() {
    setup_logging();

    info!("Starting Vibe Island");

    let app_state = AppState {
        sessions: Arc::new(Mutex::new(Vec::new())),
        pending_permissions: Arc::new(Mutex::new(Vec::new())),
        pending_connections: Arc::new(Mutex::new(HashMap::new())),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            send_permission_response,
            jump_to_terminal,
            get_sessions,
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            #[cfg(target_os = "macos")]
            if let Some(window) = app.get_webview_window("main") {
                let window: WebviewWindow = window;
                let _ = window.set_background_color(Some(tauri::window::Color(0, 0, 0, 0)));
                let _ = window.set_shadow(false);

                #[cfg(target_os = "macos")]
                #[allow(deprecated)]
                {
                    use cocoa::base::{id, YES, NO};
                    use objc::{class, msg_send, sel, sel_impl};

                    let ns_window: id = window.ns_window().unwrap() as id;
                    #[allow(deprecated)]
                    unsafe {
                        let bg: id = msg_send![class!(NSColor), colorWithCalibratedRed:0.0 green:0.0 blue:0.0 alpha:0.0];
                        let _: () = msg_send![ns_window, setOpaque: NO];
                        let _: () = msg_send![ns_window, setBackgroundColor: bg];
                        let _: () = msg_send![ns_window, setHasShadow: NO];

                        let content_view: id = msg_send![ns_window, contentView];
                        let _: () = msg_send![content_view, setWantsLayer: YES];
                        let cl: id = msg_send![content_view, layer];
                        let _: () = msg_send![cl, setCornerRadius: 24.0];
                        let _: () = msg_send![cl, setMasksToBounds: YES];
                    }
                }

                if let Ok(Some(monitor)) = window.primary_monitor() {
                    let screen_size = monitor.size();
                    let scale = monitor.scale_factor();
                    let win_w: f64 = 340.0;
                    let x = (screen_size.width as f64 / scale) - win_w - 16.0;
                    let y = 48.0;
                    let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(x, y)));
                }
            }

            tauri::async_runtime::spawn(async move {
                if let Err(e) = claude::start_bridge(handle.clone()).await {
                    error!("Claude bridge error: {}", e);
                }
            });



            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

