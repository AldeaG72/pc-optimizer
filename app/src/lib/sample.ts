// Sample answers for when the interface runs outside the app (tests, browser previews).
// Nothing here touches the PC.

import type { HelperEvent, HelperRequest, Recipe } from "./backend";
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
