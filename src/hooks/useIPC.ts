import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState, useCallback } from "react";
import type { Session, PermissionRequest, IPCEvent } from "../types";

interface UseIPCReturn {
  sessions: Session[];
  pendingPermissions: PermissionRequest[];
  sendResponse: (requestId: string, approved: boolean, response?: string) => Promise<void>;
  jumpToTerminal: (sessionId: string) => Promise<void>;
}

export function useIPC(): UseIPCReturn {
  const [sessions, setSessions] = useState<Session[]>([]);
  const [pendingPermissions, setPendingPermissions] = useState<PermissionRequest[]>([]);

  useEffect(() => {
    const unlisten = listen<IPCEvent>("ipc-event", (event) => {
      const payload = event.payload;

      switch (payload.type) {
        case "session_started":
        case "session_updated":
          setSessions((prev) => {
            const idx = prev.findIndex((s) => s.id === payload.session.id);
            if (idx >= 0) {
              const updated = [...prev];
              updated[idx] = payload.session;
              return updated;
            }
            return [...prev, payload.session];
          });
          break;

        case "session_ended":
          setSessions((prev) => prev.filter((s) => s.id !== payload.session_id));
          break;

        case "permission_requested":
          setPendingPermissions((prev) => [...prev, payload.request]);
          break;

        case "permission_approved":
        case "permission_denied":
          setPendingPermissions((prev) =>
            prev.filter((p) => p.id !== payload.request_id)
          );
          break;
      }
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const sendResponse = useCallback(
    async (requestId: string, approved: boolean, response?: string) => {
      await invoke("send_permission_response", {
        requestId,
        approved,
        response,
      });
    },
    []
  );

  const jumpToTerminal = useCallback(async (sessionId: string) => {
    await invoke("jump_to_terminal", { session_id: sessionId });
  }, []);

  return { sessions, pendingPermissions, sendResponse, jumpToTerminal };
}
