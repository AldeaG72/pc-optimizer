// The interface's only way to the engine: read commands, plus one command that starts the
// elevated helper (UAC) and streams its events. Shapes mirror app/src-tauri/src/views.rs
// and crates/elevated-helper/src/protocol.rs.
// Outside the app (tests, browser previews) it answers with sample data and changes nothing.

import type { Lang } from "./i18n";
import { isTauri } from "./window";
import {
  sampleCancel,
  sampleCleanSizes,
  sampleCleanUser,
  sampleStartup,
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
  | { code: "nothing_to_undo" }
  | { code: "cleaned"; freed_bytes: number; deleted: number; skipped: number }
  | { code: "repaired" };

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
  | { op: "clean"; ids: string[] }
  | { op: "startup"; changes: { id: string; enable: boolean }[] }
  | { op: "repair"; action: RepairAction }
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
  gameMode: GameModeSettings;
  startWithWindows: boolean;
}

export interface GameModeSettings {
  enabled: boolean;
  closeApps: string[];
  highPerformance: boolean;
  extraGames: string[];
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
  gameMode: { enabled: false, closeApps: [], highPerformance: false, extraGames: [] },
  startWithWindows: false,
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
export interface CleanSize {
  id: string;
  /** null: a system folder the app cannot see without administrator rights. */
  bytes: number | null;
  needsAdmin: boolean;
}

export interface CleanResult {
  id: string;
  freedBytes: number;
  deleted: number;
  skipped: number;
  error: string | null;
}

/** How much each cleanup would free. Changes nothing. */
export async function cleanupSizes(): Promise<CleanSize[]> {
  return isTauri() ? call<CleanSize[]>("cleanup_sizes") : sampleCleanSizes();
}

/** Cleans the user's own places (no UAC); system folders go through `runHelper` ("clean"). */
export async function cleanUser(ids: string[]): Promise<CleanResult[]> {
  return isTauri() ? call<CleanResult[]>("clean_user", { ids }) : sampleCleanUser(ids);
}

export interface StartupView {
  id: string;
  name: string;
  command: string;
  location: "user-run" | "machine-run" | "machine-run32" | "user-folder" | "common-folder";
  enabled: boolean;
  /** Turning it on or off shows the UAC prompt (an entry for every user). */
  needsAdmin: boolean;
  impact: "high" | "medium" | "low" | "unknown";
  advice: "off" | "keep" | null;
  /** `app.startup.what.<key>` when the program is known. */
  what: string | null;
  size: number | null;
}

/** Programs that start with Windows. Changes nothing. */
export async function startupList(): Promise<StartupView[]> {
  return isTauri() ? call<StartupView[]>("startup_list") : sampleStartup();
}

/** Builds the anonymized support log and asks where to save it; the path, or null if closed. */
export async function exportLog(lang: Lang): Promise<string | null> {
  return isTauri() ? call<string | null>("export_log", { lang }) : "pc-optimizer-log.txt";
}

/** Today's backup of the changed registry values, if the setting is on (once a day). */
export async function dailyBackup(): Promise<string | null> {
  return isTauri() ? call<string | null>("daily_backup") : null;
}

export type RepairAction = "network_reset" | "update_repair" | "system_scan";
export type Panel = "sound" | "power" | "network";

/** Size of the DirectX and driver shader caches (null: none found). Changes nothing. */
export async function shaderCacheSize(): Promise<number | null> {
  return isTauri() ? call<number | null>("shader_cache_size") : 734_003_200;
}

/** Empties the shader caches (after the interface asked); the bytes freed. */
export async function cleanShaderCache(): Promise<number> {
  return isTauri() ? call<number>("clean_shader_cache") : 734_003_200;
}

/** Opens a Windows control panel. */
export async function openPanel(kind: Panel): Promise<void> {
  if (isTauri()) await call<void>("open_panel", { kind });
}

export interface GameModeStatus {
  enabled: boolean;
  /** The game being played, while game mode is on. */
  game: string | null;
  error: string | null;
}

/** Game mode right now. */
export async function gamemodeStatus(): Promise<GameModeStatus> {
  return isTauri()
    ? call<GameModeStatus>("gamemode_status")
    : { enabled: memory.settings.gameMode.enabled, game: null, error: null };
}

/** Apps suggested in the "close while playing" list. */
export async function gamemodeSuggested(): Promise<string[]> {
  return isTauri()
    ? call<string[]>("gamemode_suggested")
    : ["onedrive.exe", "ms-teams.exe", "spotify.exe", "msedge.exe", "epicgameslauncher.exe", "eadesktop.exe"];
}

/** Called when game mode turns on or off. Returns a function that stops listening. */
export async function onGameMode(handler: (s: GameModeStatus) => void): Promise<() => void> {
  if (!isTauri()) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return listen<GameModeStatus>("gamemode", (e) => handler(e.payload));
}
