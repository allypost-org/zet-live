import { frontendVersionIdSchema } from "@/app/entity/v1/version";

const frontendVersion = frontendVersionIdSchema.safeParse(
  document.querySelector<HTMLMetaElement>('meta[name="zet-frontend-version"]')?.content,
);
export const FRONTEND_VERSION_ID = frontendVersion.success ? frontendVersion.data : null;

export const SITE_TITLE = "ZET Live";

export const SITE_DESCRIPTION =
  "Trenutni položaj i red vožnje svih javno dostupnih ZET vozila prikazan uživo";

export const API_URL = (import.meta.env.VITE_API_URL as string | undefined) ?? "/api";

export const PLAUSIBLE_SITE_URL =
  (import.meta.env.VITE_PLAUSIBLE_SITE_URL as string | undefined) ?? "";
export const PLAUSIBLE_SCRIPT_URL =
  (import.meta.env.VITE_PLAUSIBLE_SCRIPT_URL as string | undefined) ?? "";
export const PLAUSIBLE_API_URL =
  (import.meta.env.VITE_PLAUSIBLE_API_URL as string | undefined) ?? "";

export const links = {
  github: {
    href: "https://github.com/Allypost",
    text: "GitHub",
  },
  signal: {
    href: "https://signal.me/#eu/ufFtvrezLyxsTqGR8_1wUZk_TZKqdBzz3HdNpOimROglOzpWSt1DCoRZke_MdT4M",
    text: "Signal",
  },
  mastodon: {
    href: "https://mastodon.social/@allypost",
    text: "Mastodon",
    handle: "@Allypost@mastodon.social",
  },
  bluesky: {
    href: "https://bsky.app/profile/allypost.net",
    text: "Bluesky",
  },
  linkedin: {
    href: "https://www.linkedin.com/in/josip-igrec/",
    text: "LinkedIn",
  },
} as const;
