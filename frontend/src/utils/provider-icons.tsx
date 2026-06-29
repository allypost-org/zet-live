import type { ReactElement } from "react";

type IconRenderer = (size: number) => ReactElement;

type ProviderMeta = {
  name: string;
  icon: IconRenderer;
};

const googleIcon: IconRenderer = (size) => (
  <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
    <path
      fill="#4285F4"
      d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z"
    />
    <path
      fill="#34A853"
      d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z"
    />
    <path
      fill="#FBBC05"
      d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z"
    />
    <path
      fill="#EA4335"
      d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z"
    />
  </svg>
);

const microsoftIcon: IconRenderer = (size) => (
  <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
    <rect x="2" y="2" width="9.5" height="9.5" fill="#F25022" />
    <rect x="12.5" y="2" width="9.5" height="9.5" fill="#7FBA00" />
    <rect x="2" y="12.5" width="9.5" height="9.5" fill="#00A4EF" />
    <rect x="12.5" y="12.5" width="9.5" height="9.5" fill="#FFB900" />
  </svg>
);

const facebookIcon: IconRenderer = (size) => (
  <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
    <path
      fill="#1877F2"
      d="M24 12.07C24 5.4 18.63 0 12 0S0 5.4 0 12.07C0 18.1 4.39 23.1 10.13 24v-8.44H7.08v-3.49h3.05V9.43c0-3.02 1.79-4.69 4.53-4.69 1.31 0 2.69.24 2.69.24v2.97h-1.52c-1.49 0-1.96.93-1.96 1.89v2.26h3.33l-.53 3.49h-2.8V24C19.61 23.1 24 18.1 24 12.07z"
    />
    <path
      fill="#FFFFFF"
      d="M16.67 15.56l.53-3.49h-3.33V9.81c0-.96.47-1.89 1.96-1.89h1.52V4.95s-1.38-.24-2.69-.24c-2.74 0-4.53 1.67-4.53 4.69v2.64H7.08v3.49h3.05V24c1.22.19 2.46.19 3.68 0v-8.44h2.86z"
    />
  </svg>
);

const linkedinIcon: IconRenderer = (size) => (
  <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
    <path
      fill="#0A66C2"
      d="M20.447 20.452h-3.554v-5.569c0-1.328-.027-3.037-1.852-3.037-1.853 0-2.136 1.445-2.136 2.939v5.667H9.351V9h3.414v1.561h.046c.477-.9 1.637-1.85 3.37-1.85 3.601 0 4.267 2.37 4.267 5.455v6.286zM5.337 7.433a2.062 2.062 0 01-2.063-2.065 2.064 2.064 0 112.063 2.065zm1.782 13.019H3.555V9h3.564v11.452zM22.225 0H1.771C.792 0 0 .774 0 1.729v20.542C0 23.227.792 24 1.771 24h20.451C23.2 24 24 23.227 24 22.271V1.729C24 .774 23.2 0 22.222 0h.003z"
    />
  </svg>
);

const discordIcon: IconRenderer = (size) => (
  <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
    <path
      fill="#5865F2"
      d="M20.317 4.3698a19.7913 19.7913 0 00-4.8851-1.5152.0741.0741 0 00-.0785.0371c-.211.3753-.4447.8648-.6083 1.2495a18.2766 18.2766 0 00-5.4868 0 12.64 12.64 0 00-.6177-1.2495.077.077 0 00-.0785-.037 19.7363 19.7363 0 00-4.8854 1.515.0699.0699 0 00-.0321.0277C.5334 9.0458-.319 13.5799.0992 18.0578a.0824.0824 0 00.0312.0561c2.0528 1.5076 4.0413 2.4228 5.9929 3.0294a.0777.0777 0 00.0842-.0276 14.2122 14.2122 0 001.226-1.9942.076.076 0 00-.0416-.1057 13.107 13.107 0 01-1.8722-.8923.077.077 0 01-.0076-.1277c.1258-.0943.2517-.1923.3718-.2914a.0743.0743 0 01.0776-.0105c3.9278 1.7933 8.18 1.7933 12.0614 0a.0739.0739 0 01.0785.0095c.1202.099.246.1981.3728.2924a.077.077 0 01-.0066.1276 12.2993 12.2993 0 01-1.873.8914.0766.0766 0 00-.0407.1067c.3604.698.7719 1.3628 1.225 1.9932a.076.076 0 00.0842.0286c1.961-.6067 3.9495-1.5219 6.0023-3.0294a.077.077 0 00.0313-.0552c.5004-5.177-.8382-9.6739-3.5485-13.6604a.061.061 0 00-.0312-.0286zM8.02 15.3312c-1.1825 0-2.1569-1.0857-2.1569-2.419 0-1.3332.9555-2.4189 2.157-2.4189 1.2108 0 2.1757 1.0952 2.1568 2.419 0 1.3332-.9555 2.4189-2.1569 2.4189zm7.9748 0c-1.1825 0-2.1569-1.0857-2.1569-2.419 0-1.3332.9554-2.4189 2.1569-2.4189 1.2108 0 2.1757 1.0952 2.1568 2.419 0 1.3332-.946 2.4189-2.1568 2.4189Z"
    />
  </svg>
);

const fallbackIcon: IconRenderer = (size) => (
  <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true" fill="currentColor">
    <path d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z" />
  </svg>
);

const PROVIDERS: Record<string, ProviderMeta> = {
  google: { name: "Google", icon: googleIcon },
  microsoft: { name: "Microsoft", icon: microsoftIcon },
  facebook: { name: "Facebook", icon: facebookIcon },
  linkedin: { name: "LinkedIn", icon: linkedinIcon },
  discord: { name: "Discord", icon: discordIcon },
};

export function providerName(id: string): string {
  return PROVIDERS[id]?.name ?? id;
}

export function ProviderIcon({ id, size = 20 }: { id: string; size?: number }): ReactElement {
  return (PROVIDERS[id]?.icon ?? fallbackIcon)(size);
}
