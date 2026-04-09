import { useIPC } from "./hooks/useIPC";
import { getCurrentWindow } from "@tauri-apps/api/window";
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

  const handleDragStart = () => {
    getCurrentWindow().startDragging();
  };

  const handleMinimize = () => {
    getCurrentWindow().minimize();
  };

  const handleClose = () => {
    getCurrentWindow().close();
  };

  return (
    <div className="dynamic-island">
      <div className="island-expanded">
        <div className="drag-region" onMouseDown={handleDragStart}>
          <div className="window-controls no-drag">
            <button className="control-btn btn-minimize" onClick={handleMinimize} title="Minimize">
              <svg width="12" height="12" viewBox="0 0 12 12"><rect x="2" y="5.5" width="8" height="1" fill="currentColor"/></svg>
            </button>
            <button className="control-btn btn-close" onClick={handleClose} title="Close">
              <svg width="12" height="12" viewBox="0 0 12 12"><path d="M2.5 2.5L9.5 9.5M9.5 2.5L2.5 9.5" stroke="currentColor" strokeWidth="1.2"/></svg>
            </button>
          </div>
        </div>
        <SessionList sessions={sessions} onJump={jumpToTerminal} />
      </div>
    </div>
  );
}
