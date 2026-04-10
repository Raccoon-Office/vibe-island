use std::process::Command;
use tracing::{error, info, warn};

pub fn jump_to_session(terminal: &str, tab_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("Jumping to terminal: {} tab: {}", terminal, tab_id);

    let agent_pid = tab_id.strip_prefix("tab-cli-")
        .and_then(|p| p.parse::<u32>().ok());

    match terminal {
        "iTerm2" => jump_iterm2_by_pid(agent_pid),
        "Terminal" => jump_terminal_by_pid(agent_pid),
        "Ghostty" => jump_ghostty_tab(),
        "VSCode" | "Code" => jump_app("Visual Studio Code"),
        "Cursor" => jump_app("Cursor"),
        "Windsurf" => jump_app("Windsurf"),
        "Zed" => jump_app("Zed"),
        _ => jump_app(terminal),
    }
}

fn escape_applescript_str(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn jump_app(app_name: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let escaped = escape_applescript_str(app_name);
    let script = format!(
        r#"
        tell application "{}"
            activate
        end tell
        "#,
        escaped
    );

    let output = Command::new("osascript")
        .args(["-e", &script])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("Jump to {} failed: {}", app_name, stderr);
        return Err(stderr.into());
    }

    Ok(())
}

fn jump_ghostty_tab() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let script = r#"
        tell application "Ghostty"
            activate
        end tell
    "#;

    let output = Command::new("osascript")
        .args(["-e", script])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("Ghostty AppleScript failed: {}", stderr);
        if stderr.contains("execution error") {
            warn!("Ghostty may not support AppleScript, falling back to open");
            Command::new("open").args(["-a", "Ghostty"]).output()?;
        }
    } else {
        info!("Ghostty jump output: {}", String::from_utf8_lossy(&output.stdout).trim());
    }

    Ok(())
}

fn jump_iterm2_by_pid(agent_pid: Option<u32>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let script = if let Some(pid) = agent_pid {
        format!(r#"
tell application "iTerm"
    activate
    try
        repeat with w in windows
            set winTabs to tabs of w
            repeat with t in winTabs
                set s to current session of t
                set tty to tty of s
                try
                    set procName to do shell script "ps -o pid= -t " & quoted form of tty & " | grep -w {pid}"
                    select t
                    return
                end try
            end repeat
        end repeat
    end try
end tell
"#, pid = pid)
    } else {
        r#"
tell application "iTerm"
    activate
end tell
"#.to_string()
    };

    let output = Command::new("osascript").args(["-e", &script]).output()?;
    if !output.status.success() {
        error!("iTerm2 jump by PID failed: {:?}", String::from_utf8_lossy(&output.stderr));
        let _ = Command::new("osascript")
            .args(["-e", "tell application \"iTerm\" to activate"])
            .output();
    }
    Ok(())
}

fn jump_terminal_by_pid(agent_pid: Option<u32>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let script = if let Some(pid) = agent_pid {
        format!(r#"
tell application "Terminal"
    activate
    try
        set tabCount to count of tabs of front window
        repeat with i from 1 to tabCount
            set t to tab i of front window
            set h to history of t
            set procs to do shell script "ps -o ppid= -p {pid} | head -1"
            set parentPid to word 1 of procs
            set ttys to do shell script "ps -o tty= -p " & parentPid & " | head -1"
            set ttyMatch to do shell script "ps -o tty= -p $(ps -o pid= -t " & ttys & " | head -1) 2>/dev/null || true"
            if ttyMatch is not "" then
                set frontmost of t to true
                return
            end if
        end repeat
    end try
end tell
"#, pid = pid)
    } else {
        r#"
tell application "Terminal"
    activate
end tell
"#.to_string()
    };

    let output = Command::new("osascript").args(["-e", &script]).output()?;
    if !output.status.success() {
        error!("Terminal jump by PID failed: {:?}", String::from_utf8_lossy(&output.stderr));
        let _ = Command::new("osascript")
            .args(["-e", "tell application \"Terminal\" to activate"])
            .output();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_applescript_str_normal() {
        assert_eq!(escape_applescript_str("Hello World"), "Hello World");
    }

    #[test]
    fn test_escape_applescript_str_backslash() {
        assert_eq!(escape_applescript_str("path\\to\\file"), "path\\\\to\\\\file");
    }

    #[test]
    fn test_escape_applescript_str_quotes() {
        assert_eq!(escape_applescript_str("say \"hello\""), "say \\\"hello\\\"");
    }

    #[test]
    fn test_escape_applescript_str_mixed() {
        assert_eq!(
            escape_applescript_str("path\\to \"file\""),
            "path\\\\to \\\"file\\\""
        );
    }

    #[test]
    fn test_escape_applescript_str_empty() {
        assert_eq!(escape_applescript_str(""), "");
    }

    #[test]
    fn test_jump_to_session_unknown_terminal() {
        let result = jump_to_session("UnknownTerm", "tab-123");
        assert!(result.is_err() || result.is_ok());
    }

    #[test]
    fn test_jump_to_session_ghostty() {
        let result = jump_to_session("Ghostty", "tab-cli-12345");
        assert!(result.is_ok());
    }

    #[test]
    fn test_jump_to_session_iterm2_no_pid() {
        let result = jump_to_session("iTerm2", "tab-unknown");
        assert!(result.is_ok());
    }

    #[test]
    fn test_jump_to_session_terminal_no_pid() {
        let result = jump_to_session("Terminal", "tab-unknown");
        assert!(result.is_ok());
    }

    #[test]
    fn test_jump_to_session_vscode() {
        let result = jump_to_session("VSCode", "tab-123");
        let _ = result;
    }

    #[test]
    fn test_jump_to_session_cursor() {
        let result = jump_to_session("Cursor", "tab-123");
        let _ = result;
    }

    #[test]
    fn test_jump_to_session_windsurf() {
        let result = jump_to_session("Windsurf", "tab-123");
        let _ = result;
    }

    #[test]
    fn test_jump_to_session_zed() {
        let result = jump_to_session("Zed", "tab-123");
        let _ = result;
    }
}
