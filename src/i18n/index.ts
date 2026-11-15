import { useSyncExternalStore } from "react";
import en from "./locales/en.json";
import es from "./locales/es.json";
import fr from "./locales/fr.json";
import de from "./locales/de.json";
import ptBR from "./locales/pt-BR.json";
import it from "./locales/it.json";
import ru from "./locales/ru.json";
import pl from "./locales/pl.json";
import ja from "./locales/ja.json";
import ko from "./locales/ko.json";
import zhCN from "./locales/zh-CN.json";

export const LOCALES = [
  "en",
  "es",
  "fr",
  "de",
  "pt-BR",
  "it",
  "ru",
  "pl",
  "ja",
  "ko",
  "zh-CN",
] as const;

export type LocaleId = (typeof LOCALES)[number];

export type Catalog = { [K in keyof typeof en]: string };

const esCatalog: Catalog = es;
const frCatalog: Catalog = fr;
const deCatalog: Catalog = de;
const ptBRCatalog: Catalog = ptBR;
const itCatalog: Catalog = it;
const ruCatalog: Catalog = ru;
const plCatalog: Catalog = pl;
const jaCatalog: Catalog = ja;
const koCatalog: Catalog = ko;
const zhCNCatalog: Catalog = zhCN;

export const CATALOGS: Record<LocaleId, Catalog> = {
  en,
  es: esCatalog,
  fr: frCatalog,
  de: deCatalog,
  "pt-BR": ptBRCatalog,
  it: itCatalog,
  ru: ruCatalog,
  pl: plCatalog,
  ja: jaCatalog,
  ko: koCatalog,
  "zh-CN": zhCNCatalog,
};

/** Native names shown in the language picker. Not looked up from catalogs. */
export const LOCALE_NATIVE_NAMES: Record<LocaleId, string> = {
  en: "English",
  es: "Español",
  fr: "Français",
  de: "Deutsch",
  "pt-BR": "Português (Brasil)",
  it: "Italiano",
  ru: "Русский",
  pl: "Polski",
  ja: "日本語",
  ko: "한국어",
  "zh-CN": "简体中文",
};

export const INTL_LOCALE: Record<LocaleId, string> = {
  en: "en",
  es: "es",
  fr: "fr",
  de: "de",
  "pt-BR": "pt-BR",
  it: "it",
  ru: "ru",
  pl: "pl",
  ja: "ja",
  ko: "ko",
  "zh-CN": "zh-CN",
};

const STORAGE_KEY = "secretfolder.uiLocale";

export type Vars = Record<string, string | number>;

export function isLocaleId(value: string): value is LocaleId {
  return (LOCALES as readonly string[]).includes(value);
}

function canonicalize(tag: string): string {
  return tag.trim().replace(/_/g, "-");
}

/**
 * First supported language wins. Traditional Chinese is not shipped, so
 * zh-TW / zh-HK / zh-Hant / zh-MO are skipped and the next tag is tried.
 */
export function detectLocale(languages: readonly string[] | undefined): LocaleId {
  const list = languages && languages.length > 0 ? languages : ["en"];
  for (const raw of list) {
    const tag = canonicalize(raw);
    if (!tag) continue;
    const lower = tag.toLowerCase();
    const exact = LOCALES.find((id) => id.toLowerCase() === lower);
    if (exact) return exact;
    const primary = lower.split("-")[0] ?? "";
    if (primary === "pt") return "pt-BR";
    if (primary === "zh") {
      const traditional =
        lower === "zh-tw" ||
        lower === "zh-hk" ||
        lower === "zh-mo" ||
        lower === "zh-hant" ||
        lower.includes("hant");
      if (traditional) continue;
      if (
        lower === "zh" ||
        lower.startsWith("zh-cn") ||
        lower.startsWith("zh-hans") ||
        lower.startsWith("zh-sg")
      ) {
        return "zh-CN";
      }
      continue;
    }
    const byPrimary = LOCALES.find((id) => id === primary);
    if (byPrimary) return byPrimary;
  }
  return "en";
}

function readStorage(): LocaleId | null {
  try {
    const value = localStorage.getItem(STORAGE_KEY);
    return value && isLocaleId(value) ? value : null;
  } catch {
    return null;
  }
}

function browserLanguages(): readonly string[] {
  if (typeof navigator === "undefined") return ["en"];
  if (navigator.languages && navigator.languages.length > 0) {
    return navigator.languages;
  }
  return navigator.language ? [navigator.language] : ["en"];
}

function applyDom(id: LocaleId) {
  if (typeof document !== "undefined") {
    document.documentElement.lang = id;
  }
}

let locale: LocaleId = readStorage() ?? detectLocale(browserLanguages());
applyDom(locale);

const listeners = new Set<() => void>();
let persister: ((id: LocaleId) => void) | null = null;
let hostCi = false;

function emit() {
  for (const listener of listeners) listener();
}

export function getLocale(): LocaleId {
  return locale;
}

export function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function setLocalePersister(next: ((id: LocaleId) => void) | null) {
  persister = next;
}

export function setHostCi(value: boolean) {
  hostCi = value;
}

export function getHostCi(): boolean {
  return hostCi;
}

export function setLocale(next: string, opts?: { persist?: boolean }) {
  if (!isLocaleId(next)) return;
  const changed = next !== locale;
  locale = next;
  applyDom(next);
  if (opts?.persist !== false) {
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch {
      /* private mode / tests without storage */
    }
    persister?.(next);
  }
  if (changed) emit();
}

export interface LocaleBootInfo {
  savedLocale?: string | null;
  envLocale?: string | null;
  ci?: boolean;
}

let settleReady: () => void = () => {};
export const localeReady: Promise<void> = new Promise((resolve) => {
  settleReady = resolve;
});
let readySettled = false;

function settle() {
  if (readySettled) return;
  readySettled = true;
  settleReady();
}

/**
 * Disk prefs win. Do not persist an OS guess before this runs, or a wiped
 * webview profile would overwrite a saved ui.json locale.
 */
export function hydrateLocale(info: LocaleBootInfo | null) {
  try {
    const saved =
      info?.savedLocale && isLocaleId(info.savedLocale) ? info.savedLocale : null;
    const env =
      info?.envLocale && isLocaleId(info.envLocale) ? info.envLocale : null;
    const stored = readStorage();
    if (saved) {
      setLocale(saved, { persist: true });
    } else if (env && !stored) {
      setLocale(env, { persist: true });
    } else if (!stored) {
      setLocale(locale, { persist: true });
    } else {
      setLocale(stored, { persist: false });
    }
  } finally {
    settle();
  }
}

function lookup(id: LocaleId, key: string): string | undefined {
  const catalog = CATALOGS[id] as Record<string, string>;
  const value = catalog[key];
  if (typeof value === "string" && value.length > 0) return value;
  return undefined;
}

export function interpolate(text: string, vars?: Vars): string {
  if (!vars) return text;
  return text.replace(/\{([A-Za-z0-9_]+)\}/g, (match, name: string) => {
    const value = vars[name];
    return value == null ? match : String(value);
  });
}

export function translate(id: LocaleId, key: string, vars?: Vars): string {
  const text =
    lookup(id, key) ?? lookup("en", key) ?? (key.length > 0 ? key : "…");
  return interpolate(text, vars);
}

export function formatNumber(id: LocaleId, n: number): string {
  const value = Number.isFinite(n) ? n : 0;
  try {
    return new Intl.NumberFormat(INTL_LOCALE[id]).format(value);
  } catch {
    return String(value);
  }
}

export function tPlural(id: LocaleId, base: string, n: number): string {
  const value = Number.isFinite(n) ? n : 0;
  let rule: Intl.LDMLPluralRule = "other";
  try {
    rule = new Intl.PluralRules(INTL_LOCALE[id]).select(value);
  } catch {
    rule = "other";
  }
  const text =
    lookup(id, `${base}.${rule}`) ??
    lookup(id, `${base}.other`) ??
    lookup("en", `${base}.${rule}`) ??
    lookup("en", `${base}.other`) ??
    `${base}.other`;
  return interpolate(text, { count: formatNumber(id, value) });
}

export function formatBytes(id: LocaleId, n: number): string {
  const bytes = Number.isFinite(n) && n > 0 ? n : 0;
  let value = bytes;
  let unit = "B";
  let digits = 0;
  if (bytes >= 1024 * 1024) {
    value = bytes / (1024 * 1024);
    unit = "MB";
    digits = 1;
  } else if (bytes >= 1024) {
    value = bytes / 1024;
    unit = "KB";
    digits = 1;
  }
  let num: string;
  try {
    num = new Intl.NumberFormat(INTL_LOCALE[id], {
      minimumFractionDigits: digits,
      maximumFractionDigits: digits,
    }).format(value);
  } catch {
    num = digits ? value.toFixed(digits) : String(Math.round(value));
  }
  return `${num} ${unit}`;
}

export function formatDateTime(id: LocaleId, iso: string | null | undefined): string | null {
  if (!iso) return null;
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return null;
  try {
    return new Intl.DateTimeFormat(INTL_LOCALE[id], {
      dateStyle: "medium",
      timeStyle: "short",
    }).format(date);
  } catch {
    return null;
  }
}

export function useI18n() {
  const current = useSyncExternalStore(subscribe, getLocale, getLocale);
  return {
    locale: current,
    t: (key: string, vars?: Vars) => translate(current, key, vars),
    plural: (base: string, n: number) => tPlural(current, base, n),
  };
}

export function resetI18nForTests() {
  try {
    localStorage.removeItem(STORAGE_KEY);
  } catch {
    /* ignore */
  }
  persister = null;
  hostCi = false;
  const next: LocaleId = "en";
  const changed = locale !== next;
  locale = next;
  applyDom(next);
  if (changed) emit();
}
