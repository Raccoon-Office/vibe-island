import { useIPC } from "./hooks/useIPC";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useState, useEffect } from "react";
import type { Session } from "./types";

function PixelKnight({ status, agent }: { status: string; agent: string }) {
  return (
    <div className={`pixel-knight ${status} agent-${agent}`}>
      <div className="knight-body" />
      <div className="knight-sword" />
    </div>
  );
}

function SessionList({
  sessions,
  onJump,
}: {
  sessions: Session[];
  onJump: (sessionId: string) => void;
}) {
  if (sessions.length === 0) {
    return (
      <div className="session-empty" style={{ padding: "16px", textAlign: "center", color: "rgba(255,255,255,0.5)" }}>
        No active sessions
      </div>
    );
  }

  const getDirectoryName = (cwd: string) => {
    if (!cwd) return "";
    const parts = cwd.split(/[/\\]/);
    return parts.filter(Boolean).pop() || cwd;
  };

  const getAgentAbbreviation = (agent: string) => {
    const map: Record<string, string> = {
      "claude-code": "CC",
      "opencode": "OC",
      "gemini": "GE",
      "codex": "CX",
      "cursor": "CR",
    };
    return map[agent.toLowerCase()] || agent.substring(0, 2).toUpperCase();
  };

  return (
    <div className="session-list scrollbar-hide">
      {sessions.map((session) => (
        <div
          key={session.id}
          className={`session-item status-${session.status} agent-${session.agent}`}
          onClick={() => onJump(session.id)}
        >
          <div className="session-header">
            <div className="session-agent-tag">{getAgentAbbreviation(session.agent)}</div>
            <div className="session-cwd-tag">{getDirectoryName(session.cwd)}</div>
          </div>
          <div className="session-icon">
            <PixelKnight status={session.status} agent={session.agent} />
          </div>
          <div className="session-info">
            <div className="session-title" title={session.title}>{session.title}</div>
          </div>
        </div>
      ))}
    </div>
  );
}

export default function App() {
  const { sessions, jumpToTerminal } = useIPC();
  const appWindow = getCurrentWindow();

  const [theme, setTheme] = useState<"default" | "forest">(() => {
    return (localStorage.getItem("vibe-theme") as "default" | "forest") || "default";
  });

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
    localStorage.setItem("vibe-theme", theme);
  }, [theme]);

  const handleMinimize = () => { appWindow.minimize(); };
  const handleClose = () => { appWindow.close(); };
  const toggleTheme = () => { setTheme(t => t === "default" ? "forest" : "default"); };

  return (
    <div className="dynamic-island">
      <div className="island-expanded">
        <div className="drag-region" data-tauri-drag-region>
          <div className="app-brand" data-tauri-drag-region>
            <svg className="app-icon" width="18" height="18" viewBox="0 0 24 24" fill="none">
              <path d="M12 2L2 7l10 5 10-5-10-5z" fill="currentColor" opacity="0.8"/>
              <path d="M2 17l10 5 10-5" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" opacity="0.5"/>
              <path d="M2 12l10 5 10-5" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" opacity="0.65"/>
            </svg>
            <span className="app-name">Vibe Island</span>
          </div>
          <div className="header-actions">
            <button className="control-btn btn-theme" onClick={toggleTheme} title="Switch theme" data-tauri-drag-region={false}>
              {theme === "default" ? (
                <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M8 1a1 1 0 010 2 5 5 0 000 10 1 1 0 010 2A7 7 0 118 1z"/></svg>
              ) : (
                <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><circle cx="8" cy="8" r="3.5"/><path d="M8 1v2M8 13v2M1 8h2M13 8h2M3.05 3.05l1.41 1.41M11.54 11.54l1.41 1.41M3.05 12.95l1.41-1.41M11.54 4.46l1.41-1.41" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/></svg>
              )}
            </button>
            <button className="control-btn btn-minimize" onClick={handleMinimize} title="Minimize" data-tauri-drag-region={false}>
              <svg width="12" height="12" viewBox="0 0 12 12"><rect x="2" y="5.5" width="8" height="1" fill="currentColor"/></svg>
            </button>
            <button className="control-btn btn-close" onClick={handleClose} title="Close" data-tauri-drag-region={false}>
              <svg width="12" height="12" viewBox="0 0 12 12"><path d="M2.5 2.5L9.5 9.5M9.5 2.5L2.5 9.5" stroke="currentColor" strokeWidth="1.2"/></svg>
            </button>
          </div>
        </div>
        <SessionList sessions={sessions} onJump={jumpToTerminal} />
      </div>
    </div>
  );
}
