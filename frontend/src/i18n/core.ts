export type Language = "en" | "hr";
export type Template = string | { one?: string; few?: string; other: string };
export type TParams = Record<string, string | number>;

export function define<En extends Record<string, Template>>(value: En): En {
  return value;
}

// First entry matching a supported language prefix (case-insensitive) decides;
// nothing else in the list and no match fall back to "en".
export function detectLanguage(tags: readonly string[]): Language {
  for (const tag of tags) {
    const lower = tag.toLowerCase();
    if (lower.startsWith("hr")) return "hr";
    if (lower.startsWith("en")) return "en";
  }
  return "en";
}

export function pluralKey(count: number, lang: Language): "one" | "few" | "other" {
  const form = new Intl.PluralRules(lang).select(count);
  return form === "one" || form === "few" ? form : "other";
}

const PLACEHOLDER = /\{(\w+)\}/g;

export function substitute(template: string, params: TParams | undefined): string {
  if (!params) return template;
  return template.replace(PLACEHOLDER, (raw, name: string) => {
    const value = params[name];
    return value === undefined ? raw : String(value);
  });
}

function countOf(params: TParams | undefined): number {
  const value = params?.count;
  if (typeof value === "number") return value;
  return typeof value === "string" ? Number(value) : 0;
}

export function translate(value: Template, lang: Language, params: TParams | undefined): string {
  if (typeof value === "string") return substitute(value, params);
  const form = value[pluralKey(countOf(params), lang)] ?? value.other;
  // A plural value always refers to a count: an absent params object gets the
  // same 0 that form selection used, so no raw "{count}" marker leaks.
  return substitute(form, params ?? { count: 0 });
}
