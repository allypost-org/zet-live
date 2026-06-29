import { useEffect, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import {
  closeAuthModal,
  useAuthModalOpen,
  useAuthStatus,
  useCurrentUser,
  useIdentities,
} from "@/auth-store";
import { useAuthProviders } from "@/capabilities-store";
import {
  confirmTransfer,
  deleteAccount,
  linkProvider,
  loginWith,
  logout,
  unlinkProvider,
} from "@/hooks/use-auth";
import { useSessions } from "@/hooks/use-sessions";
import { ProviderIcon, providerName } from "@/utils/provider-icons";
import { Avatar, UserGlyph } from "@/components/avatar";
import { cn } from "@/utils/style";

export function AuthModal() {
  const open = useAuthModalOpen();
  const status = useAuthStatus();
  const user = useCurrentUser();
  const identities = useIdentities();
  const providers = useAuthProviders();

  const [busy, setBusy] = useState<string | null>(null);
  const [pendingTransfer, setPendingTransfer] = useState<{
    provider: string;
    token: string;
  } | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);

  useEffect(() => {
    if (!open) return;
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") closeAuthModal();
    }
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [open]);

  function handleClose() {
    if (busy !== null) return;
    setConfirmDelete(false);
    closeAuthModal();
  }

  async function withBusy(key: string, fn: () => Promise<unknown>) {
    setBusy(key);
    try {
      await fn();
    } finally {
      setBusy(null);
    }
  }

  const linkedIds = new Set(identities.map((i) => i.provider));
  const canUnlink = identities.length > 1;

  return (
    <AnimatePresence>
      {open && (
        <div
          id="account-panel"
          className="pointer-events-auto fixed inset-0 z-2000 flex items-center justify-center"
        >
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.15 }}
            className="absolute inset-0 bg-black/30"
            onClick={handleClose}
          />

          <motion.div
            initial={{ opacity: 0, scale: 0.95, y: 10 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.95, y: 10 }}
            transition={{ type: "spring", damping: 25, stiffness: 300 }}
            className="bg-surface relative z-10 max-h-[85dvh] w-[90vw] max-w-md overflow-auto rounded-2xl shadow-xl"
            aria-label="Account"
            aria-modal="true"
            aria-expanded="true"
          >
            <div className="bg-surface sticky top-0 flex items-center justify-between rounded-t-2xl px-4 py-2">
              <h2 className="text-on-surface text-base font-bold">
                {status === "authenticated" ? "Account" : "Sign in"}
              </h2>
              <button
                type="button"
                aria-label="Close"
                onClick={handleClose}
                disabled={busy !== null}
                className="text-on-surface-faint hover:bg-surface-hover hover:text-on-surface-muted rounded-full p-2 transition-colors disabled:cursor-not-allowed disabled:opacity-50"
              >
                <svg
                  xmlns="http://www.w3.org/2000/svg"
                  width="18"
                  height="18"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                >
                  <line x1="18" y1="6" x2="6" y2="18" />
                  <line x1="6" y1="6" x2="18" y2="18" />
                </svg>
              </button>
            </div>

            <div className="flex flex-col gap-4 px-5 pb-5">
              {status === "loading" ? (
                <div className="text-on-surface-muted py-8 text-center text-sm">Loading…</div>
              ) : status === "authenticated" && user ? (
                <>
                  <div className="flex items-center gap-3">
                    <Avatar
                      src={user.avatarUrl}
                      className="h-10 w-10 rounded-full object-cover"
                      fallback={<UserGlyph size={20} />}
                      fallbackClassName="bg-surface-hover text-on-surface-variant h-10 w-10 rounded-full"
                    />
                    <div className="min-w-0">
                      <p className="text-on-surface truncate text-sm font-semibold">
                        {user.displayName ?? "Account"}
                      </p>
                      {user.email ? (
                        <p className="text-on-surface-muted truncate text-xs">{user.email}</p>
                      ) : null}
                    </div>
                  </div>

                  <section className="flex flex-col gap-2">
                    <h3 className="text-on-surface-muted text-xs font-semibold tracking-wide uppercase">
                      Linked providers
                    </h3>
                    <ul className="flex flex-col gap-2">
                      {identities.map((id) => (
                        <li
                          key={id.provider}
                          className="border-outline bg-surface-dim flex items-center gap-3 rounded-lg border px-3 py-2"
                        >
                          <Avatar
                            src={id.avatarUrl}
                            className="h-9 w-9 rounded-full object-cover"
                            fallback={<ProviderIcon id={id.provider} size={20} />}
                            fallbackClassName="h-9 w-9 rounded-full"
                            fallbackInCorner
                          />
                          <div className="min-w-0 flex-1">
                            <span className="text-on-surface block truncate text-sm font-semibold">
                              {id.displayName ?? (
                                <em className="text-on-surface-muted font-normal">No name</em>
                              )}
                            </span>
                            {id.email ? (
                              <span className="text-on-surface-muted block truncate text-xs">
                                {id.email}
                              </span>
                            ) : null}
                          </div>
                          <button
                            type="button"
                            disabled={!canUnlink || busy !== null}
                            onClick={() => {
                              void withBusy(`unlink:${id.provider}`, () =>
                                unlinkProvider(id.provider),
                              );
                            }}
                            className="text-on-surface-muted hover:text-danger cursor-pointer rounded px-2 py-1 text-xs font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-30"
                          >
                            {busy === `unlink:${id.provider}` ? (
                              "Removing…"
                            ) : (
                              <span className="inline-block text-right capitalize">
                                Unlink
                                <br />
                                {id.provider}
                              </span>
                            )}
                          </button>
                        </li>
                      ))}
                    </ul>
                    {!canUnlink ? (
                      <p className="text-on-surface-faint text-xs">
                        Add another provider before unlinking this one.
                      </p>
                    ) : null}
                  </section>

                  <SessionsSection />

                  {providers.filter((p) => !linkedIds.has(p.id)).length > 0 ? (
                    <section className="flex flex-col gap-2">
                      <h3 className="text-on-surface-muted text-xs font-semibold tracking-wide uppercase">
                        Link another provider
                      </h3>
                      {providers
                        .filter((p) => !linkedIds.has(p.id))
                        .map((p) => (
                          <ProviderButton
                            key={p.id}
                            providerId={p.id}
                            label={`Link ${p.name}`}
                            busy={busy === `link:${p.id}`}
                            onClick={() => {
                              void withBusy(`link:${p.id}`, async () => {
                                const res = await linkProvider(p.id);
                                if (res.status === "conflict") {
                                  setPendingTransfer({ provider: p.id, token: res.transferToken });
                                }
                              });
                            }}
                          />
                        ))}
                    </section>
                  ) : null}

                  {pendingTransfer ? (
                    <div className="border-primary bg-primary-container text-on-primary-container flex flex-col gap-2 rounded-lg border p-3">
                      <p className="text-on-primary-container text-sm">
                        This <strong>{providerName(pendingTransfer.provider)}</strong> account is
                        already linked to another account. Transfer it to this account?
                      </p>
                      <p className="text-on-surface-muted text-xs">
                        The other account will be removed if it has no other sign-in methods. Its
                        feedback history (submitted messages, name, and contact info) will be merged
                        into your account and become visible in your “My feedback” list.
                      </p>
                      <div className="flex justify-end gap-2">
                        <button
                          type="button"
                          disabled={busy !== null}
                          onClick={() => {
                            setPendingTransfer(null);
                          }}
                          className="text-on-surface-variant cursor-pointer rounded px-3 py-1.5 text-sm font-medium transition-colors disabled:opacity-50"
                        >
                          Cancel
                        </button>
                        <button
                          type="button"
                          disabled={busy !== null}
                          onClick={() => {
                            const token = pendingTransfer.token;
                            void withBusy("transfer", async () => {
                              const ok = await confirmTransfer(token);
                              if (ok) {
                                setPendingTransfer(null);
                              }
                            });
                          }}
                          className="bg-primary text-on-primary cursor-pointer rounded px-3 py-1.5 text-sm font-semibold transition-colors disabled:opacity-50"
                        >
                          {busy === "transfer" ? "Transferring…" : "Transfer"}
                        </button>
                      </div>
                    </div>
                  ) : null}

                  <button
                    type="button"
                    disabled={busy !== null}
                    onClick={() => {
                      void withBusy("logout", async () => {
                        await logout();
                        closeAuthModal();
                      });
                    }}
                    className="border-outline text-on-surface-variant hover:bg-surface-hover mt-2 cursor-pointer rounded-lg border px-3 py-2 text-sm font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50"
                  >
                    {busy === "logout" ? "Signing out…" : "Sign out"}
                  </button>

                  {confirmDelete ? (
                    <div className="border-danger/40 bg-danger-container flex flex-col gap-2 rounded-lg border p-3">
                      <p className="text-on-surface text-sm font-medium">Delete account?</p>
                      <p className="text-on-surface-muted text-xs">
                        This permanently removes your account, linked providers, sessions, and
                        synced settings. Feedback you submitted stays but is anonymized.
                      </p>
                      <div className="flex justify-end gap-2">
                        <button
                          type="button"
                          disabled={busy !== null}
                          onClick={() => {
                            setConfirmDelete(false);
                          }}
                          className="text-on-surface-variant cursor-pointer rounded px-3 py-1.5 text-sm font-medium transition-colors disabled:opacity-50"
                        >
                          Cancel
                        </button>
                        <button
                          type="button"
                          disabled={busy !== null}
                          onClick={() => {
                            void withBusy("delete", async () => {
                              const ok = await deleteAccount();
                              if (ok) {
                                setConfirmDelete(false);
                                closeAuthModal();
                              }
                            });
                          }}
                          className="bg-danger text-on-primary cursor-pointer rounded px-3 py-1.5 text-sm font-semibold transition-colors disabled:opacity-50"
                        >
                          {busy === "delete" ? "Deleting…" : "Delete account"}
                        </button>
                      </div>
                    </div>
                  ) : (
                    <button
                      type="button"
                      disabled={busy !== null}
                      onClick={() => {
                        setConfirmDelete(true);
                      }}
                      className="text-danger hover:bg-surface-hover mt-1 cursor-pointer rounded-lg px-3 py-2 text-sm font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50"
                    >
                      Delete account
                    </button>
                  )}
                </>
              ) : (
                <>
                  <p className="text-on-surface-muted text-sm">
                    Sign in to sync your settings across devices. Accounts use social login only.
                  </p>
                  <div className="flex flex-col gap-2">
                    {providers.map((p) => (
                      <ProviderButton
                        key={p.id}
                        providerId={p.id}
                        label={`Continue with ${p.name}`}
                        busy={busy === p.id}
                        onClick={() => {
                          void withBusy(p.id, () => loginWith(p.id));
                        }}
                      />
                    ))}
                  </div>
                </>
              )}
            </div>
          </motion.div>
        </div>
      )}
    </AnimatePresence>
  );
}

function SessionsSection() {
  const { sessions, loading, currentSessionId, revoke, revokeAllOthers } = useSessions();
  const [confirmRevokeAll, setConfirmRevokeAll] = useState(false);
  const hasOtherSessions = sessions.some((s) => s.id !== currentSessionId);

  return (
    <section className="flex flex-col gap-2">
      <h3 className="text-on-surface-muted text-xs font-semibold tracking-wide uppercase">
        Sessions
      </h3>
      {loading ? (
        <p className="text-on-surface-muted py-2 text-sm">Loading…</p>
      ) : sessions.length === 0 ? (
        <p className="text-on-surface-muted py-2 text-sm">No active sessions.</p>
      ) : (
        <ul className="flex flex-col gap-2">
          {sessions.map((s) => {
            const isCurrent = s.id === currentSessionId;
            return (
              <li
                key={s.id}
                className="border-outline bg-surface-dim flex flex-col gap-1 rounded-lg border px-3 py-2"
              >
                <div className="flex items-center justify-between gap-2">
                  <span className="text-on-surface truncate text-sm font-medium">
                    {s.userAgent || "Unknown device"}
                  </span>
                  {isCurrent ? (
                    <span className="bg-primary-container text-on-primary-container shrink-0 rounded-full px-2 py-0.5 text-xs font-semibold">
                      This device
                    </span>
                  ) : (
                    <button
                      type="button"
                      onClick={() => void revoke(s.id)}
                      className="text-on-surface-muted hover:text-danger shrink-0 cursor-pointer rounded px-2 py-0.5 text-xs font-medium transition-colors"
                    >
                      Revoke
                    </button>
                  )}
                </div>
                <span className="text-on-surface-muted text-xs">
                  {s.ip || "IP unknown"} · {new Date(s.createdAt).toLocaleString()}
                </span>
              </li>
            );
          })}
        </ul>
      )}
      {hasOtherSessions ? (
        confirmRevokeAll ? (
          <div className="border-danger/40 bg-danger-container flex flex-col gap-2 rounded-lg border p-3">
            <p className="text-on-surface text-sm font-medium">Revoke all other sessions?</p>
            <p className="text-on-surface-muted text-xs">
              This signs out every other device/browser signed into your account. This device stays
              signed in.
            </p>
            <div className="flex justify-end gap-2">
              <button
                type="button"
                onClick={() => {
                  setConfirmRevokeAll(false);
                }}
                className="text-on-surface-variant cursor-pointer rounded px-3 py-1.5 text-sm font-medium transition-colors"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={() => {
                  void revokeAllOthers().then(() => {
                    setConfirmRevokeAll(false);
                  });
                }}
                className="bg-danger text-on-primary cursor-pointer rounded px-3 py-1.5 text-sm font-semibold transition-colors"
              >
                Revoke all
              </button>
            </div>
          </div>
        ) : (
          <button
            type="button"
            onClick={() => {
              setConfirmRevokeAll(true);
            }}
            className="text-on-surface-muted hover:text-danger cursor-pointer rounded-lg px-2 py-1.5 text-left text-xs font-medium transition-colors"
          >
            Sign out all other sessions
          </button>
        )
      ) : null}
    </section>
  );
}

function ProviderButton(props: {
  providerId: string;
  label: string;
  busy: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      disabled={props.busy}
      onClick={props.onClick}
      className={cn(
        "border-outline bg-surface hover:bg-surface-hover text-on-surface",
        "flex w-full cursor-pointer items-center justify-center gap-3 rounded-lg border px-3 py-2.5 text-sm font-semibold transition-colors",
        "disabled:cursor-not-allowed disabled:opacity-60",
      )}
    >
      <ProviderIcon id={props.providerId} />
      <span>{props.busy ? "Please wait…" : props.label}</span>
    </button>
  );
}
