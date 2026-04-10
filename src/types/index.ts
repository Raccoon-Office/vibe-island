export interface Session {
  id: string;
  agent: string;
  title: string;
  cwd: string;
  status: "running" | "waiting" | "completed";
  terminal: string;
  tabId: string;
  startedAt: number;
  lastActivity: number;
  termProgram: string;
  itermSessionId: string;
  tmuxPane: string;
  tty: string;
}

export interface PermissionRequest {
  id: string;
  sessionId: string;
  type: "tool_use" | "ask_user" | "plan_approval";
  toolName?: string;
  message: string;
  options?: string[];
  timestamp: number;
}

export type IPCEvent =
  | { type: "session_started"; session: Session }
  | { type: "session_updated"; session: Session }
  | { type: "session_ended"; session_id: string }
  | { type: "permission_requested"; request: PermissionRequest }
  | { type: "permission_approved"; request_id: string }
  | { type: "permission_denied"; request_id: string }
  | { type: "plan_review"; session_id: string; plan: string }
  | { type: "terminal_jumped"; tab_id: string };
