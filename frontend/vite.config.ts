import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import tsconfigPaths from "vite-tsconfig-paths";
import { defineConfig, loadEnv, type Plugin } from "vite";

const FAVICON_DIR = fileURLToPath(new URL("./src/assets/img/favicon/", import.meta.url));

const MANIFEST_ICONS = [
  { file: "android-chrome-192x192.png", sizes: "192x192", type: "image/png", purpose: "any" },
  { file: "android-chrome-512x512.png", sizes: "512x512", type: "image/png", purpose: "any" },
  { file: "favicon.svg", sizes: "any", type: "image/svg+xml", purpose: "any" },
  { file: "favicon-maskable.svg", sizes: "any", type: "image/svg+xml", purpose: "maskable" },
] as const;

const MANIFEST_BASE = {
  name: "ZET Live",
  short_name: "ZET Live",
  description: "Trenutni položaj i red vožnje svih javno dostupnih ZET vozila prikazan uživo",
  id: "/",
  start_url: "/",
  scope: "/",
  lang: "hr",
  theme_color: "#ffffff",
  background_color: "#ffffff",
  display: "standalone",
  orientation: "any",
  categories: ["navigation", "travel", "utilities"],
} as const;

function webmanifest(): Plugin {
  return {
    name: "zet-live:webmanifest",
    generateBundle() {
      const icons = MANIFEST_ICONS.map((icon) => {
        const source = fs.readFileSync(path.join(FAVICON_DIR, icon.file));
        const hash = createHash("sha256").update(source).digest("hex").slice(0, 8);
        const dot = icon.file.lastIndexOf(".");
        const base = dot >= 0 ? icon.file.slice(0, dot) : icon.file;
        const ext = dot >= 0 ? icon.file.slice(dot) : "";
        const fileName = `_static/file.${base}.${hash}${ext}`;

        this.emitFile({ type: "asset", fileName, source: new Uint8Array(source) });

        return iconEntry(icon, `/${fileName}`);
      });

      this.emitFile({
        type: "asset",
        fileName: "site.webmanifest",
        source: JSON.stringify({ ...MANIFEST_BASE, icons }, null, 2),
      });
    },
    configureServer(server) {
      server.middlewares.use("/site.webmanifest", (_req, res) => {
        const icons = MANIFEST_ICONS.map((icon) =>
          iconEntry(icon, `/src/assets/img/favicon/${icon.file}`),
        );

        res.setHeader("content-type", "application/manifest+json");
        res.end(JSON.stringify({ ...MANIFEST_BASE, icons }, null, 2));
      });
    },
  };
}

function iconEntry(
  icon: (typeof MANIFEST_ICONS)[number],
  src: string,
): { src: string; sizes: string; type: string; purpose?: string } {
  const entry = { src, sizes: icon.sizes, type: icon.type };

  return icon.purpose === "any" ? entry : { ...entry, purpose: icon.purpose };
}

const SEO_PATHS = ["/", "/privacy-policy.html", "/tos.html"] as const;

function sitemapXml(siteUrl: string): string {
  const entries = SEO_PATHS.map(
    (p) =>
      `  <url>\n` +
      `    <loc>${siteUrl}${p}</loc>\n` +
      `    <xhtml:link rel="alternate" hreflang="hr" href="${siteUrl}${p}"/>\n` +
      `  </url>`,
  ).join("\n");

  return (
    `<?xml version="1.0" encoding="UTF-8"?>\n` +
    `<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"\n` +
    `        xmlns:xhtml="http://www.w3.org/1999/xhtml">\n` +
    `${entries}\n` +
    `</urlset>\n`
  );
}

function robotsTxt(siteUrl: string): string {
  const lines = ["User-agent: *", "Allow: /"];
  if (siteUrl) lines.push(`Sitemap: ${siteUrl}/sitemap.xml`);
  return `${lines.join("\n")}\n`;
}

function seo(siteUrl: string): Plugin {
  return {
    name: "zet-live:seo",
    generateBundle() {
      this.emitFile({ type: "asset", fileName: "sitemap.xml", source: sitemapXml(siteUrl) });
      this.emitFile({ type: "asset", fileName: "robots.txt", source: robotsTxt(siteUrl) });
    },
    configureServer(server) {
      server.middlewares.use("/sitemap.xml", (_req, res) => {
        res.setHeader("content-type", "application/xml; charset=utf-8");
        res.end(sitemapXml(siteUrl));
      });
      server.middlewares.use("/robots.txt", (_req, res) => {
        res.setHeader("content-type", "text/plain; charset=utf-8");
        res.end(robotsTxt(siteUrl));
      });
    },
  };
}

const MODULE_PRELOAD_CHUNKS = ["maplibre-gl", "map-container"];

function modulePreloadMap(): Plugin {
  return {
    name: "zet-live:module-preload-map",
    apply: "build",
    transformIndexHtml: {
      enforce: "post",
      handler(_html, ctx) {
        if (!ctx.bundle) return;
        const tags = [];
        for (const chunk of Object.values(ctx.bundle)) {
          if (chunk.type !== "chunk") continue;
          if (MODULE_PRELOAD_CHUNKS.some((t) => chunk.fileName.includes(t))) {
            tags.push({
              tag: "link",
              attrs: { rel: "modulepreload", href: `/${chunk.fileName}` },
              injectTo: "head-prepend",
            });
          }
        }
        return tags;
      },
    },
  };
}

function frontendVersion(): Plugin {
  let publicDir = "";

  return {
    name: "zet-live:frontend-version",
    apply: "build",
    enforce: "post",
    configResolved(config) {
      publicDir = config.publicDir;
    },
    generateBundle: {
      order: "post",
      handler(_options, bundle) {
        const files = new Map<string, string | Uint8Array>();
        for (const [fileName, output] of Object.entries(bundle)) {
          files.set(fileName, output.type === "chunk" ? output.code : output.source);
        }
        if (publicDir && fs.existsSync(publicDir)) {
          for (const fileName of fs.readdirSync(publicDir, { recursive: true })) {
            const fullPath = path.join(publicDir, fileName);
            if (fs.statSync(fullPath).isFile()) {
              files.set(fileName.split(path.sep).join("/"), fs.readFileSync(fullPath));
            }
          }
        }

        const hash = createHash("sha256");
        for (const fileName of [...files.keys()].sort()) {
          const digest = createHash("sha256").update(files.get(fileName)!).digest("hex");
          hash.update(JSON.stringify([fileName, digest]));
        }
        const id = hash.digest("hex");
        const index = bundle["index.html"];
        if (index?.type !== "asset" || typeof index.source !== "string") {
          this.error("Frontend version requires index.html");
        }

        // Stamp the HTML after hashing so the version does not depend on itself.
        index.source = index.source.replace(
          "</head>",
          `  <meta name="zet-frontend-version" content="${id}" />\n</head>`,
        );
        this.emitFile({
          type: "asset",
          fileName: "frontend-version.json",
          source: JSON.stringify({ id }),
        });
      },
    },
  };
}

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), "VITE_");
  const siteUrl = (env.VITE_PUBLIC_SITE_URL ?? "").replace(/\/+$/, "");

  return {
    plugins: [
      react(),
      tailwindcss(),
      tsconfigPaths(),
      webmanifest(),
      seo(siteUrl),
      modulePreloadMap(),
      frontendVersion(),
    ],
    build: {
      rollupOptions: {
        output: {
          assetFileNames: "_static/file.[name].[hash].[ext]",
          chunkFileNames: "_static/chunk.[name].[hash].js",
          entryFileNames: "_static/entry.[name].[hash].js",
          manualChunks(id) {
            // Map packages (maplibre-gl + the react-map-gl/@vis.gl/@maplibre
            // wrappers) are reachable only via the lazy MapContainer, so leave
            // them to Rollup's default placement. Forcing them into a named
            // chunk creates a static cross-chunk edge that makes the ~900 KB
            // map bundle execute eagerly; returning null keeps them lazy.
            if (
              id.includes("/node_modules/maplibre-gl/") ||
              id.includes("/node_modules/react-map-gl/") ||
              id.includes("/node_modules/@vis.gl/react-map") ||
              id.includes("/node_modules/@maplibre/")
            ) {
              return null;
            }

            if (id.includes("/style/") && id.endsWith(".json")) {
              const name = id.split("/").pop()!.split(".").slice(0, -1).join(".");

              return `map-style-${name}`;
            }

            if (
              id.includes("/node_modules/framer-motion/") ||
              id.includes("/node_modules/motion")
            ) {
              return "motion";
            }

            if (id.includes("/node_modules/")) {
              return "vendor";
            }

            return null;
          },
        },
      },
    },
    server: {
      host: true,
      allowedHosts: true,
    },
  };
});
