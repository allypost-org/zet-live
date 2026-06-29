import { useCallback, useEffect, useState } from "react";
import { API_URL } from "@/app/consts";
import { apiFetch } from "@/app/entity/v1/api";
import {
  okResponseSchema,
  revokeAllResponseSchema,
  sessionListResponseSchema,
  type SessionInfo,
} from "@/app/entity/v1/auth";
import { authStore } from "@/auth-store";
import { toast } from "sonner";

export function useSessions() {
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const currentSessionId = authStore((s) => s.sessionId);

  const refresh = useCallback(async () => {
    const res = await apiFetch(`${API_URL}/v1/auth/sessions`, sessionListResponseSchema);
    if (res.data) {
      setSessions(res.data.sessions);
    }
    setLoading(false);
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const revoke = useCallback(async (id: string) => {
    const res = await apiFetch(`${API_URL}/v1/auth/sessions/${id}`, okResponseSchema, {
      method: "DELETE",
    });
    if (res.data) {
      setSessions((prev) => prev.filter((s) => s.id !== id));
      toast.success("Session revoked");
    } else {
      toast.error("Failed to revoke session", { description: res.error.error || undefined });
    }
  }, []);

  const revokeAllOthers = useCallback(async () => {
    const res = await apiFetch(`${API_URL}/v1/auth/sessions/revoke-all`, revokeAllResponseSchema, {
      method: "POST",
    });
    if (res.data) {
      setSessions((prev) => prev.filter((s) => s.id === currentSessionId));
      const n = res.data.revoked;
      toast.success(`Revoked ${n} session${n === 1 ? "" : "s"}`);
    } else {
      toast.error("Failed to revoke sessions", { description: res.error.error || undefined });
    }
  }, [currentSessionId]);

  return { sessions, loading, currentSessionId, refresh, revoke, revokeAllOthers };
}
