import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const mockMinimize = vi.fn();
const mockClose = vi.fn();

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(vi.fn()),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    minimize: mockMinimize,
    close: mockClose,
  }),
}));

import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import App from "./App";
import type { IPCEvent } from "./types";

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

let ipcCallback: ((event: { payload: IPCEvent }) => void) | null = null;

function setupMock() {
  ipcCallback = null;
  mockListen.mockImplementation((_event: string, cb: (e: { payload: IPCEvent; event: string; id: number }) => void) => {
    ipcCallback = cb as typeof ipcCallback;
    return Promise.resolve(vi.fn());
  });
}

function emitEvent(payload: IPCEvent) {
  if (ipcCallback) {
    act(() => {
      ipcCallback!({ payload });
    });
  }
}

describe("App", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    document.documentElement.removeAttribute("data-theme");
    setupMock();
  });

  afterEach(() => {
    document.documentElement.removeAttribute("data-theme");
  });

  it("renders Vibe Island brand", () => {
    render(<App />);
    expect(screen.getByText("Vibe Island")).toBeInTheDocument();
  });

  it("shows 'No active sessions' when empty", () => {
    render(<App />);
    expect(screen.getByText("No active sessions")).toBeInTheDocument();
  });

  it("shows session after session_started event", () => {
    render(<App />);
    emitEvent({ type: "session_started", session: baseSession });
    expect(screen.getByText("CC")).toBeInTheDocument();
    expect(screen.getByText("my-project", { selector: ".session-cwd-tag" })).toBeInTheDocument();
  });

  it("shows agent abbreviation for each agent type", () => {
    render(<App />);

    emitEvent({ type: "session_started", session: { ...baseSession, agent: "claude-code" } });
    emitEvent({ type: "session_started", session: { ...baseSession, id: "s2", agent: "opencode" } });
    emitEvent({ type: "session_started", session: { ...baseSession, id: "s3", agent: "gemini" } });
    emitEvent({ type: "session_started", session: { ...baseSession, id: "s4", agent: "codex" } });

    expect(screen.getByText("CC")).toBeInTheDocument();
    expect(screen.getByText("OC")).toBeInTheDocument();
    expect(screen.getByText("GE")).toBeInTheDocument();
    expect(screen.getByText("CX")).toBeInTheDocument();
  });

  it("shows unknown agent abbreviation for unrecognized agents", () => {
    render(<App />);
    emitEvent({ type: "session_started", session: { ...baseSession, agent: "custom-agent" } });
    expect(screen.getByText("CU")).toBeInTheDocument();
  });

  it("removes session after session_ended event", () => {
    render(<App />);
    emitEvent({ type: "session_started", session: baseSession });
    expect(screen.getByText("CC")).toBeInTheDocument();

    emitEvent({ type: "session_ended", session_id: "cli-12345" });
    expect(screen.queryByText("CC")).not.toBeInTheDocument();
    expect(screen.getByText("No active sessions")).toBeInTheDocument();
  });

  it("updates session title on session_updated", () => {
    render(<App />);
    emitEvent({ type: "session_started", session: baseSession });
    expect(screen.getByText("my-project", { selector: ".session-title" })).toBeInTheDocument();

    emitEvent({
      type: "session_updated",
      session: { ...baseSession, title: "Write src/main.ts" },
    });
    expect(screen.getByText("Write src/main.ts", { selector: ".session-title" })).toBeInTheDocument();
  });

  it("calls jumpToTerminal when session is clicked", async () => {
    mockInvoke.mockResolvedValue(undefined);
    render(<App />);
    emitEvent({ type: "session_started", session: baseSession });

    const sessionItem = screen.getByText("CC").closest(".session-item")!;
    await userEvent.click(sessionItem);

    expect(mockInvoke).toHaveBeenCalledWith("jump_to_terminal", {
      session_id: "cli-12345",
    });
  });

  it("calls minimize when minimize button clicked", async () => {
    render(<App />);
    const minimizeBtn = screen.getByTitle("Minimize");
    await userEvent.click(minimizeBtn);
    expect(mockMinimize).toHaveBeenCalled();
  });

  it("calls close when close button clicked", async () => {
    render(<App />);
    const closeBtn = screen.getByTitle("Close");
    await userEvent.click(closeBtn);
    expect(mockClose).toHaveBeenCalled();
  });

  it("toggles theme on theme button click", async () => {
    render(<App />);
    const themeBtn = screen.getByTitle("Switch theme");

    expect(document.documentElement.getAttribute("data-theme")).toBe("default");

    await userEvent.click(themeBtn);
    expect(document.documentElement.getAttribute("data-theme")).toBe("forest");

    await userEvent.click(themeBtn);
    expect(document.documentElement.getAttribute("data-theme")).toBe("default");
  });

  it("persists theme in localStorage", async () => {
    render(<App />);
    const themeBtn = screen.getByTitle("Switch theme");

    await userEvent.click(themeBtn);
    expect(localStorage.getItem("vibe-theme")).toBe("forest");
  });

  it("restores theme from localStorage", () => {
    localStorage.setItem("vibe-theme", "forest");
    render(<App />);
    expect(document.documentElement.getAttribute("data-theme")).toBe("forest");
  });

  it("extracts directory name from cwd path", () => {
    render(<App />);
    emitEvent({
      type: "session_started",
      session: { ...baseSession, cwd: "/Users/test/deep/nested/project" },
    });
    expect(screen.getByText("project")).toBeInTheDocument();
  });

  it("renders pixel knight with correct agent class", () => {
    render(<App />);
    emitEvent({ type: "session_started", session: { ...baseSession, agent: "gemini" } });
    const knight = document.querySelector(".pixel-knight.agent-gemini");
    expect(knight).toBeInTheDocument();
  });

  it("renders pixel knight with correct status class", () => {
    render(<App />);
    emitEvent({ type: "session_started", session: { ...baseSession, status: "waiting" } });
    const knight = document.querySelector(".pixel-knight.waiting");
    expect(knight).toBeInTheDocument();
  });
});
