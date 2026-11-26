# Public Frontend Translations (Croatian / English)

Date: 2026-10-25
Status: approved in chat by the owner; built through the normal workflow next

## Intent and why

ZET Live serves Zagreb public transit and nextbike stations. The main audience reads
Croatian, but the UI is today mostly English with leftover Croatian fragments
("Trenutno Stanje", "(stanica)", "[Bajs]"). This feature adds a full two-language UI:
Croatian on first visit for Croatian browsers, English for everyone else, plus a manual
switcher. Feed data is untouched: ZET stop names, route names, headsigns, nextbike
station names, and admin notices arrive from feeds and display as returned, mostly
Croatian. A Croatian user sees Croatian data; an English user sees a hybrid UI
(English chrome, Croatian data). That is accepted.

## Goals

- All user-visible strings in the public frontend exist in `en` and `hr`.
- First visit follows the browser language: any `navigator.languages` entry that
  starts with `hr` selects Croatian; otherwise English. English is the fallback.
- A manual switcher in the settings modal: Auto (browser) / English / Hrvatski.
- A stored choice wins over auto detection, persists in localStorage, and syncs
  across devices through the existing settings sync.
- Missing or mis-shaped keys are build errors, not runtime surprises.
- `<html lang>` follows the active language. Date and time strings format with the
  active locale.
- Zero new runtime dependencies.

## Non-goals

- The admin frontend (`frontend-admin/`) stays English.
- Backend API error messages (~53 strings) stay English. The UI renders them as is.
- Nothing translates feed data, admin notice texts, or admin-authored toast messages.
- MapLibre built-in control strings (attribution, navigation chrome) stay as the
  library provides them.

## Anti-goals (failure even if it works)

- Croatian UI shown by default to an English browser.
- English UI that still mixes Croatian fragments.
- Untyped dictionaries or runtime-only key lookup that lets missing keys slip to prod.
- A new i18n library when the helper covers the need.

## Constraints from the repository

- Root `AGENTS.md` and `frontend/AGENTS.md` apply. Notably: no new dependencies
  (no Redux, signals, lodash; i18n helper is dependency-free), strict TypeScript,
  typed ESLint (warnings fail), Prettier (double quotes, semicolons, 100 columns,
  2-space indent), files stay under ~500 lines where practical, ASCII in source
  except Croatian diacritics inside translation values (allowed, needed).
- Client state lives in Zustand stores: `src/settings.ts` is the language home.
- Verification: `just frontend check` after every edit batch; final smoke through
  `just run` or `just frontend dev`. Both frontend `dist/` builds must exist for
  backend checks.

## Decided (owner-approved)

### Tooling: typed dictionary, no library

- `frontend/src/i18n/en.ts` is canonical. `frontend/src/i18n/hr.ts` mirrors it.
- Helper lives in `frontend/src/i18n/index.ts`. Total support code stays small
  (order of 30 lines + hook), no runtime dependency.
- Builder call (2026-10-25): a second helper `tNode()` handles the rare sentences
  that embed styled JSX (the sheet line "Next stop **X** arriving in **Y**" in
  `app.tsx`), which plain string parameters cannot carry. `tNode()` replaces
  `{name}` placeholders with ReactNode parameters and returns the sentence parts;
  the string return stays the default path.
- Key shape is flat; keys group visually by component area. Detected at build time:

  ```ts
  // en.ts (shape illustration)
  export const en = define({
    tabBar: {
      share: "Share",
      center: "Center",
    },

    ...,
  });

  // hr.ts
  export const hr = define<typeof en>({
    tabBar: { share: "Dijeli", center: "Centriraj" },

    ...,
  });
  ```

- `define<En>(value: En): En` enforces one-to-one keys and per-key value shape:
  if `en` uses a plural object for a key, `hr` must too.

### Value shape: template or plural forms

- A value is either a plain string or an object of plural forms:

  ```ts
  type Template = string | {
    one?: string;
    few?: string;  // Croatian grammar: 2-4; English never needs it
    other: string;
  };
  ```

- Placeholders use `{name}` tokens: `"{count} departures"`. Word order is owned by
  each language's own sentence, so order differences are free.
- Parameters are `Record<string, string | number>`. A parameter that never appears
  in the template is ignored. A missing parameter leaves the `{name}` marker
  visible in the output so mistakes show up, never silent blanks.
- The plural selector uses `Intl.PluralRules(activeLocale).select(count)` and always
  takes the parameter named `count`. An absent form falls back to `other`, so an
  English `one`/`other` pair stays legal next to Croatian `one`/`few`/`other`.

### Behavior

- New setting key `language: "auto" | "en" | "hr"`, default `"auto"`.
- `"auto"` resolves the DOM language eagerly at first use: first
  `navigator.languages` entry that starts with `hr` (case-insensitive) maps to
  `hr`; otherwise `en`. The resolution is memoized per page load. `navigator.language`
  probe runs only at render time; no listeners needed.
- `"en"` or `"hr"` stored in `settings.ts` (`DEFAULTS`, `isValidLanguage` validator,
  `localStorage` persist) wins whenever set.
- The `language` key joins `SYNC_SETTINGS_KEYS`, `syncSettingsSchema`
  (`z.enum(["auto", "en", "hr"]).default("auto").catch("auto")`, so a peer that
  does not send the key, or sends a bogus value, falls back per-field without
  breaking the rest of the payload; `applySyncSettings` stays as is), and
  `serializeSync`.
- The switcher UI in `settings-modal.tsx` offers Auto / English / Hrvatski. Croatian
  is displayed as "Hrvatski" in both languages (native names, standard practice).

### Hook and access pattern

- `useT()` returns `(key, params?) => string` and subscribes to the language choice;
  components re-render on switch. `en` is the fallback map for any key missing at
  runtime (compile-time completeness makes this a safety net for hot fixes only).
- A plain (non-hook) accessor for hooks/util modules that cannot use React state
  exists only if sweep finds such call sites; decide when noticed.

### Side effects

- An effect sets `document.documentElement.lang` to `hr` or `en` whenever the
  language changes. `index.html` starts at `lang="hr"` (local majority; the effect
  corrects it immediately for other browsers).
- Locale-aware formatting sites take the locale for the active UI language:
  `hr` formats with `hr-HR` (24-hour). `en` formats with the device default
  (undefined locale argument, unchanged from today), an owner choice from the
  builder check (2026-10-25). Sites: `src/utils/time.ts` (`toLocaleTimeString`),
  `app.tsx:221`, `auth-modal.tsx:405`, `feedback-modal.tsx:410`
  (`toLocaleDateString` with options), `vehicle-sheet.tsx:57`.
- Plural selection is always keyed to the active UI language (`"hr"` / `"en"`),
  never the device locale: plural grammar follows the translated words, and
  regional variants do not change `en` or `hr` plural categories.
- The dynamic document subject (`"Trenutno Stanje"`, `"{name} (stanica)"`,
  `"{name} [Bajs]"` in `app.tsx`) translates with the UI language; the browser tab
  title and on-page h1 follow it. Static SEO strings in `index.html`
  (SITE_TITLE/DESCRIPTION and its og/ description meta) stay Croatian; the
  existing dynamic og:title update behavior (JS-updated after load) is cosmetic
  and unchanged, an owner choice from the builder check (2026-10-25).
- The `<noscript>` message in `index.html` becomes one static bilingual line
  (Croatian + English); no inline language machinery. A builder call from the
  builder check (2026-10-25).
- The status bar clock phrases ("now", "1 min", "N min" in `utils/time.ts`; "just
  now", "Ns ago", "Nm ago", "Nh ago" in `status-bar.tsx`) move into the dictionary;
  they own hour/minute plural grammar for hr.

## Sweep discipline (how strings move)

- One component batch at a time; run `just frontend check` after each batch.
- Every user-visible string moves to a key: JSX text nodes, 23 toast call sites,
  19 aria-labels, 4 placeholders, 9 title attributes, sheet and modal labels.
- English fragments already Croatian get canonical English keys; `hr` supplies the
  Croatian wording.
- Entity fallback display names move to templates: `Route {id}`, `Station {id}`
  (`src/app/entity/v1/vehicle.ts:127`, `gbfs-stations.ts:83`).
- SITE_TITLE / SITE_DESCRIPTION (`src/app/consts.ts`) stay as is (SEO strings,
  Croatian) unless the sweep shows they leak into visible chrome; then revisit.

## Out of decided scope, noted for the future

- Backend error strings (server-stated): later, needs Accept-Language plumbing.
- Admin app: stays English; the same helper could move there if asked.
- A third language: dictionary pattern scales; helper needs no change.

## Tests

Per repository rules: only meaningful failures get tests. The template helper is a
custom mini-formatter - it gets a small test file run by bun's own runner:

- `count` plural selection per active language (`hr` few vs other; `en` one vs
  other, zero counts).
- `{name}` substitution; missing parameter leaves the marker.

Builder call (2026-10-25): the file runs through a new `test` recipe in the
frontend justfile (`bun test src`) so it is discoverable and covers the settings
sync-schema checks too; hk does not run it.
No snapshot tests. No tests for ordinary plumbing (settings key, switcher UI).

## Left to the builder

- Exact key names and grouping; whether `i18n/index.ts` splits once files grow.
- English wording cleanup choices and all Croatian phrasing (owner eyeballs final
  wording in the running app).
- Where the switcher sits inside `settings-modal.tsx` and its exact styling, in line
  with existing controls.
- Sweep mechanics order (file layer first or most-visible strings first).
