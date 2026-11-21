// What the whole window shares: language, theme, the open page, the sidebar, the saved
// settings. Settings are loaded once at start and saved whenever they change.
import type { Lang } from "./i18n";
import { systemLang as browserLang, textsFor } from "./texts";
import {
  defaultSettings,
  getSettings,
  saveSettings,
  systemLang,
  tourVersion,
  type Settings,
} from "./backend";

export type Page = "home" | "tweaks" | "settings";
export type Theme = Settings["theme"];

export const ui = $state({
  lang: browserLang() as Lang,
  theme: "system" as Theme,
  page: "home" as Page,
  collapsed: false,
  /** The first-start tour is showing (its disclaimer not accepted yet). */
  tourOpen: false,
  /** Settings have been read; until then nothing is saved. */
  loaded: false,
  /** A recipe to open on the Tweaks page (from "Fix" on Home). */
  focus: null as string | null,
});

/** Opens the Tweaks page on one recipe. */
export function openRecipe(id: string): void {
  ui.focus = id;
  ui.page = "tweaks";
}

/** The saved settings not mirrored in `ui`. */
export const prefs = $state<Settings>(defaultSettings());

let currentTour = 1;

/** The text for `key` in the current language. */
export function t(key: string, args?: Record<string, string | number>): string {
  return textsFor(ui.lang).t(key, args);
}

export async function loadSettings(): Promise<void> {
  const [saved, windowsLang, version] = await Promise.all([
    getSettings(),
    systemLang(),
    tourVersion(),
  ]);
  currentTour = version;
  Object.assign(prefs, saved);
  ui.lang = saved.lang ?? windowsLang ?? browserLang();
  ui.theme = saved.theme;
  ui.collapsed = saved.sidebarCollapsed;
  ui.tourOpen = saved.tourAccepted !== version;
  ui.loaded = true;
}

/** Everything to save, from `ui` and `prefs`. */
export function snapshot(): Settings {
  return {
    ...prefs,
    lang: ui.lang,
    theme: ui.theme,
    sidebarCollapsed: ui.collapsed,
  };
}

export async function persist(): Promise<void> {
  if (ui.loaded) await saveSettings(snapshot());
}

/** "Sunt de acord" at the end of the tour. */
export async function acceptTour(): Promise<void> {
  prefs.tourAccepted = currentTour;
  ui.tourOpen = false;
  await persist();
}
