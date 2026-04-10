import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import type { IPCEvent } from "../types";

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(vi.fn()),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { useIPC } from "../hooks/useIPC";

const mockListen = vi.mocked(listen);
const mockInvoke = vi.mocked(invoke);

const baseSession = {
  id: "cli-12345",
  agent: "claude-code",
  title: "my-project",
  cwd: "/Users/test/my-project",
  status: "running" as const,
  terminal: "iTerm2",
  tabId: "tab-cli-12345",
  startedAt: 1000,
  lastActivity: 1000,
};

function createListenerSetup() {
  let listenerCallback: (event: { payload: IPCEvent }) => void = () => {};
  const unlistenFn = vi.fn();

  mockListen.mockImplementation((_event: string, cb: (e: { payload: IPCEvent; event: string; id: number }) => void) => {
    listenerCallback = cb as typeof listenerCallback;
    return Promise.resolve(unlistenFn);
  });

  return {
    emit: (payload: IPCEvent) => listenerCallback({ payload }),
    unlistenFn,
  };
}

describe("useIPC", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("starts with empty sessions", () => {
    mockListen.mockResolvedValue(vi.fn());
    const { result } = renderHook(() => useIPC());
    expect(result.current.sessions).toEqual([]);
    expect(result.current.pendingPermissions).toEqual([]);
  });

  it("registers ipc-event listener on mount", () => {
    mockListen.mockResolvedValue(vi.fn());
    renderHook(() => useIPC());
    expect(mockListen).toHaveBeenCalledWith("ipc-event", expect.any(Function));
  });

  it("adds session on session_started event", () => {
    const { emit } = createListenerSetup();
    const { result } = renderHook(() => useIPC());

    act(() => {
      emit({ type: "session_started", session: baseSession });
    });

    expect(result.current.sessions).toHaveLength(1);
    expect(result.current.sessions[0].id).toBe("cli-12345");
  });

  it("updates existing session on session_updated event", () => {
    const { emit } = createListenerSetup();
    const { result } = renderHook(() => useIPC());

    act(() => {
      emit({ type: "session_started", session: baseSession });
    });

    act(() => {
      emit({
        type: "session_updated",
        session: { ...baseSession, status: "waiting", title: "Write file.ts" },
      });
    });

    expect(result.current.sessions).toHaveLength(1);
    expect(result.current.sessions[0].status).toBe("waiting");
    expect(result.current.sessions[0].title).toBe("Write file.ts");
  });

  it("adds new session on session_updated if not found", () => {
    const { emit } = createListenerSetup();
    const { result } = renderHook(() => useIPC());

    act(() => {
      emit({ type: "session_updated", session: baseSession });
    });

    expect(result.current.sessions).toHaveLength(1);
    expect(result.current.sessions[0].id).toBe("cli-12345");
  });

  it("removes session on session_ended event", () => {
    const { emit } = createListenerSetup();
    const { result } = renderHook(() => useIPC());

    act(() => {
      emit({ type: "session_started", session: baseSession });
    });
    expect(result.current.sessions).toHaveLength(1);

    act(() => {
      emit({ type: "session_ended", session_id: "cli-12345" });
    });
    expect(result.current.sessions).toHaveLength(0);
  });

  it("handles multiple sessions", () => {
    const { emit } = createListenerSetup();
    const { result } = renderHook(() => useIPC());

    act(() => {
      emit({ type: "session_started", session: baseSession });
      emit({
        type: "session_started",
        session: { ...baseSession, id: "cli-67890", agent: "opencode" },
      });
    });

    expect(result.current.sessions).toHaveLength(2);
  });

  it("only removes the specific session on session_ended", () => {
    const { emit } = createListenerSetup();
    const { result } = renderHook(() => useIPC());

    act(() => {
      emit({ type: "session_started", session: baseSession });
      emit({
        type: "session_started",
        session: { ...baseSession, id: "cli-67890", agent: "gemini" },
      });
    });

    act(() => {
      emit({ type: "session_ended", session_id: "cli-12345" });
    });

    expect(result.current.sessions).toHaveLength(1);
    expect(result.current.sessions[0].id).toBe("cli-67890");
  });

  it("adds permission on permission_requested event", () => {
    const { emit } = createListenerSetup();
    const { result } = renderHook(() => useIPC());

    act(() => {
      emit({
        type: "permission_requested",
        request: {
          id: "req-1",
          sessionId: "cli-12345",
          type: "tool_use",
          message: "Allow write?",
          timestamp: 2000,
        },
      });
    });

    expect(result.current.pendingPermissions).toHaveLength(1);
    expect(result.current.pendingPermissions[0].id).toBe("req-1");
  });

  it("removes permission on permission_approved event", () => {
    const { emit } = createListenerSetup();
    const { result } = renderHook(() => useIPC());

    act(() => {
      emit({
        type: "permission_requested",
        request: {
          id: "req-1",
          sessionId: "cli-12345",
          type: "tool_use",
          message: "Allow write?",
          timestamp: 2000,
        },
      });
    });

    act(() => {
      emit({ type: "permission_approved", request_id: "req-1" });
    });

    expect(result.current.pendingPermissions).toHaveLength(0);
  });

  it("removes permission on permission_denied event", () => {
    const { emit } = createListenerSetup();
    const { result } = renderHook(() => useIPC());

    act(() => {
      emit({
        type: "permission_requested",
        request: {
          id: "req-1",
          sessionId: "cli-12345",
          type: "tool_use",
          message: "Allow write?",
          timestamp: 2000,
        },
      });
    });

    act(() => {
      emit({ type: "permission_denied", request_id: "req-1" });
    });

    expect(result.current.pendingPermissions).toHaveLength(0);
  });

  it("calls invoke on sendResponse", async () => {
    mockListen.mockResolvedValue(vi.fn());
    mockInvoke.mockResolvedValue(undefined);

    const { result } = renderHook(() => useIPC());

    await act(async () => {
      await result.current.sendResponse("req-1", true);
    });

    expect(mockInvoke).toHaveBeenCalledWith("send_permission_response", {
      requestId: "req-1",
      approved: true,
      response: undefined,
    });
  });

  it("calls invoke on jumpToTerminal", async () => {
    mockListen.mockResolvedValue(vi.fn());
    mockInvoke.mockResolvedValue(undefined);

    const { result } = renderHook(() => useIPC());

    await act(async () => {
      await result.current.jumpToTerminal("cli-12345");
    });

    expect(mockInvoke).toHaveBeenCalledWith("jump_to_terminal", {
      session_id: "cli-12345",
    });
  });

  it("registers cleanup function that calls unlisten", async () => {
    const unlistenFn = vi.fn();
    mockListen.mockResolvedValue(unlistenFn);

    renderHook(() => useIPC());

    await act(async () => {
      await new Promise((r) => setTimeout(r, 10));
    });

    expect(mockListen).toHaveBeenCalledWith("ipc-event", expect.any(Function));
  });
});
