// Sample answers for when the interface runs outside the app (tests, browser previews).
// Nothing here touches the PC.

import type { HelperEvent, HelperRequest, HomeView, Recipe } from "./backend";

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

export function sampleEvents(request: HelperRequest): HelperEvent[] {
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
