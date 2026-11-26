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
import { getT } from "@/i18n";

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
      toast.success(getT()("sessions.revoked", { count: 1 }));
    } else {
      toast.error(getT()("sessions.revokeFailedOne"), {
        description: res.error.error || undefined,
      });
    }
  }, []);

  const revokeAllOthers = useCallback(async () => {
    const res = await apiFetch(`${API_URL}/v1/auth/sessions/revoke-all`, revokeAllResponseSchema, {
      method: "POST",
    });
    if (res.data) {
      setSessions((prev) => prev.filter((s) => s.id === currentSessionId));
      toast.success(getT()("sessions.revoked", { count: res.data.revoked }));
    } else {
      toast.error(getT()("sessions.revokeFailedAll"), {
        description: res.error.error || undefined,
      });
    }
  }, [currentSessionId]);

  return { sessions, loading, currentSessionId, refresh, revoke, revokeAllOthers };
}
