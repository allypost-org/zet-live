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
import { useNodeT, useT } from "@/i18n";
import { useDateTimeFormatter } from "@/utils/time";
import { ProviderIcon, providerName } from "@/utils/provider-icons";
import { Avatar, UserGlyph } from "@/components/avatar";
import { cn } from "@/utils/style";

export function AuthModal() {
  const open = useAuthModalOpen();
  const status = useAuthStatus();
  const user = useCurrentUser();
  const identities = useIdentities();
  const providers = useAuthProviders();
  const t = useT();
  const nt = useNodeT();

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
            aria-label={t("auth.account")}
            aria-modal="true"
            aria-expanded="true"
          >
            <div className="bg-surface sticky top-0 flex items-center justify-between rounded-t-2xl px-4 py-2">
              <h2 className="text-on-surface text-base font-bold">
                {status === "authenticated" ? t("auth.account") : t("auth.modal.title")}
              </h2>
              <button
                type="button"
                aria-label={t("auth.modal.aria.close")}
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
                <div className="text-on-surface-muted py-8 text-center text-sm">
                  {t("auth.modal.loading")}
                </div>
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
                        {user.displayName ?? t("auth.account")}
                      </p>
                      {user.email ? (
                        <p className="text-on-surface-muted truncate text-xs">{user.email}</p>
                      ) : null}
                    </div>
                  </div>

                  <section className="flex flex-col gap-2">
                    <h3 className="text-on-surface-muted text-xs font-semibold tracking-wide uppercase">
                      {t("auth.modal.linkProvidersTitle")}
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
                                <em className="text-on-surface-muted font-normal">
                                  {t("auth.modal.noName")}
                                </em>
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
                              t("auth.modal.removing")
                            ) : (
                              <span className="inline-block text-right capitalize">
                                {t("auth.modal.removeProvider")}
                                <br />
                                {id.provider}
                              </span>
                            )}
                          </button>
                        </li>
                      ))}
                    </ul>
                    {!canUnlink ? (
                      <p className="text-on-surface-faint text-xs">{t("auth.modal.removeHint")}</p>
                    ) : null}
                  </section>

                  <SessionsSection />

                  {providers.filter((p) => !linkedIds.has(p.id)).length > 0 ? (
                    <section className="flex flex-col gap-2">
                      <h3 className="text-on-surface-muted text-xs font-semibold tracking-wide uppercase">
                        {t("auth.modal.linkAnother")}
                      </h3>
                      {providers
                        .filter((p) => !linkedIds.has(p.id))
                        .map((p) => (
                          <ProviderButton
                            key={p.id}
                            providerId={p.id}
                            label={t("auth.modal.linkProvider", { provider: p.name })}
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
                        {nt("auth.modal.transferPrompt", {
                          name: <strong>{providerName(pendingTransfer.provider)}</strong>,
                        })}
                      </p>
                      <p className="text-on-surface-muted text-xs">
                        {t("auth.modal.transferDetail")}
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
                          {t("auth.modal.cancel")}
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
                          {busy === "transfer"
                            ? t("auth.modal.transferBusy")
                            : t("auth.modal.transfer")}
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
                    {busy === "logout" ? t("auth.modal.signingOut") : t("auth.modal.signOut")}
                  </button>

                  {confirmDelete ? (
                    <div className="border-danger/40 bg-danger-container flex flex-col gap-2 rounded-lg border p-3">
                      <p className="text-on-surface text-sm font-medium">
                        {t("auth.modal.deleteConfirmTitle")}
                      </p>
                      <p className="text-on-surface-muted text-xs">
                        {t("auth.modal.deleteDetail")}
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
                          {t("auth.modal.cancel")}
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
                          {busy === "delete" ? t("auth.modal.deleteBusy") : t("auth.modal.delete")}
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
                      {t("auth.modal.delete")}
                    </button>
                  )}
                </>
              ) : (
                <>
                  <p className="text-on-surface-muted text-sm">{t("auth.modal.intro")}</p>
                  <div className="flex flex-col gap-2">
                    {providers.map((p) => (
                      <ProviderButton
                        key={p.id}
                        providerId={p.id}
                        label={t("auth.modal.continueWith", { provider: p.name })}
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
  const t = useT();
  const format = useDateTimeFormatter();
  const hasOtherSessions = sessions.some((s) => s.id !== currentSessionId);

  return (
    <section className="flex flex-col gap-2">
      <h3 className="text-on-surface-muted text-xs font-semibold tracking-wide uppercase">
        {t("auth.modal.sessionsTitle")}
      </h3>
      {loading ? (
        <p className="text-on-surface-muted py-2 text-sm">{t("auth.modal.loading")}</p>
      ) : sessions.length === 0 ? (
        <p className="text-on-surface-muted py-2 text-sm">{t("auth.modal.noActiveSessions")}</p>
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
                    {s.userAgent || t("auth.modal.unknownDevice")}
                  </span>
                  {isCurrent ? (
                    <span className="bg-primary-container text-on-primary-container shrink-0 rounded-full px-2 py-0.5 text-xs font-semibold">
                      {t("auth.modal.thisDevice")}
                    </span>
                  ) : (
                    <button
                      type="button"
                      onClick={() => void revoke(s.id)}
                      className="text-on-surface-muted hover:text-danger shrink-0 cursor-pointer rounded px-2 py-0.5 text-xs font-medium transition-colors"
                    >
                      {t("auth.modal.revokeSession")}
                    </button>
                  )}
                </div>
                <span className="text-on-surface-muted text-xs">
                  {s.ip || t("auth.modal.ipUnknown")} {"\u00B7"}{" "}
                  {format.dateTime(new Date(s.createdAt))}
                </span>
              </li>
            );
          })}
        </ul>
      )}
      {hasOtherSessions ? (
        confirmRevokeAll ? (
          <div className="border-danger/40 bg-danger-container flex flex-col gap-2 rounded-lg border p-3">
            <p className="text-on-surface text-sm font-medium">
              {t("auth.modal.revokeAllConfirm")}
            </p>
            <p className="text-on-surface-muted text-xs">
              {t("auth.modal.revokeAllConfirmDetail")}
            </p>
            <div className="flex justify-end gap-2">
              <button
                type="button"
                onClick={() => {
                  setConfirmRevokeAll(false);
                }}
                className="text-on-surface-variant cursor-pointer rounded px-3 py-1.5 text-sm font-medium transition-colors"
              >
                {t("auth.modal.cancel")}
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
                {t("auth.modal.revokeAll")}
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
            {t("auth.modal.signOutAllOthers")}
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
  const t = useT();
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
      <span>{props.busy ? t("auth.modal.pleaseWait") : props.label}</span>
    </button>
  );
}
