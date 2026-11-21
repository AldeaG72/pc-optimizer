// The interface's only way to the engine: read commands, plus one command that starts the
// elevated helper (UAC) and streams its events. Shapes mirror app/src-tauri/src/views.rs
// and crates/elevated-helper/src/protocol.rs.
// Outside the app (tests, browser previews) it answers with sample data and changes nothing.

import type { Lang } from "./i18n";
import { isTauri } from "./window";
import { sampleEvents, sampleHome, sampleRecipes } from "./sample";

export type Category =
  | "windows" | "gaming" | "power" | "input" | "network" | "audio" | "debloat"
  | "privacy" | "cleanup" | "gpu-nvidia" | "gpu-amd" | "services" | "advanced";
export type Risk = "safe" | "moderate" | "advanced";
export type Restart = "none" | "explorer" | "signout" | "reboot";

export type NotApplicableReason =
  | { code: "service_missing"; name: string }
  | { code: "platform"; detail: string }
  | { code: "precondition"; target: string };

export type Status =
  | { code: "not_applied" | "applied" | "already_in_place" | "modified_outside" }
  | { code: "not_applicable"; reason: NotApplicableReason }
  | { code: "error"; message: string };

export type StateView =
  | { kind: "missing" }
  | { kind: "value"; text: string }
  | { kind: "service"; start: "boot" | "system" | "automatic" | "manual" | "disabled" };

export interface Change {
  target: string;
  current: StateView;
  desired: StateView;
  inPlace: boolean;
}

export interface Recipe {
  id: string;
  name: string;
  description: string;
  category: Category;
  risk: Risk;
  profiles: ("recommended" | "laptop")[];
  impact: string;
  why: string;
  breaks: string;
  restart: Restart;
  sources: string[];
  verified: boolean;
  status: Status;
  changes: Change[];
  needsAdmin: boolean;
}

export type Outcome =
  | { code: "applied"; changed: number }
  | { code: "already_applied" }
  | { code: "not_applicable"; reason: NotApplicableReason }
  | { code: "failed"; error: string; rolled_back: boolean }
  | { code: "undone"; restored: number }
  | { code: "nothing_to_undo" };

export type BlockReason =
  | { code: "not_windows11"; build: number }
  | { code: "not_x64"; arch: string }
  | { code: "needs_admin" }
  | { code: "game_running"; processes: string[] }
  | { code: "recovery_failed"; errors: string[] }
  | { code: "journal_untrusted"; recipe: string };

export type HelperEvent =
  | { event: "hello"; version: number }
  | { event: "refused"; reason: { code: string; detail?: unknown } }
  | { event: "blocked"; reason: BlockReason }
  | { event: "recovered"; session: number; restored: number }
  | { event: "started"; id: string }
  | { event: "result"; id: string; outcome: Outcome }
  | { event: "done"; restart: Restart }
  | { event: "error"; message: string };

export type HelperRequest =
  | { op: "apply"; ids: string[] }
  | { op: "undo"; ids: string[] }
  | { op: "undo_all" };

export interface Settings {
  lang: Lang | null;
  theme: "system" | "light" | "dark";
  sidebarCollapsed: boolean;
  tourAccepted: number | null;
  restorePoint: boolean;
  dailyBackup: boolean;
  showAdvanced: boolean;
  gameMonitor: string | null;
}

export const defaultSettings = (): Settings => ({
  lang: null,
  theme: "system",
  sidebarCollapsed: false,
  tourAccepted: null,
  restorePoint: false,
  dailyBackup: false,
  showAdvanced: false,
  gameMonitor: null,
});

/** Outside the app: kept in memory only (tests set it directly). */
export const memory = { settings: { ...defaultSettings(), tourAccepted: 1 } as Settings };

export async function getSettings(): Promise<Settings> {
  return isTauri() ? call<Settings>("get_settings") : { ...memory.settings };
}

export async function saveSettings(settings: Settings): Promise<void> {
  if (isTauri()) await call<void>("save_settings", { settings });
  else memory.settings = { ...settings };
}

export async function tourVersion(): Promise<number> {
  return isTauri() ? call<number>("tour_version") : 1;
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

export async function systemLang(): Promise<Lang | null> {
  return isTauri() ? call<Lang>("system_lang") : null;
}

export async function recipes(lang: Lang): Promise<Recipe[]> {
  return isTauri() ? call<Recipe[]>("recipes", { lang }) : sampleRecipes(lang);
}

export type FindingOutcome = "problem" | "warning" | "info";

export interface HwRow {
  kind: "cpu" | "gpu" | "memory" | "display" | "disk" | "windows" | "power" | "games";
  label: string;
  value: string;
  detail: string;
}

export interface Finding {
  check: string;
  outcome: FindingOutcome;
  subject: string | null;
  title: string;
  message: string;
  fix: string;
  link: string | null;
  recipe: string | null;
}

export interface HomeView {
  hardware: HwRow[];
  findings: Finding[];
  ok: string[];
  unknown: string[];
  laptop: boolean;
  displays: { name: string; primary: boolean }[];
  probeErrors: string[];
}

/** Hardware and health findings (about a second on a real PC). */
export async function home(lang: Lang): Promise<HomeView> {
  return isTauri() ? call<HomeView>("home", { lang }) : sampleHome();
}

/**
 * Runs one request through the helper; `onEvent` gets every event as it happens.
 * Rejects with "cancelled" if the user says No at the UAC prompt.
 */
export async function runHelper(
  request: HelperRequest,
  onEvent: (e: HelperEvent) => void,
): Promise<void> {
  if (!isTauri()) {
    for (const e of sampleEvents(request)) onEvent(e);
    return;
  }
  const { Channel } = await import("@tauri-apps/api/core");
  const channel = new Channel<HelperEvent>();
  channel.onmessage = onEvent;
  await call<void>("run_helper", { request, onEvent: channel });
}
