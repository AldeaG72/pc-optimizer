// What the whole window shares: language, theme, the open page, the sidebar.
import type { Lang } from "./i18n";
import { systemLang, textsFor } from "./texts";

export type Page = "home" | "tweaks" | "settings";
export type Theme = "system" | "light" | "dark";

export const ui = $state({
  lang: systemLang() as Lang,
  theme: "system" as Theme,
  page: "home" as Page,
  collapsed: false,
});

/** The text for `key` in the current language. */
export function t(key: string, args?: Record<string, string | number>): string {
  return textsFor(ui.lang).t(key, args);
}
