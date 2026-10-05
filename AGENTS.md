# AGENTS.md

> This is the **root** overview. The backend and both frontend apps have their own, more detailed `AGENTS.md` — **read the relevant one before editing that component**:
>
> - [`backend/AGENTS.md`](backend/AGENTS.md) — Rust/Axum server, sqlx, migrations, protobuf
> - [`frontend/AGENTS.md`](frontend/AGENTS.md) — React 19 + Vite + Tailwind, Zustand, maplibre
> - [`frontend-admin/AGENTS.md`](frontend-admin/AGENTS.md) — React 19 + Vite + Tailwind, TanStack Router/Query

## Project overview

ZET Live — live tracking of ZET (Zagreb) public transit vehicles, plus GBFS bike-share (nextbike) stations. Monorepo: a Rust/Axum backend serves both a REST/WebSocket API and the baked-in frontend static files.

## Structure

- `backend/` — Rust (edition 2024) Axum server. Single crate `zet-live`.
- `frontend/` — React 19 + Vite + Tailwind CSS v4. Managed with bun.
- `frontend-admin/` — React 19 + Vite + Tailwind + TanStack Router SPA for the admin UI. Separate app from `frontend/`, served same-origin by the admin listener (see `frontend-admin/AGENTS.md`).
- `docs/` — planning docs only.
- `Dockerfile`, `.github/workflows/` — build & deploy (cross-cutting; see below).

## Root commands (delegates to sub-justfiles)

```
just backend <args>          # runs `just <args>` in backend/
just frontend <args>         # runs `just <args>` in frontend/
just frontend-admin <args>   # runs `just <args>` in frontend-admin/
just build                   # frontend build && frontend-admin build && backend build
just run <args>              # build, then backend run
```

The root justfile sets `dotenv-load := false`. The sub-justfiles enable it (backend reads `backend/.env`, frontend reads `frontend/.env`, frontend-admin reads `frontend-admin/.env`). The root `.env` is **not** auto-loaded by the root justfile.

No tests exist in either the frontend or the backend (yet).

## Global conventions

- All files: 2-space indent, LF line endings, trim trailing whitespace (see `.editorconfig`). **Rust files are the exception: 4-space indent.**
- Don't commit secrets.
- Use short sentences, active voice, direct instructions, and common words in documentation and user-facing text. Use one term for each concept. Preserve exact API identifiers, protocol terms, and necessary technical detail.

## Comments

- Add comments only for unexpected or difficult behavior, or when they produce user-facing output. This applies to Rust, TypeScript, shell scripts, inline comments, module comments, and doc comments.
- Useful comments explain non-obvious invariants, lock/transaction order, protocol quirks, necessary workarounds, or a surprising choice.
- Do not narrate the code or add routine function summaries. Prefer clear names and simpler code to comments that explain ordinary operations.
- Keep clap doc comments that produce `--help` text, including comments on `Parser`, `Args`, `Subcommand`, `ValueEnum`, and flattened configuration fields. Keep them accurate and useful. Apply the same rule to other generated user-facing documentation.
- Keep necessary `SAFETY:` explanations and reasons for lint suppressions when they explain a real invariant or exception. This does not authorize new lint suppressions contrary to component instructions. Do not add ceremonial comments.
- Fix or remove stale comments in code you change. Avoid unrelated comment churn.

## Document the current state

- Describe what exists now. Do not reference removed or renamed code, files, flags, handlers, commands, or config keys. Explain the present constraint instead of recounting old behavior, unless that behavior remains a live hazard (for example, an upstream bug that can return or an outstanding migration).
- Do not add a comment, test, doc section, or config key merely because it was mentioned in conversation. Add it when the implementation or requirements need it.
- Verify facts before asserting them in tracked files: search for the identifier, read the file, or run the command. Do not describe directories you have not read, or assume machine-local scratch files exist in a fresh clone.

## Tests and verification

- Do not use TDD by default. Do not write a unit test just because code was written.
- Prefer E2E verification of complete flows through public interfaces, against real dependencies or controlled protocol peers. End each E2E check with a verifiable, repeatable artifact, such as a captured response or browser trace.
- Add unit tests for custom parsers, difficult algorithms, or subtle concurrency and protocol behavior. For those, first write down the ways the code can fail. Put Rust unit tests in an inline `#[cfg(test)] mod tests` beside the code they cover.
- Before adding a test, name the meaningful failure it detects. Do not add tests to meet a count, a coverage target, or a generic workflow requirement.
- Assert outcomes, not implementation steps. A search test must check returned results; a log line or mock call count does not prove the user-visible outcome.
- Test failures at the boundary. Cover material risks once instead of repeating the same check at every layer.
- Do not test ordinary getters, setters, thin wrappers, framework behavior, or standard library behavior. Do not copy implementation logic into expected values. Avoid large snapshots and verbose fixtures with little diagnostic value.
- Use fakes to control external conditions, while still asserting outcomes. Prefer tests that survive an internal refactor with unchanged behavior. Keep setup and assertions short and relevant.
- Run the checks the change needs. Formatting, type checks, builds, and focused manual checks are enough for low-risk changes; risky changes need meaningful behavioral verification.
- Use this repo's commands: `just backend fmt-dev` for backend formatting, linting, and compile checks; `just backend test` for Rust tests; `just frontend check` and `just frontend-admin check` for frontend checks. Follow the relevant component `AGENTS.md` for details. Do not invent frontend test commands before tooling exists.
- Auto-fix commands edit source. Review the diff and command output afterwards, including warnings, rather than relying only on the exit code.
- Report what you verified and its limits. Do not claim tests or commands passed unless they ran successfully.

## Types and function interfaces

- Use types to express domain meaning and valid states. Avoid bare strings for values with structure or constraints; prefer URL types, typed keys, durations, and enums.
- Prefer existing library types. Otherwise use validated Rust newtypes with constructors or schema validation (such as zod in the frontend apps). A type alias for `String` or `string` does not enforce a constraint.
- Parse and validate at external boundaries: HTTP/WebSocket payloads, external feeds, config files, environment variables, and browser storage. Keep typed values inside the application, and convert back to strings only where a wire or storage format requires it. Preserve original bytes separately from parsed views when signatures or encryption depend on them.
- Use enums or discriminated unions for alternatives. Avoid magic strings, boolean combinations, or bags of optional fields that permit invalid states.
- In TypeScript, use runtime validation for untrusted data; a type assertion does not validate a payload. Prefer inference from schemas to duplicating their types.
- Keep trivial operations simple. Do not add builders, wrappers, or generic frameworks unless they prevent real mistakes.

### Free functions vs. methods

- If a Rust function primarily works on a type defined in this codebase, make it a method on that type. Use a free function when the arguments genuinely belong to different types; do not move it onto a type that does not own its subject.
- Group related Rust functions with no useful receiver under a module or a unit struct namespace when appropriate. Do not add a unit struct for one or two functions that already fit elsewhere. Keep it unit unless there is real state.
- Keep TypeScript functions in the relevant domain module; do not introduce classes merely to namespace functions.

### Type annotations at the call site

- In Rust, when a type must be named, prefer naming it on the expression that produces the value rather than on the binding. For example, use `let values = iter.collect::<Vec<_>>();` rather than `let values: Vec<_> = iter.collect();`. Apply the same preference to `parse::<T>()`, `downcast_ref::<T>()`, and serde helper calls.
- Leave annotations off when inference already works. Use binding annotations when the producing expression cannot express the required type.

## Docker / deploy (cross-cutting)

- Multi-stage Dockerfile: cargo-chef → bun frontend build + bun frontend-admin build → Rust build (with UPX compression) → `scratch` runner. **Docker build targets `x86_64-unknown-linux-gnu`**, whereas dev justfiles target `x86_64-unknown-linux-musl` — don't assume one target everywhere.
- CI (`.github/workflows/`): push to `main` triggers build on Blacksmith runners, pushes to Docker Hub (`allypost/zet-live`), notifies Watchtower. Uses S3 for build cache.
- Frontend env vars for production are set via `frontend/.env.docker` in CI.
- **Database dependency:** the runtime image no longer has an in-process DB. The container requires `DATABASE_URL=postgres://...` at runtime (pointing at an external PostgreSQL instance). The `scratch` runner ships no DB. Cutover from the previous SQLite-on-a-volume setup requires a one-shot data migration per `docs/superpowers/specs/2026-08-03-move-to-postgres-design.md` §15.
