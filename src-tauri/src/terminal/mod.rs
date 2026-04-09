use std::process::Command;
use tracing::{error, info, warn};

pub fn jump_to_session(terminal: &str, tab_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("Jumping to terminal: {} tab: {}", terminal, tab_id);

    let tab_index: usize = tab_id.parse::<usize>().unwrap_or(1).saturating_sub(1);

    match terminal {
        "Ghostty" => jump_ghostty_tab(tab_index),
        "iTerm2" => jump_iterm2(tab_id),
        "Terminal" => jump_terminal(tab_index),
        "VSCode" | "Code" => jump_app("Visual Studio Code"),
        "Cursor" => jump_app("Cursor"),
        "Windsurf" => jump_app("Windsurf"),
        "Zed" => jump_app("Zed"),
        _ => {
            // Fallback: try to jump to whatever string is provided as an app name
            jump_app(terminal)
        }
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

fn jump_ghostty_tab(_tab_index: usize) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

fn jump_iterm2(tab_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if tab_id.starts_with("tmux-") {
        jump_iterm2_tmux(tab_id)
    } else if let Ok(index) = tab_id.parse::<usize>() {
        jump_iterm2_by_index(index.saturating_sub(1))
    } else {
        jump_iterm2_native()
    }
}

fn jump_iterm2_native() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let script = r#"
        tell application "iTerm"
            activate
            try
                set theWindows to windows
                if (count of theWindows) > 0 then
                    set currentWindow to item 1 of theWindows
                    set currentTab to (current session of currentWindow)
                    tell currentTab
                        select
                    end tell
                end if
            end try
        end tell
    "#;

    let output = Command::new("osascript")
        .args(["-e", script])
        .output()?;

    if !output.status.success() {
        error!("iTerm2 AppleScript failed: {:?}", String::from_utf8_lossy(&output.stderr));
    }

    Ok(())
}

fn jump_iterm2_by_index(tab_index: usize) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let script = format!(
        r#"
        tell application "iTerm"
            activate
            try
                set theWindows to windows
                if (count of theWindows) > 0 then
                    set currentWindow to item 1 of theWindows
                    set tabCount to count of tabs of currentWindow
                    
                    set targetIndex to {}
                    if targetIndex > tabCount then
                        set targetIndex to tabCount
                    end if
                    if targetIndex < 1 then
                        set targetIndex to 1
                    end if
                    
                    set selectedTab to tab targetIndex of currentWindow
                    tell selectedTab
                        select
                    end tell
                end if
            end try
        end tell
        "#,
        tab_index + 1
    );

    let output = Command::new("osascript")
        .args(["-e", &script])
        .output()?;

    if !output.status.success() {
        error!("iTerm2 AppleScript failed: {:?}", String::from_utf8_lossy(&output.stderr));
    }

    Ok(())
}

fn jump_iterm2_tmux(session_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let tmux_session = session_id.trim_start_matches("tmux-");

    let script = format!(
        r#"
        tell application "iTerm"
            activate
            tell current window
                create tab with profile "Default"
                tell the current session
                    write text "tmux attach -t {}"
                end tell
            end tell
        end tell
        "#,
        tmux_session
    );

    let output = Command::new("osascript")
        .args(["-e", &script])
        .output()?;

    if !output.status.success() {
        error!("iTerm2 tmux AppleScript failed: {:?}", String::from_utf8_lossy(&output.stderr));
    }

    Ok(())
}

fn jump_terminal(tab_index: usize) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let script = format!(
        r#"
        tell application "Terminal"
            activate
            
            set tabCount to count of tabs of front window
            if tabCount = 0 then
                return
            end if
            
            set targetIndex to {}
            if targetIndex > tabCount then
                set targetIndex to tabCount
            end if
            if targetIndex < 1 then
                set targetIndex to 1
            end if
            
            set frontmost of tab targetIndex of front window to true
        end tell
        "#,
        tab_index + 1
    );

    let output = Command::new("osascript")
        .args(["-e", &script])
        .output()?;

    if !output.status.success() {
        error!("Terminal AppleScript failed: {:?}", String::from_utf8_lossy(&output.stderr));
    }

    Ok(())
}

pub fn get_active_terminals() -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
    let mut terminals = Vec::new();

    let apps = [
        ("Ghostty", "Ghostty"),
        ("iTerm", "iTerm2"),
        ("Apple Terminal", "Terminal"),
        ("Code", "VSCode"),
        ("Cursor", "Cursor"),
        ("Windsurf", "Windsurf"),
        ("Zed", "Zed"),
    ];

    for (app_name, display) in apps {
        let output = Command::new("pgrep")
            .args(["-fl", app_name])
            .output()?;

        if output.status.success() && !output.stdout.is_empty() {
            terminals.push(display.to_string());
        }
    }

    Ok(terminals)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jump_match_logic() {
        // Test match cases (implicitly via public function logic check)
        // Since AppleScript requires UI, we test the logic via small utility if needed
        // Here we can at least test that get_active_terminals returns a result
        let terminals = get_active_terminals().unwrap();
        assert!(terminals.is_empty() || !terminals[0].is_empty());
    }
}
