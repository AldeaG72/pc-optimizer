// The translation files are bundled into the app (no network), straight from i18n/ at the
// repository root, the same files `pco` uses.
import roRaw from "../../../i18n/ro.yaml?raw";
import enRaw from "../../../i18n/en.yaml?raw";
import { Texts, type Lang } from "./i18n";

/** The files are flat `key: "text"` lines (checked by a test); the quoted part is JSON. */
export function parseTexts(raw: string): Record<string, string> {
  const map: Record<string, string> = {};
  for (const line of raw.split(/\r?\n/)) {
    const m = /^([a-z0-9_.]+):\s*(".*")\s*$/.exec(line);
    if (m && m[1] && m[2]) map[m[1]] = JSON.parse(m[2]) as string;
  }
  return map;
}

const cache: Partial<Record<Lang, Texts>> = {};

export function textsFor(lang: Lang): Texts {
  return (cache[lang] ??= new Texts(lang, parseTexts(lang === "ro" ? roRaw : enRaw)));
}

/** Romanian if Windows (and so WebView2) is in Romanian, English otherwise. */
export function systemLang(): Lang {
  const l = typeof navigator === "undefined" ? "" : navigator.language;
  return l.toLowerCase().startsWith("ro") ? "ro" : "en";
}
