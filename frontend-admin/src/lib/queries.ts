import { type QueryClient, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api } from "@/lib/api";
import {
  type AdminSettings,
  type AuthProvider,
  type AuthPreset,
  type Connections,
  type FeedbackFilter,
  type FeatureFlagRow,
  type FeatureFlagUpdate,
  type NoticeSeverity,
  type SessionInfo,
  type ToastPayload,
  type UserDetail,
  type UserEdit,
  type UserSummary,
  adminSettingsSchema,
  adminSessionInfoSchema,
  pageSchema,
  authProvidersResponseSchema,
  connectionsSchema,
  feedbackRowSchema,
  featureFlagRowSchema,
  metadataMapSchema,
  toastPayloadSchema,
  userDetailSchema,
  userNoticeRowSchema,
  userSummarySchema,
} from "@/entity/schemas";

export const qk = {
  settings: ["settings"] as const,
  connections: ["connections"] as const,
  metadata: ["metadata"] as const,
  authProviders: ["auth-providers"] as const,
  users: ["users"] as const,
  userDetail: (id: string) => ["users", id] as const,
  sessions: ["sessions"] as const,
  userNotices: ["user-notices"] as const,
  feedback: (filter: FeedbackFilter) => ["feedback", filter] as const,
  featureFlags: ["feature-flags"] as const,
};

function parse<T>(schema: { parse: (v: unknown) => T }, value: unknown): T {
  return schema.parse(value);
}

export function useSettings() {
  return useQuery({
    queryKey: qk.settings,
    queryFn: async ({ signal }) => parse(adminSettingsSchema, await api.get("/settings", signal)),
  });
}

export function useConnections() {
  return useQuery({
    queryKey: qk.connections,
    queryFn: async ({ signal }) => parse(connectionsSchema, await api.get("/connections", signal)),
    refetchInterval: 5000,
  });
}

export function useMetadata() {
  return useQuery({
    queryKey: qk.metadata,
    queryFn: async ({ signal }) => parse(metadataMapSchema, await api.get("/metadata", signal)),
    refetchInterval: 5000,
  });
}

export function useAuthProviders() {
  return useQuery({
    queryKey: qk.authProviders,
    queryFn: async ({ signal }) =>
      parse(authProvidersResponseSchema, await api.get("/auth-providers", signal)),
  });
}

export interface PageOptions {
  cursor?: string;
  search: string;
  sort: string;
  descending: boolean;
}

function pagePath(path: string, page: PageOptions) {
  const params = new URLSearchParams({
    search: page.search,
    sort: page.sort,
    descending: String(page.descending),
  });
  if (page.cursor) params.set("cursor", page.cursor);
  return `${path}${path.includes("?") ? "&" : "?"}${params.toString()}`;
}

export function useUsers(page: PageOptions) {
  return useQuery({
    queryKey: [...qk.users, "list", page],
    queryFn: async ({ signal }) =>
      parse(pageSchema(userSummarySchema), await api.get(pagePath("/users", page), signal)),
  });
}

export function useUserDetail(id: string) {
  return useQuery({
    queryKey: qk.userDetail(id),
    queryFn: async ({ signal }) =>
      parse(userDetailSchema, await api.get(`/users/${encodeURIComponent(id)}`, signal)),
  });
}

export function useUpdateUser(id: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (body: UserEdit) =>
      parse(
        userDetailSchema,
        await api.patch<UserDetail>(`/users/${encodeURIComponent(id)}`, body),
      ),
    onSuccess: (detail) => {
      qc.setQueryData(qk.userDetail(id), detail);
      void qc.invalidateQueries({ queryKey: qk.users });
    },
  });
}

export function useSessions(page: PageOptions) {
  return useQuery({
    queryKey: [...qk.sessions, page],
    queryFn: async ({ signal }) =>
      parse(pageSchema(adminSessionInfoSchema), await api.get(pagePath("/sessions", page), signal)),
  });
}

export function useUserNotices(page: PageOptions) {
  return useQuery({
    queryKey: [...qk.userNotices, page],
    queryFn: async ({ signal }) =>
      parse(
        pageSchema(userNoticeRowSchema),
        await api.get(pagePath("/user-notices", page), signal),
      ),
  });
}

export function useFeedback(filter: FeedbackFilter, page: PageOptions) {
  return useQuery({
    queryKey: [...qk.feedback(filter), page],
    queryFn: async ({ signal }) => {
      const data = await api.get(
        pagePath(`/feedback?handled=${encodeURIComponent(filter)}`, page),
        signal,
      );
      return parse(pageSchema(feedbackRowSchema), data);
    },
  });
}

// --- Mutations ---

export function useUpdateSetting() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ name, value }: { name: string; value: unknown }) =>
      parse(adminSettingsSchema, await api.put<AdminSettings>(`/settings/${name}`, { value })),
    onSuccess: (settings) => {
      qc.setQueryData(qk.settings, settings);
    },
  });
}

export function useForceSync() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (type: "realtime" | "static" | "gbfs") => api.post(`/sync/${type}`),
    onSuccess: () => {
      setTimeout(() => {
        void qc.invalidateQueries({ queryKey: qk.metadata });
      }, 1000);
    },
  });
}

export function useSendNotify() {
  return useMutation({
    mutationFn: async (payload: ToastPayload) =>
      api.post("/notify", toastPayloadSchema.parse(payload)),
  });
}

function invalidateFeedback(qc: QueryClient) {
  void qc.invalidateQueries({ queryKey: ["feedback"] });
}

export function useArchiveFeedback() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, body }: { id: number; body: unknown }) =>
      parse(feedbackRowSchema, await api.put(`/feedback/${id}/handled`, body)),
    onSuccess: () => {
      invalidateFeedback(qc);
    },
  });
}

export function useReplyFeedback() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, body }: { id: number; body: { reply: string } }) =>
      parse(feedbackRowSchema, await api.post(`/feedback/${id}/reply`, body)),
    onSuccess: () => {
      invalidateFeedback(qc);
    },
  });
}

export function useDismissFeedback() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ id }: { id: number }) =>
      parse(feedbackRowSchema, await api.post(`/feedback/${id}/dismiss`)),
    onSuccess: () => {
      invalidateFeedback(qc);
    },
  });
}

export function useDeleteFeedback() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ id }: { id: number }) => {
      await api.del(`/feedback/${id}`);
    },
    onSuccess: () => {
      invalidateFeedback(qc);
    },
  });
}

export function useClearAllFeedback() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async () => api.del("/feedback"),
    onSuccess: () => {
      invalidateFeedback(qc);
    },
  });
}

export function useCreateAuthProvider() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (body: { id: string; clientId: string; clientSecret: string }) =>
      api.post("/auth-providers", body),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: qk.authProviders });
    },
  });
}

export function useUpdateAuthProvider() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({
      id,
      body,
    }: {
      id: string;
      body: { clientId?: string; clientSecret?: string; enabled?: boolean };
    }) => api.put(`/auth-providers/${id}`, body),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: qk.authProviders });
    },
  });
}

export function useDeleteAuthProvider() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) => api.del(`/auth-providers/${id}`),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: qk.authProviders });
    },
  });
}

export function useDeleteUser() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) => api.del(`/users/${id}`),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: qk.users });
      void qc.invalidateQueries({ queryKey: qk.sessions });
    },
  });
}

export function useRevokeUserSessions() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) => api.post(`/users/${id}/revoke-sessions`),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: qk.sessions });
      void qc.invalidateQueries({ queryKey: ["users"] });
    },
  });
}

export function useDeleteSession() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) => api.del(`/sessions/${id}`),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: qk.sessions });
      void qc.invalidateQueries({ queryKey: ["users"] });
    },
  });
}

export function useCreateUserNotice() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (body: { userId: string; text: string; severity: NoticeSeverity }) =>
      api.post("/user-notices", body),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: qk.userNotices });
      void qc.invalidateQueries({ queryKey: ["users"] });
    },
  });
}

export function useDeleteUserNotice() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) => api.del(`/user-notices/${id}`),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: qk.userNotices });
      void qc.invalidateQueries({ queryKey: ["users"] });
    },
  });
}

export function useFeatureFlags() {
  return useQuery({
    queryKey: qk.featureFlags,
    queryFn: async ({ signal }) =>
      parse(featureFlagRowSchema.array(), await api.get("/feature-flags", signal)),
  });
}

export function useUpdateFeatureFlag() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, body }: { id: number; body: FeatureFlagUpdate }) =>
      parse(featureFlagRowSchema, await api.patch<FeatureFlagRow>(`/feature-flags/${id}`, body)),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: qk.featureFlags });
    },
  });
}

export function useDeleteFeatureFlag() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (id: number) => api.del(`/feature-flags/${id}`),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: qk.featureFlags });
    },
  });
}

export type { AuthProvider, AuthPreset, Connections, SessionInfo, UserSummary };
