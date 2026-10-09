# AGENTS.md

ZET Live tracks Zagreb public transit vehicles and nextbike stations. The Rust backend serves the API and built frontend assets.

## Apply these instructions

- Read this file and the applicable component instructions from the current worktree before editing:
  - [backend/AGENTS.md](backend/AGENTS.md): Rust/Axum, PostgreSQL, sqlx, protobuf.
  - [frontend/AGENTS.md](frontend/AGENTS.md): public React app, Zustand, MapLibre.
  - [frontend-admin/AGENTS.md](frontend-admin/AGENTS.md): admin React app, TanStack Router and Query.
- Follow repository rules over generic skill defaults, including mandatory TDD. Higher-priority harness instructions still apply; report conflicts that prevent compliance.
- When delegating implementation or review, provide the applicable instruction paths and constraints. Require the agent to read those files.
- Before committing or reporting completion, review the final diff against these rules. Run the applicable checks after the last edit.
- Fix check failures before committing. Report failed or blocked checks and their limits; do not claim that unrun checks passed.

## Commands

Run component recipes with `just backend <args>`, `just frontend <args>`, or `just frontend-admin <args>`.

| Change          | Verification                                                                  |
| --------------- | ----------------------------------------------------------------------------- |
| Backend         | `just backend fmt-dev`, then `just backend check`; Rust tests when applicable |
| Public frontend | `just frontend check`                                                         |
| Admin frontend  | `just frontend-admin check`                                                   |

`just build` builds both frontends, then the backend. `just run <args>` builds and runs the backend.
The root justfile does not load `.env`. Component justfiles load their own `.env` files.

Review auto-fix output and the resulting diff. Resolve lint warnings before committing, even when the command succeeds.
Tests do not replace lint, formatting, type, or build checks. Use existing test tooling; do not invent commands.

### Pre-commit hook

`hk.pkl` requires hk 2.5 or later. Install the hook with `hk install` unless your machine already has a global hk hook.
Run `hk run pre-commit` to check the staged commit. Separate steps check diff whitespace, ASCII glyphs, and each affected component.
Component checks use the full index snapshot: all tracked files as they would exist after the commit. Partly staged files use their staged content.
Unstaged and untracked source files do not affect the checks. The hook does not fix, stage, or stash working files.
The ASCII check rejects common typographic glyphs in added lines. It allows language-specific characters and leaves existing lines alone.
Rust checks include formatting, locked Clippy, and all targets. Both frontend checks include types, formatting, and lint; warnings fail.
Install both frontend dependency sets before checking Rust; its snapshot builds both embedded frontend asset directories.
Snapshots are cached under the worktree's Git directory in `precommit-snapshots/`. The hook needs Bash, flock, Git, awk, grep with PCRE support, just, bun, and Rust tooling.

## Writing and comments

- Follow ASD-STE100 Simplified Technical English (STE) as far as practical in agent responses, documentation, and all comments. This includes internal comments, doc comments, and comments that produce user-facing text.
- Use short sentences, active voice, direct instructions, and literal wording. Use one term for each concept. Avoid idioms, ornate language, and filler.
- Preserve exact identifiers, protocol terms, quotations, and necessary technical detail. Prefer STE vocabulary where it expresses the correct meaning.
- Use ASCII in source and documentation, except where a language needs non-ASCII characters, such as Croatian diacritics. Do not use typographic decoration in prose or comments.
- Use `-`, `->`, `...`, and straight quotes. Use HTML entities or `\uXXXX` escapes when a rendered glyph is required; prefer plain ASCII when possible.
- Add comments only to explain difficult behavior, invariants, lock order, protocol quirks, or necessary workarounds. Do not narrate routine code.
- Keep accurate comments that generate user-facing documentation, including clap `--help` text. Keep necessary `SAFETY:` explanations and reasons for permitted lint suppressions.
- Fix stale comments in code you change or remove them outright if they break these rules. Avoid unrelated comment edits.
- Describe the current implementation. Verify facts in source or command output before adding them to tracked files.
- Mention removed behavior only when it remains a live hazard. Add documentation or configuration only when the implementation needs it.

## Code and interfaces

- Follow `.editorconfig`: 2-space indent, LF, no trailing whitespace. Rust uses 4-space indent. Do not commit secrets.
- Prefer source files under 500 lines; avoid files with 1,000 or more lines. Large files make navigation, organization, and context management harder.
- Split growing files into meaningful units using the language's standard modules or submodules. Separate distinct entities and their behavior along clear domain boundaries.
- Keep a larger file only when splitting would make the code harder to understand. Valid reasons include a complex function that needs to stay together or a cohesive route definition.
- Keep trivial operations simple. Avoid forwarding wrappers that add no validation, invariant, or domain behavior. Add builders or generic frameworks only when they prevent real mistakes.
- Avoid long positional argument lists. Reuse an owning type or group related inputs when this expresses domain meaning. Do not add a parameter object solely to bypass a lint.
- Use types to express valid states. Prefer existing library types, validated Rust newtypes, and inferred schema types over unvalidated string aliases.
- Validate external inputs at HTTP, WebSocket, feed, configuration, and browser-storage boundaries. Keep typed values inside the application.
- Use enums or discriminated unions for alternatives. Avoid magic strings and optional-field combinations that permit invalid states.
- In TypeScript, validate untrusted data at runtime; type assertions do not validate payloads.
- Preserve original bytes separately when signatures or encryption depend on them. Convert typed values to strings at wire or storage boundaries.

## Tests

- Do not use TDD by default. Before adding a test, identify the meaningful failure it detects.
- Prefer E2E checks through public interfaces, with real dependencies or controlled protocol peers. Capture a response, trace, or other repeatable artifact.
- Add unit tests for custom parsers, difficult algorithms, and subtle concurrency or protocol behavior. First identify failure modes.
- Assert observable outcomes, such as returned search results. A log line or mock call count alone does not prove correct behavior.
- Cover each material risk at the relevant boundary. Avoid duplicate coverage, implementation-shaped expectations, and large snapshots with little diagnostic value.
- Do not test ordinary getters, setters, thin wrappers, framework behavior, or standard library behavior. Do not add tests to satisfy counts or generic workflows.
- Keep fixtures and assertions small. Use fakes to control external conditions while checking outcomes that remain valid after an internal refactor.
- For low-risk changes, use formatting, type checks, builds, and focused manual checks. Risky changes need behavioral verification.

## Deployment constraints

- Docker builds for `x86_64-unknown-linux-gnu`; backend development builds use `x86_64-unknown-linux-musl`.
- The runtime image is `scratch`. It requires an external PostgreSQL database through `DATABASE_URL`.
- Build both frontends before compiling the backend; their `dist/` directories are embedded in the binary.
- The deployment workflow builds and publishes on pushes to `main`, then notifies Watchtower. Production frontend variables come from `frontend/.env.docker`.
