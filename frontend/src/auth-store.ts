import { create } from "zustand";
import type { Identity, User } from "@/app/entity/v1/auth";
import { useStore } from "@/store";

export type AuthStatus = "loading" | "authenticated" | "anonymous";

const TOKEN_STORAGE_KEY = "zet_auth_token";

type AuthState = {
  user: User | null;
  identities: Identity[];
  status: AuthStatus;
  token: string | null;
  sessionId: string | null;
  modalOpen: boolean;
};

function loadToken(): string | null {
  try {
    return localStorage.getItem(TOKEN_STORAGE_KEY);
  } catch {
    return null;
  }
}

function saveToken(token: string) {
  try {
    localStorage.setItem(TOKEN_STORAGE_KEY, token);
  } catch (e) {
    console.error("Failed to persist session token", e);
  }
}

function clearStoredToken() {
  try {
    localStorage.removeItem(TOKEN_STORAGE_KEY);
  } catch {
    // ignore
  }
}

export const authStore = create<AuthState>()(() => ({
  user: null,
  identities: [],
  status: "loading",
  token: loadToken(),
  sessionId: null,
  modalOpen: false,
}));

export function useAuthStatus(): AuthStatus {
  return authStore((s) => s.status);
}

export function useCurrentUser(): User | null {
  return authStore((s) => s.user);
}

export function useIdentities(): Identity[] {
  return authStore((s) => s.identities);
}

export function useAuthModalOpen(): boolean {
  return authStore((s) => s.modalOpen);
}

export function openAuthModal(): void {
  authStore.setState({ modalOpen: true });
}

export function closeAuthModal(): void {
  authStore.setState({ modalOpen: false });
}

export function sessionToken(): string | null {
  return authStore.getState().token;
}

export function setSessionToken(token: string): void {
  saveToken(token);
  authStore.setState({ token });
}

export function setAuth(user: User | null, identities: Identity[], sessionId: string | null): void {
  authStore.setState({
    user,
    identities,
    sessionId,
    status: user ? "authenticated" : "anonymous",
  });
}

export function clearAuth(): void {
  clearStoredToken();
  authStore.setState({
    user: null,
    identities: [],
    status: "anonymous",
    token: null,
    sessionId: null,
  });
  useStore.setState({ userNotices: null });
}
