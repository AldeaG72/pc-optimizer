// The interface's only way to the engine: read commands, plus one command that starts the
// elevated helper (UAC) and streams its events. Shapes mirror app/src-tauri/src/views.rs
// and crates/elevated-helper/src/protocol.rs.
// Outside the app (tests, browser previews) it answers with sample data and changes nothing.

import type { Lang } from "./i18n";
import { isTauri } from "./window";
import {
  sampleCancel,
  sampleEvents,
  sampleForget,
  sampleGames,
  sampleHistory,
  sampleHome,
  sampleRecipes,
  sampleStatus,
} from "./sample";

export type Category =
  | "windows" | "gaming" | "power" | "input" | "network" | "audio" | "debloat"
  | "privacy" | "cleanup" | "gpu-nvidia" | "gpu-amd" | "services" | "advanced";
export type Risk = "safe" | "moderate" | "advanced";
export type Restart = "none" | "explorer" | "signout" | "reboot";

export type NotApplicableReason =
  | { code: "service_missing"; name: string }
  | { code: "platform"; detail: string }
  | { code: "precondition"; target: string }
  | {
      code:
        | "power_plan_missing"
        | "power_setting_missing"
        | "no_network_adapter"
        | "app_missing"
        | "app_not_restorable"
        | "cleanup";
    };

export type Status =
  | { code: "not_applied" | "applied" | "already_in_place" | "modified_outside" }
  | { code: "not_applicable"; reason: NotApplicableReason }
  | { code: "error"; message: string };

export type StateView =
  | { kind: "missing" }
  | { kind: "value"; text: string }
  | { kind: "service"; start: "boot" | "system" | "automatic" | "manual" | "disabled" }
  | { kind: "plan"; guid: string }
  | { kind: "power"; ac: number; dc: number }
  | { kind: "app"; installed: boolean };

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
  /** Deletes files: a "Clean" button with a confirmation, no switch, no undo. */
  cleanup: boolean;
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
  | { event: "restore_point"; created: boolean; message: string | null }
  | { event: "bench_waiting_game"; processes: string[] }
  | { event: "bench_waiting_key"; run: number; runs: number }
  | { event: "bench_measuring"; run: number; seconds: number }
  | { event: "bench_run"; run: number; metrics: Metrics }
  | { event: "bench_finished"; series: number; runs: number }
  | { event: "done"; restart: Restart }
  | { event: "error"; message: string };

export type HelperRequest =
  | { op: "apply"; ids: string[]; restore_point?: boolean }
  | { op: "undo"; ids: string[] }
  | { op: "undo_all" }
  | { op: "bench"; target: string; label: "before" | "after"; runs: number; duration: number; warmup: number };

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

export interface Catalog {
  recipes: Recipe[];
  /** For the Laptop profile of "Recommended". */
  laptop: boolean;
}

export async function recipes(lang: Lang): Promise<Catalog> {
  return isTauri()
    ? call<Catalog>("recipes", { lang })
    : { recipes: sampleRecipes(lang), laptop: true };
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

/** "Restart now" after applying (only on the user's click). */
export async function restart(kind: Exclude<Restart, "none">): Promise<void> {
  if (isTauri()) await call<void>("restart", { kind });
}

export interface Metrics {
  frames: number;
  seconds: number;
  avg_fps: number;
  low1_fps: number;
  avg_frametime_ms: number;
}

export interface Summary {
  runs: number;
  avg_fps: number;
  low1_fps: number;
  avg_frametime_ms: number;
  fps_spread: number;
}

export interface Verdict {
  target: string;
  comparison: {
    before: Summary;
    after: Summary;
    fps_change_pct: number;
    low1_change_pct: number;
    significant_drop: boolean;
  };
  kind: "better" | "same" | "drop";
  undoIds: string[];
}

export interface BenchGame {
  key: string;
  name: string;
  installed: boolean;
  canLaunch: boolean;
  precise: boolean;
}

export async function benchGames(): Promise<BenchGame[]> {
  return isTauri() ? call<BenchGame[]>("bench_games") : sampleGames();
}

export async function launchGame(key: string): Promise<void> {
  if (isTauri()) await call<void>("launch_game", { key });
}

/** The latest verdict for `target`, and the newest "before" still waiting for its "after". */
export async function benchStatus(
  target: string | null,
): Promise<{ verdict: Verdict | null; openBefore: string | null }> {
  return isTauri() ? call("bench_status", { target }) : sampleStatus(target);
}

/** Stops a running benchmark (the runs measured so far are kept). */
export async function cancelHelper(): Promise<void> {
  if (isTauri()) await call<void>("cancel_helper");
  else sampleCancel();
}

export interface Series {
  id: number;
  target: string;
  label: "before" | "after";
  runs: Metrics[];
}

/** Benchmark history, newest first. */
export async function benchHistory(): Promise<Series[]> {
  return isTauri() ? call<Series[]>("bench_history") : sampleHistory();
}

/** Deletes the whole benchmark history; how many results were deleted. */
export async function benchForget(): Promise<number> {
  return isTauri() ? call<number>("bench_forget") : sampleForget();
}

export async function appVersion(): Promise<string> {
  return isTauri() ? call<string>("app_version") : "0.0.1";
}