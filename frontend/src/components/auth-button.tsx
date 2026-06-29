import { useAuthModalOpen, useCurrentUser, openAuthModal } from "@/auth-store";
import { useAuthProviders } from "@/capabilities-store";
import { Avatar, UserGlyph } from "@/components/avatar";

export function AuthButton() {
  const providers = useAuthProviders();
  const user = useCurrentUser();
  const open = useAuthModalOpen();

  // No providers configured on the backend -> hide the account button entirely.
  if (providers.length === 0) return null;

  return (
    <button
      type="button"
      aria-label="Account"
      onClick={openAuthModal}
      aria-expanded={open}
      aria-controls="account-panel"
      title="Account"
      className="bg-surface-overlay text-on-surface-variant hover:bg-surface flex h-9 w-9 cursor-pointer items-center justify-center overflow-hidden rounded-lg shadow-md backdrop-blur-sm transition-colors"
    >
      <Avatar
        src={user?.avatarUrl ?? null}
        className="h-9 w-9 rounded-lg object-cover"
        fallback={<UserGlyph size={18} />}
        fallbackClassName="text-on-surface-variant h-9 w-9"
      />
    </button>
  );
}
