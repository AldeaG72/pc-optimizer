// Sample answers for when the interface runs outside the app (tests, browser previews).
// Nothing here touches the PC.

import type { BenchGame, HelperEvent, HelperRequest, HomeView, Recipe, Series, Verdict } from "./backend";

/** A laptop like the owner's (Stage 1 data), in Romanian. */
export function sampleHome(): HomeView {
  return {
    hardware: [
      { kind: "cpu", label: "Procesor", value: "Intel Core i5-1245U", detail: "10 nuclee, 12 fire" },
      { kind: "gpu", label: "Placă video", value: "Intel Iris Xe Graphics (integrată)", detail: "driver 32.0.101.7088, din 2026-06-17" },
      { kind: "memory", label: "Memorie", value: "16 GB", detail: "2 module, 3200 MT/s" },
      { kind: "display", label: "Monitor", value: "1920 × 1200, 60 Hz", detail: "Built-in display" },
      { kind: "power", label: "Alimentare", value: "laptop, pe baterie", detail: "plan „Balanced”" },
    ],
    findings: [
      {
        check: "on_battery",
        outcome: "warning",
        subject: null,
        title: "Laptop pe baterie",
        message: "Pe baterie, Windows reduce viteza procesorului și a plăcii video.",
        fix: "Conectează încărcătorul când joci.",
        link: null,
        recipe: null,
      },
      {
        check: "display_refresh",
        outcome: "problem",
        subject: "Monitor 2",
        title: "Frecvența monitorului",
        message: "Monitor 2 merge la 60 Hz din 144 Hz.",
        fix: "Setări › Afișaj › Afișaj avansat.",
        link: null,
        recipe: null,
      },
    ],
    ok: ["TRIM", "Spațiu liber"],
    unknown: [],
    laptop: true,
    displays: [
      { name: "Built-in display", primary: true },
      { name: "Monitor 2", primary: false },
    ],
    probeErrors: [],
  };
}
import type { Lang } from "./i18n";

export function sampleRecipes(lang: Lang): Recipe[] {
  const ro = lang === "ro";
  return [
    {
      id: "gaming.game-dvr-off",
      name: ro ? "Captură în fundal (Game DVR) oprită" : "Background capture (Game DVR) off",
      description: ro
        ? "Oprește înregistrarea video automată din Xbox Game Bar."
        : "Turns off automatic background recording in Xbox Game Bar.",
      category: "gaming",
      risk: "safe",
      profiles: ["recommended", "laptop"],
      impact: ro ? "Mediu pe plăci video slabe" : "Medium on weak GPUs",
      why: ro ? "Consumă resurse chiar dacă nu o folosești." : "Uses resources even if unused.",
      breaks: ro ? "Win+Alt+R nu mai funcționează." : "Win+Alt+R stops working.",
      restart: "none",
      sources: ["https://support.microsoft.com/windows/record-a-game-clip-on-your-pc-with-xbox-game-bar"],
      verified: false,
      status: { code: "not_applied" },
      changes: [
        {
          target: "HKCU\\System\\GameConfigStore\\GameDVR_Enabled",
          current: { kind: "missing" },
          desired: { kind: "value", text: "0" },
          inPlace: false,
        },
      ],
      needsAdmin: false,
    },
    {
      id: "services.fax-disabled",
      name: ro ? "Serviciul Fax oprit" : "Fax service disabled",
      description: ro ? "Oprește serviciul Fax." : "Disables the Fax service.",
      category: "services",
      risk: "safe",
      profiles: [],
      impact: ro ? "Mic" : "Low",
      why: ro ? "Nu e folosit pe PC-urile de gaming." : "Unused on gaming PCs.",
      breaks: ro ? "Trimiterea de faxuri." : "Sending faxes.",
      restart: "none",
      sources: [],
      verified: false,
      status: { code: "not_applicable", reason: { code: "service_missing", name: "Fax" } },
      changes: [],
      needsAdmin: true,
    },
  ];
}

/** Sample benchmark results kept in memory (what a real journal would hold). */
export const sampleBench = {
  before: null as number[] | null,
  after: null as number[] | null,
  cancelled: false,
};

export function sampleCancel(): void {
  sampleBench.cancelled = true;
}

export function sampleGames(): BenchGame[] {
  return [
    { key: "cs2", name: "Counter-Strike 2", installed: true, canLaunch: true, precise: true },
    { key: "valorant", name: "VALORANT", installed: true, canLaunch: true, precise: false },
    { key: "fortnite", name: "Fortnite", installed: false, canLaunch: false, precise: true },
  ];
}

export function sampleStatus(target: string | null): { verdict: Verdict | null; openBefore: string | null } {
  const avg = (xs: number[]) => xs.reduce((a, b) => a + b, 0) / xs.length;
  const summary = (xs: number[]) => ({
    runs: xs.length,
    avg_fps: avg(xs),
    low1_fps: avg(xs) * 0.6,
    avg_frametime_ms: 1000 / avg(xs),
    fps_spread: Math.max(...xs) - Math.min(...xs),
  });
  const { before, after } = sampleBench;
  let verdict: Verdict | null = null;
  if (target && before && after) {
    const b = summary(before);
    const a = summary(after);
    const change = ((a.avg_fps - b.avg_fps) / b.avg_fps) * 100;
    const drop = change < -3 && b.avg_fps - a.avg_fps > Math.max(b.fps_spread, a.fps_spread);
    verdict = {
      target,
      comparison: {
        before: b,
        after: a,
        fps_change_pct: change,
        low1_change_pct: change,
        significant_drop: drop,
      },
      kind: drop ? "drop" : change >= 1 ? "better" : "same",
      undoIds: drop ? ["gaming.game-dvr-off"] : [],
    };
  }
  return { verdict, openBefore: before && !after ? "cs2" : null };
}

function sampleBenchEvents(request: Extract<HelperRequest, { op: "bench" }>): HelperEvent[] {
  const fps = request.label === "before" ? [140, 141, 142] : [150, 151, 152];
  const events: HelperEvent[] = [{ event: "hello", version: 1 }];
  const done: number[] = [];
  for (let run = 1; run <= request.runs; run++) {
    events.push({ event: "bench_waiting_key", run, runs: request.runs });
    if (sampleBench.cancelled) break;
    events.push({ event: "bench_measuring", run, seconds: request.duration });
    const f = fps[(run - 1) % fps.length] ?? 140;
    done.push(f);
    events.push({
      event: "bench_run",
      run,
      metrics: { frames: f * 60, seconds: 60, avg_fps: f, low1_fps: f * 0.6, avg_frametime_ms: 1000 / f },
    });
  }
  sampleBench.cancelled = false;
  if (done.length) sampleBench[request.label] = done;
  events.push({ event: "bench_finished", series: 1, runs: done.length });
  events.push({ event: "done", restart: "none" });
  return events;
}

export function sampleEvents(request: HelperRequest): HelperEvent[] {
  if (request.op === "bench") return sampleBenchEvents(request);
  const ids = request.op === "undo_all" ? [] : request.ids;
  const undo = request.op !== "apply";
  return [
    { event: "hello", version: 1 },
    ...ids.flatMap((id): HelperEvent[] => [
      { event: "started", id },
      {
        event: "result",
        id,
        outcome: undo ? { code: "undone", restored: 1 } : { code: "applied", changed: 1 },
      },
    ]),
    { event: "done", restart: "none" },
  ];
}

export function sampleHistory(): Series[] {
  const series = (id: number, label: "before" | "after", fps: number[] | null): Series[] =>
    fps
      ? [
          {
            id,
            target: "cs2",
            label,
            runs: fps.map((f) => ({ frames: f * 60, seconds: 60, avg_fps: f, low1_fps: f * 0.6, avg_frametime_ms: 1000 / f })),
          },
        ]
      : [];
  return [...series(2, "after", sampleBench.after), ...series(1, "before", sampleBench.before)];
}

export function sampleForget(): number {
  const n = (sampleBench.before?.length ?? 0) + (sampleBench.after?.length ?? 0);
  sampleBench.before = null;
  sampleBench.after = null;
  return n;
}