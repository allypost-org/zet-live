import { createElement, Fragment, type ReactNode } from "react";
import { settingsStore, useSetting } from "@/settings";
import { detectLanguage, pluralKey, translate } from "./core";
import type { Language, TParams, Template } from "./core";
import { en } from "./en";
import { hr } from "./hr";

export type { Language, Template, TParams } from "./core";

let cached: Language | null = null;

export type TFn = (key: keyof typeof en, params?: TParams) => string;
export type TNodeFn = (key: keyof typeof en, params?: Record<string, ReactNode>) => ReactNode[];
export type UiText = string | { key: keyof typeof en };

export function renderText(value: UiText, t: TFn): string {
  return typeof value === "string" ? value : t(value.key);
}

function resolveOrChoice(choice: "auto" | Language): Language {
  return choice === "auto" ? resolveLanguage() : choice;
}

export function resolveLanguage(): Language {
  if (cached === null) cached = detectLanguage(navigator.languages);
  return cached;
}

export function currentLanguage(): Language {
  return resolveOrChoice(settingsStore.getState().language);
}

export function makeT(lang: Language): TFn {
  return (key, params) => {
    const fallback = en[key];
    return translate(lang === "hr" ? (hr[key] ?? fallback) : fallback, lang, params);
  };
}

// A plural value refers to a count: a missing count acts as 0, matching the
// string path in core. A count param can be a string or number ReactNode.
function nodeCount(params: Record<string, ReactNode> | undefined): number {
  const value = params?.count;
  if (typeof value === "number") return value;
  if (typeof value === "string") return Number(value);
  return 0;
}

export function makeNodeT(lang: Language): TNodeFn {
  return (key, params) => {
    const fallback = en[key];
    const value: Template = lang === "hr" ? (hr[key] ?? fallback) : fallback;
    const template =
      typeof value === "string"
        ? value
        : (value[pluralKey(nodeCount(params), lang)] ?? value.other);
    // A missing count still substitutes, so no raw "{count}" marker leaks.
    const nodeParams =
      params && params.count !== undefined ? params : { ...params, count: nodeCount(params) };
    return nodeParts(template, nodeParams);
  };
}

export function useLanguage(): Language {
  return resolveOrChoice(useSetting("language"));
}

export function useT(): TFn {
  return makeT(useLanguage());
}

export function useNodeT(): TNodeFn {
  return makeNodeT(useLanguage());
}

export function getT(): TFn {
  return makeT(currentLanguage());
}

// Splits a template into literal strings and React nodes. A param that is
// absent or undefined leaves its whole "{name}" marker as literal text.
const NODE_PLACEHOLDER = /\{(\w+)\}/g;

function nodeParts(template: string, params: Record<string, ReactNode> | undefined): ReactNode[] {
  if (!params) return [template];
  const parts: ReactNode[] = [];
  let cursor = 0;
  for (const match of template.matchAll(NODE_PLACEHOLDER)) {
    const raw = match[0];
    const name = match[1];
    if (raw === undefined || name === undefined) continue;
    const value = params[name];
    if (value === undefined) continue;
    if (cursor < match.index) parts.push(template.slice(cursor, match.index));
    parts.push(createElement(Fragment, { key: match.index }, value));
    cursor = match.index + raw.length;
  }
  if (cursor < template.length) parts.push(template.slice(cursor));
  return parts;
}
