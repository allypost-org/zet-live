# Public Frontend Translations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Full two-language UI chrome (English canonical, Croatian) in the public frontend, with browser-language detection, a settings switcher, and cross-device sync of the choice.

**Architecture:** A dependency-free typed dictionary module (`src/i18n/`: core helpers + `en.ts`/`hr.ts`) keyed off a new `language` setting (`"auto" | "en" | "hr"`) in `src/settings.ts`. Hooks `useT()`/`useLanguage()` rebind on switch; `getT()` serves non-React call sites (entity fallbacks). Strings move per batch of component files; each batch adds both `en` and `hr` entries and keeps both dictionaries complete.

**Tech Stack:** React 19, TypeScript strict, Zustand, zod, bun (`bun test`), Vite.

**Spec:** `docs/superpowers/specs/2026-10-25-translations-design.md` (read it first; it holds all owner decisions).

## Global Constraints

- Zero new runtime dependencies. The helper is hand-rolled; no i18n library.
- `hr.ts` is typed `define<typeof en>({ ... })`: missing keys, extra keys, or a plural-vs-string mismatch are build errors.
- English fragments currently on screen move verbatim into `en.ts`; the owner's existing Croatian fragments keep their exact wording: `Trenutno Stanje`, `{name} (stanica)`, `{name} [Bajs]`.
- `hr` wording is natural Croatian written by the implementer; run the app and read it before committing a batch (`just frontend dev`, port 5173).
- Data from feeds is never rewritten (stop names, station names, notices, trip headsigns).
- Strict TypeScript, typed ESLint with `--max-warnings 0`; Prettier: double quotes, semicolons, 100 cols, 2-space indent. Editorconfig: LF, no trailing whitespace.
- ASCII in source except Croatian diacritics inside dictionary values.
- After each task's edits: `just frontend check` must pass. Before each commit: `hk run pre-commit` must pass (hook is not installed globally in this repo).
- Commit messages: imperative, capitalized, no prefix (match repo history, e.g. "Fix bottom sheet not respecting content dimensions").
- Scope boundary: `frontend-admin/`, backend, and `index.html` SEO/meta strings are untouched.

## Review Focus

Input classes the sweep cannot fully prove, most likely to bite first:

1. Browser language lists like `["hr-HR", "en"]`, `["HR-hr"]`, `["en", "hr"]` - the first `hr` prefix wins, case-insensitive. Pinned by `detectLanguage` tests in Task 1.
2. Croatian plural grammar at `count` = 0, 1, 2-4, 22-24, 5+ - `few` only for 2-4 (and n%10 in 2-4 with n%100 not in 12-14); `other` for 0 and 5+. Pinned by `pluralKey` tests in Task 1.
3. A `{name}` parameter missing from the params object - the raw marker must stay visible, never silently blank. Pinned by `substitute` test in Task 1.
4. A sync payload from an older peer without `language`, or with `language: "de"` - the other settings must still sync and `language` falls back to `auto`. Pinned by sync-schema tests in Task 2.
5. The `lang` attribute must not stay `"hr"` for English browsers after load. Pinned by the manual dev-server check in Task 4 (inspect `documentElement.lang` in both modes).

---

### Task 1: i18n core module (helpers, dictionaries skeleton, tests, recipe)

**Files:**
- Create: `frontend/src/i18n/core.ts`
- Create: `frontend/src/i18n/en.ts`
- Create: `frontend/src/i18n/hr.ts`
- Create: `frontend/src/i18n/index.ts`
- Test: `frontend/src/i18n/core.test.ts`
- Modify: `frontend/justfile` (add `test` recipe)

**Interfaces (Produces - later tasks rely on these exact names):**

```ts
// src/i18n/core.ts
export type Language = "en" | "hr";
export type Template = string | { one?: string; few?: string; other: string };
export type TParams = Record<string, string | number>;

export function define<En extends Record<string, Template>>(value: En): En;

// pure, exported for tests
export function detectLanguage(tags: readonly string[]): Language;  // first "hr" prefix (case-insensitive) wins, else "en"
export function pluralKey(count: number, lang: Language): "one" | "few" | "other";
export function substitute(template: string, params: TParams | undefined): string;  // missing param leaves raw "{name}" marker
export function translate(value: Template, lang: Language, params: TParams | undefined): string;

// src/i18n/index.ts
export type TFn = (key: keyof typeof en, params?: TParams) => string;
export type { Language, Template, TParams } from "./core";
export function resolveLanguage(): Language;            // navigator.languages -> detectLanguage, memoized per page load
export function currentLanguage(): Language;            // settings store choice, "auto" resolved
export function makeT(lang: Language): TFn;             // hr dictionary value ?? en value -> translate
export function makeNodeT(lang: Language): TNodeFn;     // same lookup, params may be ReactNode
export type TNodeFn = (key: keyof typeof en, params?: Record<string, ReactNode>) => ReactNode[];
export function formattingLocale(lang: Language): string | undefined; // "hr-HR" or undefined (device default)
export function useLanguage(): Language;                // useSetting("language") + resolveLanguage
export function useT(): TFn;                            // makeT(useLanguage())
export function useNodeT(): TNodeFn;                    // makeNodeT(useLanguage())
export function getT(): TFn;                            // makeT(currentLanguage()), for non-React call sites
```

`tNode` in the spec maps to `makeNodeT`/`useNodeT` here; there is no separate standalone `tNode` export.

- [ ] **Step 1: Write the failing tests** in `core.test.ts` (`bun:test` - import from `./core`):

```ts
import { describe, test } from "node:test";
import assert from "node:assert/strict";
import { detectLanguage, pluralKey, substitute, translate } from "./core";

function eq(actual: unknown, expected: unknown) {
  assert.equal(actual, expected);
}

describe("detectLanguage", () => {
  test("picks hr from prefixed regional tag", () => eq(detectLanguage(["hr-HR", "en"]), "hr"));
  test("is case-insensitive", () => eq(detectLanguage(["HR-hr"]), "hr"));
  test("takes the first matching entry", () => eq(detectLanguage(["en-US", "hr"]), "en"));
  test("falls back to en", () => eq(detectLanguage(["de-DE", "sl-SI"]), "en"));
  test("empty list falls back to en", () => eq(detectLanguage([]), "en"));
});

describe("pluralKey", () => {
  test("en: one only for 1", () => {
    eq(pluralKey(1, "en"), "one");
    eq(pluralKey(0, "en"), "other");
    eq(pluralKey(2, "en"), "other");
  });
  test("hr: 1, 21 one (n%10 == 1 except 11); 2-4 few", () => {
    eq(pluralKey(1, "hr"), "one");
    eq(pluralKey(21, "hr"), "one");
    eq(pluralKey(2, "hr"), "few");
    eq(pluralKey(4, "hr"), "few");
  });
  test("hr: 11-14 other, 22-24 few", () => {
    eq(pluralKey(12, "hr"), "other");
    eq(pluralKey(14, "hr"), "other");
    eq(pluralKey(22, "hr"), "few");
    eq(pluralKey(24, "hr"), "few");
  });
  test("hr: 0 and 5+ other", () => {
    eq(pluralKey(0, "hr"), "other");
    eq(pluralKey(5, "hr"), "other");
  });
});

describe("substitute", () => {
  test("replaces named params in order", () => eq(substitute("{count} polazaka, {stop}", { count: 5, stop: "Kvaternikov trg" }), "5 polazaka, Kvaternikov trg"));
  test("missing param leaves the marker", () => eq(substitute("{name} line", undefined), "{name} line"));
  test("renders numbers", () => eq(substitute("{count} min", { count: 3 }), "3 min"));
});

describe("translate", () => {
  test("plain template", () => eq(translate("Share", "en", undefined), "Share"));
  test("plural object picks hr form and falls back to other when a form is absent", () => {
    const one = { one: "1 polazak", other: "{count} polazaka" };
    eq(translate(one, "hr", { count: 1 }), "1 polazak");
    eq(translate(one, "hr", { count: 3 }), "3 polazaka"); // missing "few" -> other, still substituted
    eq(translate({ one: "1 departure", few: "{count} polaska", other: "{count} polazaka" }, "hr", { count: 3 }), "3 polaska");
  });
  test("plural object picks en form and treats missing count as 0 (other)", () => {
    eq(translate({ one: "{count} departure", other: "{count} departures" }, "en", { count: 1 }), "1 departure");
    eq(translate({ one: "{count} departure", other: "{count} departures" }, "en", undefined), "0 departures");
  });
});
```

Tests use `node:test` + `node:assert/strict` (typed by the existing `@types/node`, no new dependency); bun runs `node:test` files natively.

- [ ] **Step 2: Run to verify they fail**

Run: `just frontend test`
Expected: FAIL - `core.ts` does not exist yet. Add the recipe first (Step 8 depends on it existing for later steps; create it now):

```just
# frontend/justfile addition (alphabetical placement matching existing style)
test:
    bun test src
```

- [ ] **Step 3: Implement `core.ts`**

`pluralKey` wraps `new Intl.PluralRules(lang).select(count)` and maps result to `"one" | "few" | "other"` (CLDR already yields those categories for `en`/`hr`; 22-24 are `few` in hr via CLDR decimals - trust `Intl`, the tests pin the behavior). `substitute` replaces every `{name}` occurrence; missing params leave markers. `translate` dispatches string vs plural object; `pluralKey(count)` with no params means count resolves to 0. `detectLanguage` returns `"hr"` when any tag lowercased starts with `"hr"`, else `"en"`. `define` is the identity function that only feeds inference.

- [ ] **Step 4: Create dictionary skeletons**

`en.ts` and `hr.ts` both call `define({})` for now (groups are added per sweep task). `hr.ts` is typed `define<typeof en>({ ... })` with `en` imported from `./en`.

- [ ] **Step 5: Implement `index.ts`** with the exact exported surface from the Interfaces block. Imports: `en`, `hr`, `./core`, `useSetting` from `@/settings`, `react`. `resolveLanguage` memoizes a module-level `let cached: Language | null = null`.

- [ ] **Step 6: Run tests**

Run: `just frontend test`
Expected: PASS (all core tests green).

- [ ] **Step 7: Run `just frontend check`**

Expected: PASS - dictionaries are empty, nothing consumes the API yet; this proves the module type-checks on its own.

- [ ] **Step 8: Commit**

```bash
git add frontend/justfile frontend/src/i18n
git commit -m "Add typed i18n template helper without dependencies"
```

### Task 2: `language` setting with validation and sync fallback

**Files:**
- Modify: `frontend/src/settings.ts`
- Test: `frontend/src/settings.test.ts`

**Interfaces:**
- Consumes: nothing new from Task 1 (the literal union `"auto" | "en" | "hr"` is local to settings.ts; i18n consumes settings, never the reverse).
- Produces: `language` on `Settings` (type `LanguageChoice = "auto" | "en" | "hr"`, exported), in `DEFAULTS` (`"auto"`), persisted/validated in `loadSettings` (`isValidLanguageChoice`), and in the sync surface: `SYNC_SETTINGS_KEYS` + `serializeSync` + `syncSettingsSchema` entry `z.enum(["auto", "en", "hr"]).default("auto").catch("auto")`.

- [ ] **Step 1: Write the failing tests** in `src/settings.test.ts`:

```ts
import { describe, test } from "node:test";
import assert from "node:assert/strict";
import { syncSettingsSchema } from "@/settings";

function fullPayload(): Record<string, unknown> {
  return {
    mapStyle: "flat",
    mapThemeMode: "manual",
    wakeLockEnabled: false,
    uiThemeMode: "manual",
    uiThemeManual: "dark",
    showGbfsStations: true,
    showBuses: true,
    showTrams: false,
    language: "hr",
  };
}

describe("syncSettingsSchema", () => {
  test("accepts language values", () => {
    const parsed = syncSettingsSchema.safeParse(fullPayload());
    assert.equal(parsed.success, true);
    assert.ok(parsed.success);
    assert.equal(parsed.data.language, "hr");
  });
  test("old peer payload without language defaults to auto", () => {
    const { language: _drop, ...legacy } = fullPayload();
    const parsed = syncSettingsSchema.safeParse(legacy);
    assert.equal(parsed.success, true);
    assert.ok(parsed.success);
    assert.equal(parsed.data.language, "auto");
  });
  test("bogus language falls back to auto without breaking other fields", () => {
    const parsed = syncSettingsSchema.safeParse({ ...fullPayload(), language: "de" });
    assert.equal(parsed.success, true);
    assert.ok(parsed.success);
    assert.equal(parsed.data.language, "auto");
    assert.equal(parsed.data.mapStyle, "flat");
  });
});
```

- [ ] **Step 2: Run to verify they fail**

Run: `just frontend test`
Expected: FAIL - `language` is not part of the schema yet (TS may also flag `language` on the object literal).

- [ ] **Step 3: Implement in `src/settings.ts`**

Following the existing per-key pattern: export `LanguageChoice`, add `DEFAULTS.language`, add `isValidLanguageChoice`, extend `loadSettings` (both return paths), add to `SYNC_SETTINGS_KEYS`, extend `syncSettingsSchema` and `serializeSync`.

- [ ] **Step 4: Run tests**

Run: `just frontend test`
Expected: PASS.

- [ ] **Step 5: Run `just frontend check`, then commit**

```bash
just frontend check
hk run pre-commit
git add frontend/src/settings.ts frontend/src/settings.test.ts
git commit -m "Add synced language setting with browser fallback"
```

### Task 3: Settings modal sweep + language switcher

**Files:**
- Modify: `frontend/src/components/settings-modal.tsx`

**Interfaces:**
- Consumes: `useT`, `useLanguage`, `Language` (Task 1); `LanguageChoice`, `useSetting`, `updateSetting` (Task 2).
- Produces: dictionary groups `settings.*` and `language.*` (e.g. `language.title`, `language.auto = "Auto (browser)"`, `language.en = "English"`, `language.hr = "Hrvatski"`, plus a description line per choice: e.g. `language.autoDescription`).

- [ ] **Step 1: Add the switcher section**

Follow the existing `SettingsCategory`/`ItemSelect` radio pattern. Placement: its own category adjacent to the existing appearance/behavior categories. Three options control `updateSetting("language", ...)` with `"auto" | "en" | "hr"`; the active option mirrors `useSetting("language")`. Croatian option label is `"Hrvatski"` in both languages (native names convention), per spec.

- [ ] **Step 2: Extract the modal's user-visible strings**

Move every user-visible string in the file into `en.ts` (verbatim) and `hr.ts` (translated). Includes: category titles, item titles/descriptions, `OnOffSwitch` fallbacks `"Visible"`, `"Hidden"`, `"On"`, `"Off"`, aria-labels, toasts. Key grouping by area, e.g. `settings.categories.*`, `settings.map.*`, `settings.theme.*`, `settings.visibility.*`, `language.*`. Prettier formats the dictionary files; keep keys alphabetically sorted inside each group (existing convention risk: none - new file).

- [ ] **Step 3: Verify and commit**

Run: `just frontend check && just frontend test`
Expected: PASS. Manual eyeball: open the modal in `en` and `hr` via dev server, toggle a few options.

```bash
hk run pre-commit
git add frontend/src
git commit -m "Add language switcher and translate settings modal"
```

### Task 4: Language wiring - document lang, noscript, formatting hook helper

**Files:**
- Modify: `frontend/index.html`
- Modify: `frontend/src/app.tsx` (effect only; string sweep happens in Task 6)

**Interfaces:**
- Consumes: `useLanguage` (Task 1).
- Produces: `<html lang="hr">` initial value and the correction effect; static bilingual `<noscript>` line.

- [ ] **Step 1: `index.html`**

Change `<html lang="hr-HR">` to `<html lang="hr">` (matching the unit-tested `hr` tag convention; the effect corrects to `en` immediately). Replace the `<noscript>` inner text with a single bilingual line: `Za koristenje aplikacije potreban je JavaScript. JavaScript is required to use ZET Live.` (note: this is static text outside the dictionaries - write `kori\u{0161}tenje` with a real diacritic in the file; ASCII rule applies to prose, Croatian diacritics are allowed). No inline script machinery.

- [ ] **Step 2: Add the correction effect in `app.tsx`**

Inside the existing top-level App component body: `const language = useLanguage();` and an effect setting `document.documentElement.lang = language` on change. No other behavior.

- [ ] **Step 3: Verify and commit**

Run: `just frontend check`
Manual: dev server, browser in hr and en modes - check `document.documentElement.lang` flips.

```bash
hk run pre-commit
git add frontend/index.html frontend/src/app.tsx
git commit -m "Sync html lang attribute with UI language"
```

### Task 5: Sweep batch A - status chrome (time utils, status bar, loading screen, line badge)

**Files:**
- Modify: `frontend/src/utils/time.ts`
- Modify: `frontend/src/components/status-bar.tsx`
- Modify: `frontend/src/components/loading-screen.tsx`
- Modify: `frontend/src/components/line-badge.tsx`

**Interfaces:**
- Consumes: `useT`, `makeT`/`getT` if needed, `formattingLocale` (Task 1).
- Produces: groups `status.*` (`status.connected`, `status.disconnected`, `status.lastUpdate`, `status.justNow`, `status.never`, `status.secondsAgo`...), `time.*` (`time.now`, `time.minutes`, plural), `loading.*`, `line.*` - plus a locale-aware `formatClockTime(date, locale?)` signature: adds an optional `locale: string | undefined` parameter, defaulting to `undefined` (device); callers switch to `formattingLocale(useLanguage())` in Tasks 5-7. `formatMinutesFromNow(dateTime, templateParts?)` becomes locale-owning: replace its literals with keys `time.now` / `time.minutes` using `getT()` inside the util (it is called from sheets without hook context).

- [ ] **Step 1: Move every user-visible string from these four files into `en.ts` verbatim and translate into `hr.ts`**

Known anchors (the sweep may find more): `formatAgo`'s `"just now"`, `"${seconds}s ago"`, `"${minutes}m ago"`, `"${...}h ago"`, `"never"`; `"Connection status"` + `"Connection status details"` aria; `"Connected"` / `"Disconnected"`; `"Last update: {ago}"` (template with `{ago}`); `formatMinutesFromNow`'s `"now"`, `"1 min"`, `"${minutes} min"`. The status bar's `formatAgo` moves to dictionary keys; the "never" case becomes a key too. `lastError` text is rendered as-is (backend English strings stay). Loading screen: brand text stays; other literals translate.

- [ ] **Step 2: Verify and commit**

Run: `just frontend check && just frontend test`
Manual eyeball in both languages: status bar expands, "Last update" and clock text read naturally in Croatian.

```bash
hk run pre-commit
git add frontend/src
git commit -m "Translate status bar and time helpers"
```

### Task 6: Sweep batch B - app shell (app.tsx, bottom sheet, sheet actions)

**Files:**
- Modify: `frontend/src/app.tsx`
- Modify: `frontend/src/components/bottom-sheet.tsx`
- Modify: `frontend/src/components/sheet-actions.tsx`

**Interfaces:**
- Consumes: `useT`, `useLanguage`, `useNodeT` if needed (Task 1); `formattingLocale` (Task 1) for the `<time title>` fallback (`app.tsx:221`).
- Produces: groups `app.*` (`app.currentStatus = "Trenutno Stanje"` / hr keeps `Trenutno Stanje`... careful: the key `documentSubject` default sits here), `documentSubject.*` (`default`, `stop`, `bikes`), `actions.*` (`share`, `center`, `fitAll`), `minimized.*` (`vehicleNextStop`, `stopNextDeparture`, `arrivalNow`, `minutesShort`).

- [ ] **Step 1: Translate `documentSubject` fragments**

Per owner decision: en `"Current status"`, hr `"Trenutno Stanje"`. Stop suffix: en `"{name} (stop)"`, hr `"{name} (stanica)"` (verbatim fragment). GBFS: en `"{name} (bike)"`, hr `"{name} [Bajs]"` (verbatim fragment). Vehicle subject: en and hr both `"{routeId} {displayName}"` (no literal; interpolated). `documentTitle` composition stays.

- [ ] **Step 2: Translate the minimized sheet sentence with `tNode`**

The sentence `Next stop{stopLabel} arriving {timeLabel}` with styled infixes (`app.tsx:209-234`) becomes a `useNodeT` key per language owning full word order, e.g. en template `Next stop{stop} arriving {time}`, hr template `Sljedeca stanica{stop}, dolazak {time}` - keys `minimized.vehicleNextStop` with the `{stop}`/`{time}` holes filled by the existing `stopLabel`/`timeLabel` React fragments (keep their internals, including `minutesStr` selection, translated via `time.*` keys). The stop variant `Route {routeId} in {label}` (`app.tsx:258`) becomes a template key with `{route}` and `{time}` holes. `sheetActions` labels (`"Share"`, `"Center"`, `"Fit all"`) and any aria-labels/toasts in `bottom-sheet.tsx` and `sheet-actions.tsx` move to keys.

- [ ] **Step 3: Verify and commit**

Run: `just frontend check && just frontend test`
Manual: select a vehicle and a stop, minimize, check the sentence reads correctly in both languages (Node order check: the styled infixes must land inside the sentence grammatically in hr).

```bash
hk run pre-commit
git add frontend/src
git commit -m "Translate app shell and minimized sheet"
```

### Task 7: Sweep batch C - sheets and departures

**Files:**
- Modify: `frontend/src/components/vehicle-sheet.tsx`
- Modify: `frontend/src/components/gbfs-station-sheet.tsx`
- Modify: `frontend/src/components/stop-sheet.tsx`
- Modify: `frontend/src/components/departures-board.tsx`
- Modify: `frontend/src/components/grouped-stop-departures.tsx`
- Modify: `frontend/src/components/notice-bar.tsx`

**Interfaces:**
- Consumes: `useT`, `formattingLocale` (Task 1); `formatClockTime(date, locale)` (Task 5) for `vehicle-sheet.tsx:57` and departure time columns.
- Produces: groups `vehicle.*`, `station.*`, `stop.*`, `departures.*`, `notice.*`.

- [ ] **Step 1: Move every user-visible string (JSX text, toasts, aria-labels, placeholders, titles) into `en.ts` verbatim + `hr.ts`**

Known anchors: `"No more departures today"`, `"No active vehicles"`, `"Schedule unavailable"`, `"No results found"` (search-bar in Task 8), date formatting at `feedback-modal.tsx:410` stays Task 9. Departure time stamps format with the active locale. Notice text itself (feed data) displays as-is.

- [ ] **Step 2: Verify and commit**

Run: `just frontend check && just frontend test`
Manual: open vehicle sheet, stop sheet, bike station sheet in both languages.

```bash
hk run pre-commit
git add frontend/src
git commit -m "Translate vehicle, stop, and bike sheets"
```

### Task 8: Sweep batch D - map, search, auth button

**Files:**
- Modify: `frontend/src/components/map-container.tsx`
- Modify: `frontend/src/components/search-bar.tsx`
- Modify: `frontend/src/components/auth-button.tsx`
- Modify: `frontend/src/components/avatar.tsx`

**Interfaces:**
- Consumes: `useT` (Task 1).
- Produces: groups `map.*`, `search.*`, `auth.*`.

- [ ] **Step 1: Move every user-visible string into `en.ts` verbatim + `hr.ts`**

Known anchors: `"No results found"`; map control aria-labels/tooltips (center/zoom/fit); sign-in/out button labels. MapLibre built-in control strings are untouched.

- [ ] **Step 2: Verify and commit**

Run: `just frontend check && just frontend test`
Manual: search with no matches, map controls focus-visible in both languages.

```bash
hk run pre-commit
git add frontend/src
git commit -m "Translate map, search, and auth button"
```

### Task 9: Sweep batch E - big modals (auth, feedback)

**Files:**
- Modify: `frontend/src/components/auth-modal.tsx`
- Modify: `frontend/src/components/feedback-modal.tsx`

**Interfaces:**
- Consumes: `useT`, `formattingLocale` (Task 1); `formatClockTime(date, locale)` (Task 5).
- Produces: groups `auth.*`, `feedback.*`.

- [ ] **Step 1: Move every user-visible string into `en.ts` verbatim + `hr.ts`**

Known anchors: session row date at `auth-modal.tsx:405` (`toLocaleString()` - pass the active locale); feedback date grouping at `feedback-modal.tsx:410` (`toLocaleDateString(undefined, { options })` - locale argument becomes `formattingLocale(useLanguage())`); form labels, placeholders, validation hints, toasts. The `"IP unknown"` literal at `auth-modal.tsx:405` becomes a key.

- [ ] **Step 2: Verify and commit**

Run: `just frontend check && just frontend test`
Manual: open both modals end-to-end in hr (validating feedback submit is not required - label language is the check).

```bash
hk run pre-commit
git add frontend/src
git commit -m "Translate auth and feedback modals"
```

### Task 10: Non-React call sites (entity fallbacks) + residual strings

**Files:**
- Modify: `frontend/src/app/entity/v1/vehicle.ts`
- Modify: `frontend/src/app/entity/v1/gbfs-station.ts`
- Modify: any other non-component files the residual grep finds (hooks, utils, stores) - decide per hit with the same key-adding flow

**Interfaces:**
- Consumes: `getT` (Task 1).
- Produces: keys `entity.routeFallback = "Route {id}"` (en) / `"{id}"`-order per hr wording; `entity.stationFallback = "Station {id}"`.

- [ ] **Step 1: Replace the fallback literals**

`vehicle.ts:127` becomes `getT()("entity.routeFallback", { id: this.routeId })`, `gbfs-station.ts:83` similarly. The entity method signatures do not gain language parameters (the store-read inside `getT` resolves the choice at call time).

- [ ] **Step 2: Residual grep for untranslated literals**

Run: `grep -rn "toast\.\|is required\|fallback\|Label\|aria-label" frontend/src/hooks frontend/src/utils frontend/src/store.ts frontend/src/data 2>/dev/null` and read each hit's context. Any user-visible literal found: move to `en.ts` + `hr.ts` (consumer decides hook vs `getT`). Non-user-visible constants stay.

- [ ] **Step 3: Verify and commit**

Run: `just frontend check && just frontend test`

```bash
hk run pre-commit
git add frontend/src
git commit -m "Translate entity fallback names"
```

### Task 11: Final verification and rebuild

**Files:** none new.

- [ ] **Step 1: Full checks**

Run: `just frontend check && just frontend test && just frontend build`
Expected: PASS.

- [ ] **Step 2: Backend check with rebuilt frontends**

Run: `just backend check`
Expected: PASS (embedded dists rebuild for the Rust snapshot).

- [ ] **Step 3: Commit any formatting fallout**

If Step 1 or 2 produced fixes, commit them:

```bash
hk run pre-commit
git add -A && git commit -m "Polish translations branch"
git status --short
```
Expected: clean tree. (Skip the commit if the tree is already clean.)

## Out of Scope

- Admin frontend, backend error strings, feed data, MapLibre control internals, static SEO strings (title/meta/og/twitter/ld+json stay Croatian).
- An `og:title` update on language change beyond what the existing `documentTitle` sync already does (cosmetic per owner decision).
- A third language.
