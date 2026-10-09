# frontend-admin/AGENTS.md

Apply the [root instructions](../AGENTS.md) with these admin frontend rules.
This separate app uses React 19, Vite, Tailwind CSS v4, TanStack Router and Query, and bun.
Run recipes from `frontend-admin/`, or use `just frontend-admin <args>` from the root.
The backend admin listener serves the built `dist/` assets.

## Commands and checks

| Command           | Purpose                                                        |
| ----------------- | -------------------------------------------------------------- |
| `just install`    | Install dependencies with bun                                  |
| `just dev`        | Start Vite on port 5174 and proxy `/api` to the admin listener |
| `just build`      | Build production assets into `dist/`                           |
| `just check`      | Run type, lint, and formatting checks in parallel              |
| `just fix`        | Apply Prettier and ESLint fixes                                |
| `just type-check` | Run TypeScript without emitting files                          |
| `just lint-check` | Check ESLint rules in `src/`                                   |
| `just fmt-check`  | Check Prettier formatting in `src/`                            |

Use `just check` after source edits. `just fix` and `just lint` edit files; rerun `just check` after fixes.
Lint checks fail on warnings. Review the output and resolve all findings.

## Implementation rules

- Use the automatic JSX transform; do not add `import React` for JSX.
- Use `@/*` for imports from `src/*`. Keep domain functions in their domain modules; do not add classes solely to group functions.
- Define routes in `src/router.tsx` with TanStack Router.
- Keep server queries, mutations, and invalidation in `src/lib/queries.ts` with TanStack Query.
- Keep credentials in `src/lib/auth.ts`. The route guard and API handler redirect users to login when credentials are absent or rejected.
- Validate API responses with zod schemas in `src/entity/schemas.ts`. Keep their camelCase shapes consistent with the backend admin and auth types.
- Use `sonner` for toasts. Use Query for server state and Router for route state; do not add Zustand without a concrete client-state need.
- Preserve strict TypeScript and typed ESLint checks. Configuration lives in `tsconfig.json` and `eslint.config.mjs`.
- Follow `.prettierrc`: double quotes, semicolons, trailing commas, and 100-character width. The Tailwind plugin sorts classes.

## Environment and authentication

- `VITE_ADMIN_API_URL` defaults to `/api` for the same-origin admin API.
- `VITE_ADMIN_PROXY_TARGET` selects the dev proxy target; its default is `http://localhost:9013`.
- The backend needs both `ADMIN_KEY` and `ADMIN_BIND_TO` to start the admin listener.
- Users enter the admin key in the login form. Do not put it in `VITE_` variables.
- Credentials use `localStorage` keys `admin_api_url` and `admin_key`. A 401 clears credentials and redirects to login through `src/lib/api.ts`.
