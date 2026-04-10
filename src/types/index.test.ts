import { describe, it, expect } from "vitest";
import type { Session, PermissionRequest, IPCEvent } from "../types";

describe("Session type", () => {
  const baseSession: Session = {
    id: "cli-12345",
    agent: "claude-code",
    title: "my-project",
    cwd: "/Users/test/my-project",
    status: "running",
    terminal: "iTerm2",
    tabId: "tab-cli-12345",
    startedAt: 1000000,
    lastActivity: 1000100,
  };

  it("accepts valid running session", () => {
    expect(baseSession.status).toBe("running");
    expect(baseSession.agent).toBe("claude-code");
  });

  it("accepts waiting status", () => {
    const session: Session = { ...baseSession, status: "waiting" };
    expect(session.status).toBe("waiting");
  });

  it("accepts completed status", () => {
    const session: Session = { ...baseSession, status: "completed" };
    expect(session.status).toBe("completed");
  });

  it("has all required fields", () => {
    const keys = Object.keys(baseSession);
    expect(keys).toContain("id");
    expect(keys).toContain("agent");
    expect(keys).toContain("title");
    expect(keys).toContain("cwd");
    expect(keys).toContain("status");
    expect(keys).toContain("terminal");
    expect(keys).toContain("tabId");
    expect(keys).toContain("startedAt");
    expect(keys).toContain("lastActivity");
  });
});

describe("PermissionRequest type", () => {
  const baseRequest: PermissionRequest = {
    id: "req-1",
    sessionId: "cli-12345",
    type: "tool_use",
    message: "Allow file write?",
    timestamp: 1000200,
  };

  it("accepts tool_use type", () => {
    expect(baseRequest.type).toBe("tool_use");
  });

  it("accepts ask_user type", () => {
    const req: PermissionRequest = { ...baseRequest, type: "ask_user" };
    expect(req.type).toBe("ask_user");
  });

  it("accepts plan_approval type", () => {
    const req: PermissionRequest = { ...baseRequest, type: "plan_approval" };
    expect(req.type).toBe("plan_approval");
  });

  it("supports optional fields", () => {
    const req: PermissionRequest = {
      ...baseRequest,
      toolName: "Write",
      options: ["Allow once", "Allow always"],
    };
    expect(req.toolName).toBe("Write");
    expect(req.options).toHaveLength(2);
  });
});

describe("IPCEvent type", () => {
  const baseSession: Session = {
    id: "cli-12345",
    agent: "claude-code",
    title: "test",
    cwd: "/test",
    status: "running",
    terminal: "iTerm2",
    tabId: "tab-1",
    startedAt: 1000,
    lastActivity: 1000,
  };

  it("handles session_started event", () => {
    const event: IPCEvent = { type: "session_started", session: baseSession };
    expect(event.type).toBe("session_started");
    if (event.type === "session_started") {
      expect(event.session.id).toBe("cli-12345");
    }
  });

  it("handles session_updated event", () => {
    const event: IPCEvent = { type: "session_updated", session: { ...baseSession, status: "waiting" } };
    expect(event.type).toBe("session_updated");
    if (event.type === "session_updated") {
      expect(event.session.status).toBe("waiting");
    }
  });

  it("handles session_ended event", () => {
    const event: IPCEvent = { type: "session_ended", session_id: "cli-12345" };
    expect(event.type).toBe("session_ended");
    if (event.type === "session_ended") {
      expect(event.session_id).toBe("cli-12345");
    }
  });

  it("handles permission_requested event", () => {
    const event: IPCEvent = {
      type: "permission_requested",
      request: {
        id: "req-1",
        sessionId: "cli-12345",
        type: "tool_use",
        message: "Allow?",
        timestamp: 1000,
      },
    };
    expect(event.type).toBe("permission_requested");
  });

  it("handles permission_approved event", () => {
    const event: IPCEvent = { type: "permission_approved", request_id: "req-1" };
    expect(event.type).toBe("permission_approved");
  });

  it("handles permission_denied event", () => {
    const event: IPCEvent = { type: "permission_denied", request_id: "req-1" };
    expect(event.type).toBe("permission_denied");
  });

  it("handles plan_review event", () => {
    const event: IPCEvent = { type: "plan_review", session_id: "cli-1", plan: "do stuff" };
    expect(event.type).toBe("plan_review");
  });

  it("handles terminal_jumped event", () => {
    const event: IPCEvent = { type: "terminal_jumped", tab_id: "tab-1" };
    expect(event.type).toBe("terminal_jumped");
  });
});
