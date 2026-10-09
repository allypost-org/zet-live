# frontend/AGENTS.md

Apply the [root instructions](../AGENTS.md) with these public frontend rules.
This app uses React 19, Vite, Tailwind CSS v4, and bun.
Run recipes from `frontend/`, or use `just frontend <args>` from the root.

## Commands and checks

| Command           | Purpose                                           |
| ----------------- | ------------------------------------------------- |
| `just install`    | Install dependencies with bun                     |
| `just dev`        | Start Vite on port 5173                           |
| `just build`      | Build production assets into `dist/`              |
| `just check`      | Run type, lint, and formatting checks in parallel |
| `just fix`        | Apply Prettier and ESLint fixes                   |
| `just type-check` | Run TypeScript without emitting files             |
| `just lint-check` | Check ESLint rules in `src/`                      |
| `just fmt-check`  | Check Prettier formatting in `src/`               |

Use `just check` after source edits. `just fix` and `just lint` edit files; rerun `just check` after fixes.
Lint checks fail on warnings. Review the output and resolve all findings.

## Implementation rules

- Use the automatic JSX transform; do not add `import React` for JSX.
- Use `@/*` for imports from `src/*`. Keep domain functions in their domain modules; do not add classes solely to group functions.
- Use Zustand for client state. Existing stores are in `src/store.ts`, `src/feedback-store.ts`, and `src/settings.ts`.
- The SharedWorker in `src/scripts/worker.ts` handles WebSocket traffic and CBOR decoding. `src/hooks/use-websocket.ts` connects it to the app.
- Validate API entities with zod schemas in `src/app/entity/v1/`. Infer types from schemas.
- Use `react-map-gl` and `maplibre-gl` for maps. Map styles live in `src/data/maps/`.
- Use the existing dependencies for animation, toasts, and search: `motion`, `sonner`, and `fuse.js`. Do not introduce Redux, signals, or lodash.
- Preserve strict TypeScript and typed ESLint checks. Configuration lives in `tsconfig.json` and `eslint.config.mjs`.
- Follow `.prettierrc`: double quotes, semicolons, trailing commas, and 100-character width. The Tailwind plugin sorts classes.

## Environment

Client environment variables require the `VITE_` prefix. Do not put secrets in client variables.
`VITE_API_URL` defaults to `/api`. See [README.md](README.md) for production URL and analytics variables.
